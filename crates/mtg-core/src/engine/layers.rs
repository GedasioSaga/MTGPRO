//! Sistema de camadas — CR 613.
//!
//! Toda pergunta sobre "o que este objeto é agora" passa por aqui. Ler
//! `CardDef` direto ignora anthem, marcador, mudança de tipo e troca de
//! controle — e o erro é silencioso: o motor continua rodando com números
//! errados até a partida inteira estar corrompida.
//!
//! Ordem de aplicação (CR 613.1), e é ela que separa um motor certo de um
//! motor que só parece certo:
//!   1 cópia · 2 controle · 3 texto · 4 tipo · 5 cor · 6 habilidade ·
//!   7a P/T por característica · 7b define P/T · 7c modifica P/T ·
//!   7d marcadores +1/+1 e −1/−1 · 7e troca P/T.
//! Dentro de uma camada, por timestamp (CR 613.7), com desempate estável por
//! (fonte, índice da habilidade) para que a mesma semente dê o mesmo jogo.
//!
//! As camadas 1, 3, 7a e 7e são no-op deliberado: o IR não tem como expressar
//! cópia, mudança de texto, P/T definido por característica (*/*) nem troca de
//! P/T. Ficam nomeadas na ordem para que implementá-las depois seja acrescentar
//! uma variante, não reescrever o resto.
//!
//! ## Duas fontes de modificação
//!
//! - `state.continuous` — efeito criado por resolução. Os números já foram
//!   travados no momento da resolução (CR 611.2c) e a lista de afetados é
//!   explícita, então avaliar não exige consulta nenhuma.
//! - `Ability::Static` de cada permanente no campo — recalculado sempre, porque
//!   depende do estado atual (CR 604.3). Exige avaliar `Condition`, `Selector`
//!   e `Value` via `query`.
//!
//! ## Quebra de recursão
//!
//! `characteristics` avalia habilidade estática → chama `query` → que volta a
//! chamar `characteristics`. O ciclo é cortado por um contador de profundidade
//! por thread: na chamada reentrante o cálculo pula as habilidades estáticas e
//! usa só `state.continuous` (sem query) mais os marcadores. Consequência
//! assumida: uma habilidade estática enxerga o jogo sem as *outras* habilidades
//! estáticas. Isso erra combos raros do tipo Humility + Opalescence e acerta
//! tudo que os decks do simulador usam, com custo O(n) por consulta em vez de
//! O(n³) — e, o que importa mais, termina sempre.
use std::cell::Cell;

use super::query::{self, EvalCtx};
use super::{Characteristics, Game};
use crate::card::StaticMod;
use crate::ids::{ObjectId, PlayerId};
use crate::ir::{Duration, Keyword, StaticModRuntime};
use crate::mana::{Color, ColorSet};
use crate::state::ObjectState;
use crate::types::{CardType, CounterKind, TypeLine};
use crate::zone::{ZoneId, ZoneKind};

// ---------------------------------------------------------------------------
// Códigos de camada
// ---------------------------------------------------------------------------
//
// Um u8 por camada, com as sub-camadas de 7 numeradas 70..75 para que a
// ordenação lexicográfica simples já produza a ordem das regras.

const LAYER_CONTROL: u8 = 2;
const LAYER_TYPE: u8 = 4;
const LAYER_COLOR: u8 = 5;
const LAYER_ABILITY: u8 = 6;
/// 7b — define P/T.
const LAYER_SET_PT: u8 = 72;
/// 7c — modifica P/T.
const LAYER_MODIFY_PT: u8 = 73;

/// Profundidade máxima de reentrada. Com 1, a avaliação de uma habilidade
/// estática não dispara outra rodada de habilidades estáticas.
const MAX_DEPTH: u32 = 1;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Guarda RAII do contador de reentrada. `Drop` garante que o contador volte
/// mesmo se algum caminho retornar cedo.
struct DepthGuard;

impl DepthGuard {
    /// `Some` quando ainda cabe uma camada de avaliação de habilidade estática.
    fn enter() -> Option<DepthGuard> {
        DEPTH.with(|d| {
            if d.get() >= MAX_DEPTH {
                None
            } else {
                d.set(d.get() + 1);
                Some(DepthGuard)
            }
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

// ---------------------------------------------------------------------------
// Modificação normalizada
// ---------------------------------------------------------------------------

/// Modificação já resolvida em números, pronta para entrar na fila de camadas.
/// Normalizar `StaticMod` e `StaticModRuntime` no mesmo tipo é o que permite
/// ordenar as duas fontes juntas — que é exatamente o que CR 613.7 exige.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ModKind {
    GainControl(PlayerId),
    AddTypes(TypeLine),
    SetColors(Vec<Color>),
    GrantKeywords(Vec<Keyword>),
    LoseKeywords(Vec<Keyword>),
    CantAttack,
    CantBlock,
    CantBeBlocked,
    CantAttackOrBlock,
    PreventAllDamage,
    DoesNotUntap,
    SetPT(i32, i32),
    ModifyPT(i32, i32),
}

impl ModKind {
    fn layer(&self) -> u8 {
        match self {
            ModKind::GainControl(_) => LAYER_CONTROL,
            ModKind::AddTypes(_) => LAYER_TYPE,
            ModKind::SetColors(_) => LAYER_COLOR,
            ModKind::GrantKeywords(_)
            | ModKind::LoseKeywords(_)
            | ModKind::CantAttack
            | ModKind::CantBlock
            | ModKind::CantBeBlocked
            | ModKind::CantAttackOrBlock
            | ModKind::PreventAllDamage
            | ModKind::DoesNotUntap => LAYER_ABILITY,
            ModKind::SetPT(_, _) => LAYER_SET_PT,
            ModKind::ModifyPT(_, _) => LAYER_MODIFY_PT,
        }
    }
}

#[derive(Debug, Clone)]
struct LayerMod {
    layer: u8,
    timestamp: u64,
    /// Desempate determinístico dentro do mesmo timestamp: (fonte, habilidade).
    tie: (u32, u32),
    kind: ModKind,
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

/// Características atuais do objeto, com todas as camadas aplicadas.
/// `None` só quando o objeto não existe ou não tem definição carregada.
pub fn characteristics(game: &Game, id: ObjectId) -> Option<Characteristics> {
    match DepthGuard::enter() {
        Some(_guard) => compute(game, id, true),
        // Reentrante: para antes das habilidades estáticas (ver cabeçalho).
        None => compute(game, id, false),
    }
}

/// O que está impresso na carta (ou na ficha), sem camada nenhuma aplicada.
/// É o que a UI mostra como "base P/T" ao lado do valor atual.
pub fn base_characteristics(game: &Game, id: ObjectId) -> Option<Characteristics> {
    let state = game.state.object(id)?;
    printed(game, state)
}

/// Fim do turno (CR 514.2) e perda de fonte: remove o que já não vale.
/// Chamado da limpeza, uma vez por turno.
pub fn expire_continuous_effects(game: &mut Game) {
    let turn = game.state.turn;
    let active = game.state.active_player;
    // Quem começa o próximo turno. `Duration::YourNextTurn` termina no começo
    // do turno do controlador; como nada acontece entre a limpeza e o desvirar,
    // expirar aqui é equivalente e não exige um segundo ponto de chamada.
    let next_active = game.state.next_player(active);
    let battlefield = zone_objects(game, ZoneId::BATTLEFIELD);

    let before = game.state.continuous.len();
    game.state.continuous.retain(|eff| match eff.duration {
        // Não deveria existir como contínuo; se apareceu, é resíduo.
        Duration::Instant => false,
        // CR 514.2 — "até o fim do turno" acaba na limpeza.
        Duration::EndOfTurn => false,
        // CR 611.2b — some junto com a fonte que o sustenta.
        Duration::WhileSourcePresent => battlefield.contains(&eff.source),
        Duration::YourNextTurn => !(next_active == eff.controller && turn >= eff.created_turn),
        Duration::Permanent => true,
    });

    let removed = before.saturating_sub(game.state.continuous.len());
    if removed > 0 {
        game.state
            .push_log(format!("{removed} efeito(s) contínuo(s) expirou(ram)"), None);
    }
}

// ---------------------------------------------------------------------------
// Cálculo
// ---------------------------------------------------------------------------

fn compute(game: &Game, id: ObjectId, with_statics: bool) -> Option<Characteristics> {
    let state = game.state.object(id)?;
    let mut ch = printed(game, state)?;

    // Palavra-chave impressa entra na camada 6 junto das concedidas, com o
    // timestamp do próprio objeto — que é sempre o menor entre os efeitos que
    // o atingem. Assim "perde todas as habilidades" com timestamp maior apaga
    // o que veio impresso, e uma concessão posterior devolve (CR 613.7).
    let printed_keywords = std::mem::take(&mut ch.keywords);

    let mut mods: Vec<LayerMod> = Vec::new();
    if !printed_keywords.is_empty() {
        mods.push(LayerMod {
            layer: LAYER_ABILITY,
            timestamp: state.timestamp,
            tie: (id.0, 0),
            kind: ModKind::GrantKeywords(printed_keywords),
        });
    }

    collect_runtime_mods(game, id, &mut mods);
    if with_statics {
        collect_static_mods(game, id, &mut mods);
    }

    mods.sort_by(|a, b| {
        (a.layer, a.timestamp, a.tie).cmp(&(b.layer, b.timestamp, b.tie))
    });
    for m in &mods {
        apply(&mut ch, &m.kind);
    }

    apply_counters(state, &mut ch);
    Some(ch)
}

/// Efeitos contínuos vindos de resolução. Não exigem consulta: a lista de
/// afetados e os números já vieram travados (CR 611.2c).
fn collect_runtime_mods(game: &Game, id: ObjectId, out: &mut Vec<LayerMod>) {
    for eff in &game.state.continuous {
        if !eff.affected.contains(&id) {
            continue;
        }
        let kind = match &eff.modification {
            StaticModRuntime::GainControl(p) => ModKind::GainControl(*p),
            StaticModRuntime::AddTypes(tl) => ModKind::AddTypes(tl.clone()),
            StaticModRuntime::SetColors(cs) => ModKind::SetColors(cs.clone()),
            StaticModRuntime::GrantKeywords(ks) => ModKind::GrantKeywords(ks.clone()),
            StaticModRuntime::LoseKeywords(ks) => ModKind::LoseKeywords(ks.clone()),
            StaticModRuntime::CantAttack => ModKind::CantAttack,
            StaticModRuntime::CantBlock => ModKind::CantBlock,
            StaticModRuntime::CantBeBlocked => ModKind::CantBeBlocked,
            StaticModRuntime::CantAttackOrBlock => ModKind::CantAttackOrBlock,
            StaticModRuntime::PreventAllDamage => ModKind::PreventAllDamage,
            StaticModRuntime::DoesNotUntap => ModKind::DoesNotUntap,
            StaticModRuntime::SetPT(p, t) => ModKind::SetPT(*p, *t),
            StaticModRuntime::ModifyPT(p, t) => ModKind::ModifyPT(*p, *t),
            // `Characteristics` não tem campo para a exceção do bloqueio; quem
            // precisa dela é o combate, que lê `state.continuous` direto.
            StaticModRuntime::CantBeBlockedExceptBy(_) => continue,
        };
        out.push(LayerMod {
            layer: kind.layer(),
            timestamp: eff.timestamp,
            tie: (eff.source.0, 0),
            kind,
        });
    }
}

/// Habilidades estáticas de todo permanente no campo (CR 604.3). Recalculadas
/// a cada consulta porque dependem do estado atual — um anthem que conta
/// criaturas muda de valor quando uma criatura morre.
fn collect_static_mods(game: &Game, id: ObjectId, out: &mut Vec<LayerMod>) {
    for src in zone_objects(game, ZoneId::BATTLEFIELD) {
        let Some(sstate) = game.state.object(src) else { continue };
        let Some(card) = game.db.get(sstate.card) else { continue };
        let ctx = EvalCtx::for_source(src, sstate.controller);

        for (index, ability) in card.statics() {
            if !query::eval_condition(game, &ability.condition, &ctx) {
                continue;
            }
            if !query::matches_selector(game, id, &ability.affects, &ctx) {
                continue;
            }
            let kind = match &ability.modification {
                StaticMod::ModifyPT(p, t) => ModKind::ModifyPT(
                    query::eval_value(game, p, &ctx),
                    query::eval_value(game, t, &ctx),
                ),
                StaticMod::SetPT(p, t) => ModKind::SetPT(
                    query::eval_value(game, p, &ctx),
                    query::eval_value(game, t, &ctx),
                ),
                StaticMod::GrantKeywords(ks) => ModKind::GrantKeywords(ks.clone()),
                StaticMod::LoseKeywords(ks) => ModKind::LoseKeywords(ks.clone()),
                StaticMod::SetColors(cs) => ModKind::SetColors(cs.clone()),
                StaticMod::AddTypes(tl) => ModKind::AddTypes(tl.clone()),
                StaticMod::CantAttack => ModKind::CantAttack,
                StaticMod::CantBlock => ModKind::CantBlock,
                StaticMod::PreventAllDamage => ModKind::PreventAllDamage,
                // Ver comentário gêmeo em `collect_runtime_mods`.
                StaticMod::CantBeBlockedExceptBy(_) => continue,
                // CR 601.2f — modificador de custo não é camada; é resolvido em
                // `cast::spell_total_cost` na hora de montar o custo total.
                StaticMod::CostReduction(_, _) | StaticMod::CostIncrease(_, _) => continue,
            };
            out.push(LayerMod {
                layer: kind.layer(),
                timestamp: sstate.timestamp,
                tie: (src.0, index as u32),
                kind,
            });
        }
    }
}

fn apply(ch: &mut Characteristics, kind: &ModKind) {
    match kind {
        // Camada 2 — controle.
        ModKind::GainControl(p) => ch.controller = *p,
        // Camada 4 — tipo. Acrescenta sem remover (CR 613.1d).
        ModKind::AddTypes(tl) => {
            for s in &tl.supertypes {
                if !ch.type_line.supertypes.contains(s) {
                    ch.type_line.supertypes.push(*s);
                }
            }
            for t in &tl.types {
                if !ch.type_line.types.contains(t) {
                    ch.type_line.types.push(*t);
                }
            }
            for s in &tl.subtypes {
                if !ch.type_line.has_subtype(s) {
                    ch.type_line.subtypes.push(s.clone());
                }
            }
        }
        // Camada 5 — cor. Define, não soma (CR 613.1e).
        ModKind::SetColors(cs) => {
            let mut set = ColorSet::COLORLESS;
            for c in cs {
                set.insert(*c);
            }
            ch.colors = set;
        }
        // Camada 6 — habilidade.
        ModKind::GrantKeywords(ks) => {
            for k in ks {
                if !ch.keywords.contains(k) {
                    ch.keywords.push(k.clone());
                }
            }
        }
        ModKind::LoseKeywords(ks) => ch.keywords.retain(|k| !ks.contains(k)),
        ModKind::CantAttack => ch.cant_attack = true,
        ModKind::CantBlock => ch.cant_block = true,
        ModKind::CantBeBlocked => ch.cant_be_blocked = true,
        ModKind::CantAttackOrBlock => {
            ch.cant_attack = true;
            ch.cant_block = true;
        }
        ModKind::PreventAllDamage => ch.prevent_all_damage = true,
        ModKind::DoesNotUntap => ch.does_not_untap = true,
        // Camada 7b — define.
        ModKind::SetPT(p, t) => {
            ch.power = *p;
            ch.toughness = *t;
        }
        // Camada 7c — modifica.
        ModKind::ModifyPT(p, t) => {
            ch.power = ch.power.saturating_add(*p);
            ch.toughness = ch.toughness.saturating_add(*t);
        }
    }
}

/// Camada 7d — marcadores. Vêm depois de qualquer efeito de P/T (CR 613.4d),
/// então um anthem e um marcador +1/+1 somam em vez de competir.
fn apply_counters(state: &ObjectState, ch: &mut Characteristics) {
    let plus = state.counter(&CounterKind::PlusOnePlusOne);
    let minus = state.counter(&CounterKind::MinusOneMinusOne);
    ch.power = ch.power.saturating_add(plus).saturating_sub(minus);
    ch.toughness = ch.toughness.saturating_add(plus).saturating_sub(minus);

    // CR 306.5b — a lealdade de um planeswalker é a contagem de marcadores.
    // Enquanto nenhum marcador foi mexido, o valor impresso é o que vale: é o
    // que a entrada no campo teria colocado lá.
    if state.on_battlefield() && state.counters.contains_key(&CounterKind::Loyalty) {
        ch.loyalty = state.counter(&CounterKind::Loyalty);
    }
}

// ---------------------------------------------------------------------------
// Características impressas
// ---------------------------------------------------------------------------

fn blank(controller: PlayerId) -> Characteristics {
    Characteristics {
        name: String::new(),
        colors: ColorSet::COLORLESS,
        type_line: TypeLine::default(),
        mana_value: 0,
        power: 0,
        toughness: 0,
        loyalty: 0,
        keywords: Vec::new(),
        controller,
        cant_attack: false,
        cant_block: false,
        cant_be_blocked: false,
        prevent_all_damage: false,
        does_not_untap: false,
    }
}

fn printed(game: &Game, state: &ObjectState) -> Option<Characteristics> {
    // CR 708.2 — permanente virado para baixo é uma criatura 2/2 sem nome, sem
    // tipo de criatura, sem custo de mana e sem habilidade nenhuma.
    if state.face_down && state.on_battlefield() {
        let mut ch = blank(state.controller);
        ch.type_line.types.push(CardType::Creature);
        ch.power = 2;
        ch.toughness = 2;
        return Some(ch);
    }

    // CR 111.4 — ficha tem as características que o efeito que a criou definiu.
    if let Some(spec) = &state.token_spec {
        let mut ch = blank(state.controller);
        ch.name = spec.name.clone();
        for c in &spec.colors {
            ch.colors.insert(*c);
        }
        ch.type_line = spec.type_line.clone();
        ch.power = spec.power;
        ch.toughness = spec.toughness;
        ch.keywords = spec.keywords.clone();
        return Some(ch);
    }

    let card = game.db.get(state.card)?;
    let mut ch = blank(state.controller);
    ch.name = card.name.clone();
    ch.colors = card.colors();
    ch.type_line = card.type_line.clone();
    ch.mana_value = card.mana_value();
    ch.power = card.power.unwrap_or(0);
    ch.toughness = card.toughness.unwrap_or(0);
    ch.loyalty = card.loyalty.unwrap_or(0);
    ch.keywords = card.keywords().cloned().collect();
    Some(ch)
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

/// Leitura de zona sem pânico — `GameState::zone` indexa direto e explodiria
/// numa zona não registrada.
fn zone_objects(game: &Game, z: ZoneId) -> Vec<ObjectId> {
    let key = (z.kind, z.owner.map_or(u8::MAX, |p| p.0));
    game.state
        .zones
        .get(&key)
        .map(|zone| zone.objects.clone())
        .unwrap_or_default()
}

/// Exposto para quem precisa saber se o objeto ainda responde às camadas do
/// campo de batalha (combate, SBA) sem duplicar a checagem de zona.
pub fn is_on_battlefield(game: &Game, id: ObjectId) -> bool {
    game.state
        .object(id)
        .map(|o| o.zone.kind == ZoneKind::Battlefield)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Ability, CardDatabase, CardDef, StaticAbility};
    use crate::engine::{GameConfig, PlayerConfig};
    use crate::ids::CardDefId;
    use crate::ir::{Selector, Value};
    use crate::mana::ManaCost;
    use crate::state::ContinuousEffect;
    use crate::types::Rarity;
    use crate::zone::ZoneId;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::sync::Arc;

    fn card(id: u32, name: &str, types: Vec<CardType>, pt: Option<(i32, i32)>) -> CardDef {
        CardDef {
            id: CardDefId(id),
            name: name.to_string(),
            mana_cost: ManaCost::default(),
            type_line: TypeLine { supertypes: Vec::new(), types, subtypes: Vec::new() },
            color_override: None,
            power: pt.map(|(p, _)| p),
            toughness: pt.map(|(_, t)| t),
            loyalty: None,
            abilities: Vec::new(),
            spell_effect: None,
            spell_targets: Vec::new(),
            oracle_text: String::new(),
            flavor_text: None,
            rarity: Rarity::Common,
            set_code: "TST".to_string(),
            collector_number: String::new(),
            artist: None,
            art_key: None,
        }
    }

    /// Monta um jogo sem passar por `Game::new`: mulligan pediria agente, e o
    /// que se testa aqui é cálculo puro de característica.
    fn game_with(cards: Vec<CardDef>, deck_size: usize) -> Game {
        let db = Arc::new(CardDatabase { cards });
        let deck: Vec<CardDefId> = (0..deck_size)
            .map(|i| CardDefId((i % db.cards.len()) as u32))
            .collect();
        let players = vec![
            PlayerConfig { name: "A".into(), deck: deck.clone() },
            PlayerConfig { name: "B".into(), deck },
        ];
        let config = GameConfig::default();
        let state = match super::super::turn::initial_state(&db, &players, &config) {
            Ok(s) => s,
            Err(e) => panic!("estado inicial de teste inválido: {e}"),
        };
        Game {
            state,
            db,
            rng: ChaCha8Rng::seed_from_u64(7),
            config,
            agents: Vec::new(),
            match_events: Vec::new(),
            decisions_made: 0,
            seed: 7,
        }
    }

    /// Primeiro objeto da biblioteca do jogador cuja definição é `def`.
    fn take_from_library(game: &mut Game, player: PlayerId, def: CardDefId) -> ObjectId {
        let ids = zone_objects(game, ZoneId::library(player));
        let found = ids.into_iter().find(|id| {
            game.state.object(*id).map(|o| o.card == def).unwrap_or(false)
        });
        match found {
            Some(id) => id,
            None => panic!("carta {def} não está na biblioteca de {player}"),
        }
    }

    fn put_on_battlefield(game: &mut Game, id: ObjectId, ts: u64) {
        let from = match game.state.object(id) {
            Some(o) => o.zone,
            None => panic!("objeto {id} não existe"),
        };
        if let Some(zone) = game.state.zones.get_mut(&(from.kind, from.owner.map_or(u8::MAX, |p| p.0))) {
            zone.remove(id);
        }
        if let Some(obj) = game.state.object_mut(id) {
            obj.zone = ZoneId::BATTLEFIELD;
            obj.timestamp = ts;
        }
        if let Some(zone) = game.state.zones.get_mut(&(ZoneKind::Battlefield, u8::MAX)) {
            zone.push_bottom(id);
        }
    }

    fn anthem_card(id: u32, name: &str, power: i32, toughness: i32) -> CardDef {
        let mut c = card(id, name, vec![CardType::Enchantment], None);
        c.abilities.push(Ability::Static(StaticAbility {
            condition: crate::ir::Condition::Always,
            affects: Selector::creatures().yours(),
            modification: StaticMod::ModifyPT(Value::c(power), Value::c(toughness)),
            text: "Creatures you control get +1/+1.".to_string(),
        }));
        c
    }

    #[test]
    fn anthem_soma_com_marcador() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let anthem = anthem_card(1, "Anthem", 1, 1);
        let mut game = game_with(vec![bear, anthem], 8);

        let bear_id = take_from_library(&mut game, PlayerId::P0, CardDefId(0));
        let anthem_id = take_from_library(&mut game, PlayerId::P0, CardDefId(1));
        put_on_battlefield(&mut game, bear_id, 10);
        put_on_battlefield(&mut game, anthem_id, 11);
        if let Some(obj) = game.state.object_mut(bear_id) {
            obj.add_counter(CounterKind::PlusOnePlusOne, 1);
        }

        let ch = characteristics(&game, bear_id).expect("urso existe");
        // 2/2 impresso + 1/1 do anthem (7c) + 1/1 do marcador (7d).
        assert_eq!((ch.power, ch.toughness), (4, 4));

        // A base não enxerga camada nenhuma.
        let base = base_characteristics(&game, bear_id).expect("urso existe");
        assert_eq!((base.power, base.toughness), (2, 2));
    }

    #[test]
    fn anthem_nao_pega_criatura_do_oponente() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let anthem = anthem_card(1, "Anthem", 1, 1);
        let mut game = game_with(vec![bear, anthem], 8);

        let mine = take_from_library(&mut game, PlayerId::P0, CardDefId(1));
        let theirs = take_from_library(&mut game, PlayerId::P1, CardDefId(0));
        put_on_battlefield(&mut game, mine, 10);
        put_on_battlefield(&mut game, theirs, 11);

        let ch = characteristics(&game, theirs).expect("criatura existe");
        assert_eq!((ch.power, ch.toughness), (2, 2));
    }

    #[test]
    fn set_pt_aplica_antes_de_modify_pt_mesmo_com_timestamp_maior() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let bear_id = take_from_library(&mut game, PlayerId::P0, CardDefId(0));
        put_on_battlefield(&mut game, bear_id, 1);

        // ModifyPT tem timestamp MAIOR, mas está na camada 7c: aplica depois do
        // SetPT de timestamp menor, que é 7b (CR 613.4b vs 613.4c).
        game.state.continuous.push(ContinuousEffect {
            id: 1,
            source: bear_id,
            affected: vec![bear_id],
            modification: StaticModRuntime::ModifyPT(3, 3),
            duration: Duration::EndOfTurn,
            timestamp: 50,
            created_turn: 1,
            controller: PlayerId::P0,
        });
        game.state.continuous.push(ContinuousEffect {
            id: 2,
            source: bear_id,
            affected: vec![bear_id],
            modification: StaticModRuntime::SetPT(1, 1),
            duration: Duration::EndOfTurn,
            timestamp: 20,
            created_turn: 1,
            controller: PlayerId::P0,
        });

        let ch = characteristics(&game, bear_id).expect("urso existe");
        assert_eq!((ch.power, ch.toughness), (4, 4));
    }

    #[test]
    fn concessao_e_perda_de_palavra_chave_seguem_timestamp() {
        let mut flyer = card(0, "Flyer", vec![CardType::Creature], Some((1, 1)));
        flyer.abilities.push(Ability::Keyword(Keyword::Flying));
        let mut game = game_with(vec![flyer], 8);
        let id = take_from_library(&mut game, PlayerId::P0, CardDefId(0));
        put_on_battlefield(&mut game, id, 5);

        // Impressa: voa.
        let ch = characteristics(&game, id).expect("existe");
        assert!(ch.has_keyword(&Keyword::Flying));

        // Perde depois: não voa mais.
        game.state.continuous.push(ContinuousEffect {
            id: 1,
            source: id,
            affected: vec![id],
            modification: StaticModRuntime::LoseKeywords(vec![Keyword::Flying]),
            duration: Duration::EndOfTurn,
            timestamp: 30,
            created_turn: 1,
            controller: PlayerId::P0,
        });
        let ch = characteristics(&game, id).expect("existe");
        assert!(!ch.has_keyword(&Keyword::Flying));

        // Concessão com timestamp posterior devolve.
        game.state.continuous.push(ContinuousEffect {
            id: 2,
            source: id,
            affected: vec![id],
            modification: StaticModRuntime::GrantKeywords(vec![Keyword::Flying]),
            duration: Duration::EndOfTurn,
            timestamp: 40,
            created_turn: 1,
            controller: PlayerId::P0,
        });
        let ch = characteristics(&game, id).expect("existe");
        assert!(ch.has_keyword(&Keyword::Flying));

        // E uma perda de timestamp anterior à concessão não desfaz nada.
        game.state.continuous.push(ContinuousEffect {
            id: 3,
            source: id,
            affected: vec![id],
            modification: StaticModRuntime::LoseKeywords(vec![Keyword::Flying]),
            duration: Duration::EndOfTurn,
            timestamp: 35,
            created_turn: 1,
            controller: PlayerId::P0,
        });
        let ch = characteristics(&game, id).expect("existe");
        assert!(ch.has_keyword(&Keyword::Flying));
    }

    #[test]
    fn controle_muda_na_camada_2() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let id = take_from_library(&mut game, PlayerId::P0, CardDefId(0));
        put_on_battlefield(&mut game, id, 1);

        game.state.continuous.push(ContinuousEffect {
            id: 1,
            source: id,
            affected: vec![id],
            modification: StaticModRuntime::GainControl(PlayerId::P1),
            duration: Duration::Permanent,
            timestamp: 9,
            created_turn: 1,
            controller: PlayerId::P1,
        });
        let ch = characteristics(&game, id).expect("existe");
        assert_eq!(ch.controller, PlayerId::P1);
    }

    #[test]
    fn expira_fim_de_turno_e_fonte_ausente() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let on = take_from_library(&mut game, PlayerId::P0, CardDefId(0));
        let off = take_from_library(&mut game, PlayerId::P1, CardDefId(0));
        put_on_battlefield(&mut game, on, 1);

        let mk = |id: u32, source: ObjectId, duration: Duration| ContinuousEffect {
            id,
            source,
            affected: vec![source],
            modification: StaticModRuntime::ModifyPT(1, 1),
            duration,
            timestamp: 10 + id as u64,
            created_turn: 1,
            controller: PlayerId::P0,
        };
        game.state.continuous.push(mk(1, on, Duration::EndOfTurn));
        game.state.continuous.push(mk(2, on, Duration::WhileSourcePresent));
        game.state.continuous.push(mk(3, off, Duration::WhileSourcePresent));
        game.state.continuous.push(mk(4, on, Duration::Permanent));

        expire_continuous_effects(&mut game);
        let alive: Vec<u32> = game.state.continuous.iter().map(|e| e.id).collect();
        assert_eq!(alive, vec![2, 4]);
    }

    #[test]
    fn habilidade_estatica_que_se_consulta_termina() {
        // "Criaturas com poder 2 ou mais recebem +1/+1": decidir se a criatura
        // é afetada exige saber o poder dela, que depende desta própria
        // habilidade. Sem a guarda de profundidade isso não termina.
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut lord = card(1, "Recursive Lord", vec![CardType::Enchantment], None);
        lord.abilities.push(Ability::Static(StaticAbility {
            condition: crate::ir::Condition::Always,
            affects: Selector::battlefield(crate::ir::Filter::And(vec![
                crate::ir::Filter::creature(),
                crate::ir::Filter::PowerAtLeast(2),
            ])),
            modification: StaticMod::ModifyPT(Value::c(1), Value::c(1)),
            text: "Creatures with power 2 or greater get +1/+1.".to_string(),
        }));
        let mut game = game_with(vec![bear, lord], 8);
        let bear_id = take_from_library(&mut game, PlayerId::P0, CardDefId(0));
        let lord_id = take_from_library(&mut game, PlayerId::P0, CardDefId(1));
        put_on_battlefield(&mut game, bear_id, 1);
        put_on_battlefield(&mut game, lord_id, 2);

        // Uma volta só: o teste do filtro vê o 2/2 sem a estática, casa, e o
        // bônus entra uma vez.
        let ch = characteristics(&game, bear_id).expect("urso existe");
        assert_eq!((ch.power, ch.toughness), (3, 3));
    }

    #[test]
    fn ficha_usa_o_spec_e_nao_o_carddef() {
        let bear = card(0, "Bear", vec![CardType::Creature], Some((2, 2)));
        let mut game = game_with(vec![bear], 8);
        let id = take_from_library(&mut game, PlayerId::P0, CardDefId(0));
        put_on_battlefield(&mut game, id, 1);
        if let Some(obj) = game.state.object_mut(id) {
            obj.is_token = true;
            obj.token_spec = Some(Box::new(crate::ir::TokenSpec {
                name: "Soldier".to_string(),
                type_line: TypeLine {
                    supertypes: Vec::new(),
                    types: vec![CardType::Creature],
                    subtypes: vec!["Soldier".to_string()],
                },
                colors: vec![Color::White],
                power: 1,
                toughness: 1,
                keywords: vec![Keyword::Vigilance],
                art_key: None,
            }));
        }
        let ch = characteristics(&game, id).expect("ficha existe");
        assert_eq!(ch.name, "Soldier");
        assert_eq!((ch.power, ch.toughness), (1, 1));
        assert!(ch.has_keyword(&Keyword::Vigilance));
        assert!(ch.colors.contains(Color::White));
    }
}
