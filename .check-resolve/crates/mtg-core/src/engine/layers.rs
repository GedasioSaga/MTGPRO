//! Sistema de camadas (CR 613).
//!
//! Característica atual de um objeto não é o que está impresso: é o impresso
//! passado por uma pilha ordenada de modificações. A ordem é o que separa um
//! motor que acerta de um que erra — aplicar "define P/T 1/1" depois de "+2/+2"
//! dá 1/1, aplicar na ordem certa dá 3/3.
//!
//! Ordem implementada: 1 cópia · 2 controle · 3 texto · 4 tipo · 5 cor ·
//! 6 habilidade · 7a P/T por característica · 7b define P/T · 7c modifica P/T ·
//! 7d marcadores · 7e troca P/T. Dentro da camada, timestamp crescente
//! (CR 613.7). Camadas 1, 3, 7a e 7e não têm variante correspondente no IR
//! (`StaticModRuntime`) — ficam como ponto de extensão, não como stub.
//!
//! Duas fontes alimentam as camadas:
//!   - `state.continuous` — efeito já resolvido, valores travados na resolução
//!     (CR 611.2c). Não precisa de avaliação, é só aplicar.
//!   - `Ability::Static` de todo permanente no campo — recalculado a cada
//!     consulta, com `Value` avaliado no contexto da fonte.
//!
//! **Recursão.** Avaliar a habilidade estática de um lorde exige perguntar
//! "esse objeto casa com o seletor?", e o seletor pode falar de poder ou
//! palavra-chave — que voltam a chamar `characteristics`. Quebramos o ciclo com
//! um guarda de reentrância por thread: na chamada aninhada devolvemos a versão
//! reduzida, que aplica base + `state.continuous` + marcadores e **ignora
//! habilidade estática**. Como a versão reduzida não consulta habilidade
//! nenhuma, ela termina sempre. O custo é aproximação em caso raro de dependência
//! entre estáticas (lorde que só afeta criaturas que outro lorde tornou 3/3);
//! a alternativa correta seria o grafo de dependência de CR 613.8, que não paga
//! sua complexidade num simulador de bot.
use std::cell::Cell;

use super::query::{self, EvalCtx};
use super::{Characteristics, Game};
use crate::card::{Ability, StaticMod};
use crate::event::Step;
use crate::ids::ObjectId;
use crate::ir::{Duration, Filter, StaticModRuntime};
use crate::mana::ColorSet;
use crate::types::{CardType, CounterKind};
use crate::zone::ZoneKind;

// ---------------------------------------------------------------------------
// Ordem das camadas
// ---------------------------------------------------------------------------

/// Camada 7b — define P/T.
const LAYER_SET_PT: u8 = 71;
/// Camada 7c — modifica P/T.
const LAYER_MODIFY_PT: u8 = 72;

/// Chave de ordenação da modificação. `StaticModRuntime::layer` já separa 7b de
/// 7c usando 7 e 8; aqui isso vira uma escala única em que camadas 2..6 vêm
/// antes de qualquer mexida em P/T.
fn layer_order(m: &StaticModRuntime) -> u8 {
    match m.layer() {
        7 => LAYER_SET_PT,
        8 => LAYER_MODIFY_PT,
        n => n.saturating_mul(10),
    }
}

// ---------------------------------------------------------------------------
// Guarda de reentrância
// ---------------------------------------------------------------------------

thread_local! {
    static DEPTH: Cell<u8> = const { Cell::new(0) };
}

struct DepthGuard;

impl DepthGuard {
    fn enter() -> DepthGuard {
        DEPTH.with(|d| d.set(d.get().saturating_add(1)));
        DepthGuard
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

fn reentrant() -> bool {
    DEPTH.with(|d| d.get()) > 0
}

// ---------------------------------------------------------------------------
// Camada 1 — características impressas
// ---------------------------------------------------------------------------

fn blank(controller: crate::ids::PlayerId) -> Characteristics {
    Characteristics {
        name: String::new(),
        colors: ColorSet::COLORLESS,
        type_line: crate::types::TypeLine::default(),
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

/// O que está impresso (ou, para ficha, o que a `TokenSpec` declara).
/// Sem camadas, sem marcadores — é isto que a UI mostra como "P/T base".
pub fn base_characteristics(game: &Game, id: ObjectId) -> Option<Characteristics> {
    let obj = game.state.object(id)?;
    let mut c = blank(obj.controller);

    // Ficha não tem carta impressa: a especificação é a própria impressão.
    if let Some(spec) = obj.token_spec.as_deref() {
        c.name = spec.name.clone();
        for color in &spec.colors {
            c.colors.insert(*color);
        }
        c.type_line = spec.type_line.clone();
        c.power = spec.power;
        c.toughness = spec.toughness;
        c.keywords = spec.keywords.clone();
        return Some(c);
    }

    let def = game.db.get(obj.card)?;
    c.name = def.name.clone();
    c.colors = def.colors();
    c.type_line = def.type_line.clone();
    c.mana_value = def.mana_value();
    c.power = def.power.unwrap_or(0);
    c.toughness = def.toughness.unwrap_or(0);
    c.loyalty = def.loyalty.unwrap_or(0);
    // Palavra-chave impressa é camada 6 tanto quanto a concedida (CR 613.1f);
    // entra na base porque a base é o ponto de partida daquela camada.
    c.keywords = def.keywords().cloned().collect();
    Some(c)
}

// ---------------------------------------------------------------------------
// Coleta das modificações
// ---------------------------------------------------------------------------

/// Ids do campo de batalha sem passar por `GameState::zone`, que entra em pânico
/// se a zona não existir.
fn battlefield_ids(game: &Game) -> &[ObjectId] {
    game.state
        .zones
        .get(&(ZoneKind::Battlefield, u8::MAX))
        .map(|z| z.objects.as_slice())
        .unwrap_or(&[])
}

/// Converte a modificação declarada na carta em modificação com números já
/// resolvidos, avaliando cada `Value` no contexto da fonte.
fn runtime_mod(game: &Game, m: &StaticMod, ctx: &EvalCtx) -> Option<StaticModRuntime> {
    let v = |val| query::eval_value(game, val, ctx);
    Some(match m {
        StaticMod::ModifyPT(p, t) => StaticModRuntime::ModifyPT(v(p), v(t)),
        StaticMod::SetPT(p, t) => StaticModRuntime::SetPT(v(p), v(t)),
        StaticMod::GrantKeywords(ks) => StaticModRuntime::GrantKeywords(ks.clone()),
        StaticMod::LoseKeywords(ks) => StaticModRuntime::LoseKeywords(ks.clone()),
        StaticMod::SetColors(cs) => StaticModRuntime::SetColors(cs.clone()),
        StaticMod::AddTypes(tl) => StaticModRuntime::AddTypes(tl.clone()),
        StaticMod::CantAttack => StaticModRuntime::CantAttack,
        StaticMod::CantBlock => StaticModRuntime::CantBlock,
        StaticMod::CantBeBlockedExceptBy(f) => StaticModRuntime::CantBeBlockedExceptBy(f.clone()),
        StaticMod::PreventAllDamage => StaticModRuntime::PreventAllDamage,
        // Alteração de custo não é camada: é aplicada ao calcular o custo total
        // em `cast.rs` (CR 601.2f). Não tem efeito sobre característica.
        StaticMod::CostReduction(_, _) | StaticMod::CostIncrease(_, _) => return None,
    })
}

/// Todas as modificações que atingem `id`, já com camada e timestamp.
fn collect_mods(game: &Game, id: ObjectId, include_statics: bool) -> Vec<(u8, u64, StaticModRuntime)> {
    let mut mods: Vec<(u8, u64, StaticModRuntime)> = Vec::new();

    // Fonte 1: efeito já resolvido. Valores travados na resolução (CR 611.2c).
    for e in &game.state.continuous {
        if e.affected.contains(&id) {
            mods.push((
                layer_order(&e.modification),
                e.timestamp,
                e.modification.clone(),
            ));
        }
    }

    // Fonte 2: habilidade estática de permanente no campo, recalculada agora.
    if include_statics {
        for src_id in battlefield_ids(game) {
            let Some(src) = game.state.object(*src_id) else {
                continue;
            };
            // Ficha não herda as habilidades da carta cujo id ela reaproveita.
            if src.is_token && src.token_spec.is_some() {
                continue;
            }
            let Some(def) = game.db.get(src.card) else {
                continue;
            };
            let ctx = EvalCtx::for_source(*src_id, src.controller);
            for ability in &def.abilities {
                let Ability::Static(sa) = ability else {
                    continue;
                };
                if !query::eval_condition(game, &sa.condition, &ctx) {
                    continue;
                }
                if !query::matches_selector(game, id, &sa.affects, &ctx) {
                    continue;
                }
                let Some(m) = runtime_mod(game, &sa.modification, &ctx) else {
                    continue;
                };
                // CR 613.7d — o efeito de habilidade estática usa o timestamp
                // do objeto que a tem.
                mods.push((layer_order(&m), src.timestamp, m));
            }
        }
    }

    // Ordenação estável: mesma camada, timestamp crescente (CR 613.7).
    mods.sort_by_key(|(layer, ts, _)| (*layer, *ts));
    mods
}

// ---------------------------------------------------------------------------
// Aplicação
// ---------------------------------------------------------------------------

fn apply(chars: &mut Characteristics, m: &StaticModRuntime) {
    match m {
        StaticModRuntime::GainControl(p) => chars.controller = *p,
        StaticModRuntime::AddTypes(tl) => {
            for s in &tl.supertypes {
                if !chars.type_line.supertypes.contains(s) {
                    chars.type_line.supertypes.push(*s);
                }
            }
            for t in &tl.types {
                if !chars.type_line.types.contains(t) {
                    chars.type_line.types.push(*t);
                }
            }
            for s in &tl.subtypes {
                if !chars.type_line.has_subtype(s) {
                    chars.type_line.subtypes.push(s.clone());
                }
            }
        }
        StaticModRuntime::SetColors(cs) => {
            let mut set = ColorSet::COLORLESS;
            for c in cs {
                set.insert(*c);
            }
            chars.colors = set;
        }
        StaticModRuntime::GrantKeywords(ks) => {
            for k in ks {
                if !chars.keywords.contains(k) {
                    chars.keywords.push(k.clone());
                }
            }
        }
        StaticModRuntime::LoseKeywords(ks) => chars.keywords.retain(|k| !ks.contains(k)),
        StaticModRuntime::CantAttack => chars.cant_attack = true,
        StaticModRuntime::CantBlock => chars.cant_block = true,
        StaticModRuntime::CantBeBlocked => chars.cant_be_blocked = true,
        StaticModRuntime::CantAttackOrBlock => {
            chars.cant_attack = true;
            chars.cant_block = true;
        }
        StaticModRuntime::PreventAllDamage => chars.prevent_all_damage = true,
        StaticModRuntime::DoesNotUntap => chars.does_not_untap = true,
        // Restrição de bloqueio carrega um filtro, que não cabe em `bool`:
        // `combat.rs` consulta `block_restrictions` para lê-la.
        StaticModRuntime::CantBeBlockedExceptBy(_) => {}
        StaticModRuntime::SetPT(p, t) => {
            chars.power = *p;
            chars.toughness = *t;
        }
        StaticModRuntime::ModifyPT(p, t) => {
            chars.power = chars.power.saturating_add(*p);
            chars.toughness = chars.toughness.saturating_add(*t);
        }
    }
}

fn compute(game: &Game, id: ObjectId, include_statics: bool) -> Option<Characteristics> {
    let mut chars = base_characteristics(game, id)?;

    for (_, _, m) in collect_mods(game, id, include_statics) {
        apply(&mut chars, &m);
    }

    let obj = game.state.object(id)?;

    // Camada 7d — marcadores +1/+1 e −1/−1 entram depois de toda modificação de
    // camada 7c (CR 613.4d), por isso ficam fora da lista ordenada acima.
    let delta = obj
        .counter(&CounterKind::PlusOnePlusOne)
        .saturating_sub(obj.counter(&CounterKind::MinusOneMinusOne));
    if delta != 0 {
        chars.power = chars.power.saturating_add(delta);
        chars.toughness = chars.toughness.saturating_add(delta);
    }

    // CR 306.5b — lealdade de um planeswalker no campo é a contagem de
    // marcadores de lealdade, não o número impresso.
    if obj.on_battlefield() && chars.type_line.has_type(CardType::Planeswalker) {
        chars.loyalty = obj.counter(&CounterKind::Loyalty);
    }

    Some(chars)
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

/// Características atuais, com todas as camadas aplicadas.
/// Devolve `None` só quando o objeto não existe ou a carta some do catálogo.
pub fn characteristics(game: &Game, id: ObjectId) -> Option<Characteristics> {
    if reentrant() {
        // Chamada vinda de dentro da avaliação de uma habilidade estática:
        // versão reduzida, sem habilidade estática, para o ciclo terminar.
        return compute(game, id, false);
    }
    let _guard = DepthGuard::enter();
    compute(game, id, true)
}

/// Filtros de "só pode ser bloqueada por..." ativos sobre um atacante.
///
/// Adição ao contrato: `StaticModRuntime::CantBeBlockedExceptBy` carrega um
/// `Filter`, e `Characteristics` só tem campos booleanos. `combat::can_block`
/// consulta esta função em vez de reimplementar a coleta de camadas.
pub fn block_restrictions(game: &Game, id: ObjectId) -> Vec<Filter> {
    let include_statics = !reentrant();
    let _guard = DepthGuard::enter();
    collect_mods(game, id, include_statics)
        .into_iter()
        .filter_map(|(_, _, m)| match m {
            StaticModRuntime::CantBeBlockedExceptBy(f) => Some(f),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Expiração
// ---------------------------------------------------------------------------

/// Remove os efeitos contínuos cuja duração acabou (CR 613.6, 611.2b).
///
/// Seguro de chamar com qualquer frequência: `EndOfTurn` só some na limpeza (ou
/// se sobrou de um turno anterior), então chamar isto depois de cada SBA não
/// apaga efeito que ainda deveria valer.
pub fn expire_continuous_effects(game: &mut Game) {
    let turn = game.state.turn;
    let active = game.state.active_player;
    let cleanup = game.state.step == Step::Cleanup;

    let mut expired: Vec<(u32, ObjectId)> = Vec::new();
    for e in &game.state.continuous {
        let source_present = game
            .state
            .object(e.source)
            .is_some_and(|o| o.on_battlefield());
        let done = match e.duration {
            // Efeito único não tinha o que continuar: se está na lista, é lixo.
            Duration::Instant => true,
            Duration::EndOfTurn => cleanup || e.created_turn < turn,
            Duration::WhileSourcePresent => !source_present,
            // "Até o começo do seu próximo turno": criado no turno T, some no
            // primeiro turno posterior em que o controlador é o jogador ativo.
            Duration::YourNextTurn => turn > e.created_turn && active == e.controller,
            Duration::Permanent => false,
        };
        if done {
            expired.push((e.id, e.source));
        }
    }

    if expired.is_empty() {
        return;
    }
    game.state
        .continuous
        .retain(|e| !expired.iter().any(|(id, _)| *id == e.id));
    for (id, source) in expired {
        let name = game.card_name(source);
        game.state
            .push_log(format!("efeito contínuo #{id} de {name} expirou"), None);
    }
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Request;
    use crate::card::{CardDatabase, CardDef, StaticAbility};
    use crate::engine::GameConfig;
    use crate::ids::{CardDefId, IdGen, PlayerId};
    use crate::ir::{Condition, Keyword, Selector, Value};
    use crate::mana::ManaCost;
    use crate::state::{ContinuousEffect, GameOutcome, GameState, ObjectState, PlayerState};
    use crate::types::{Rarity, TypeLine};
    use crate::zone::{Zone, ZoneId};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn def(name: &str, types: Vec<CardType>, pt: Option<(i32, i32)>, abilities: Vec<Ability>) -> CardDef {
        CardDef {
            id: CardDefId(0),
            name: name.to_string(),
            mana_cost: ManaCost::default(),
            type_line: TypeLine {
                supertypes: Vec::new(),
                types,
                subtypes: Vec::new(),
            },
            color_override: None,
            power: pt.map(|(p, _)| p),
            toughness: pt.map(|(_, t)| t),
            loyalty: None,
            abilities,
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

    /// Anthem: "criaturas que você controla recebem +1/+1".
    fn anthem(modification: StaticMod) -> CardDef {
        def(
            "Anthem",
            vec![CardType::Enchantment],
            None,
            vec![Ability::Static(StaticAbility {
                condition: Condition::Always,
                affects: Selector::creatures().yours(),
                modification,
                text: "Criaturas que você controla recebem o bônus.".to_string(),
            })],
        )
    }

    fn empty_game(cards: Vec<CardDef>) -> Game {
        let mut db = CardDatabase { cards };
        db.reindex();

        let mut zones = BTreeMap::new();
        zones.insert(
            (ZoneKind::Battlefield, u8::MAX),
            Zone::new(ZoneKind::Battlefield),
        );
        zones.insert((ZoneKind::Stack, u8::MAX), Zone::new(ZoneKind::Stack));
        zones.insert((ZoneKind::Exile, u8::MAX), Zone::new(ZoneKind::Exile));
        for p in 0u8..2 {
            zones.insert((ZoneKind::Hand, p), Zone::new(ZoneKind::Hand));
            zones.insert((ZoneKind::Library, p), Zone::new(ZoneKind::Library));
            zones.insert((ZoneKind::Graveyard, p), Zone::new(ZoneKind::Graveyard));
        }

        let state = GameState {
            players: vec![
                PlayerState::new(PlayerId(0), "A", 20),
                PlayerState::new(PlayerId(1), "B", 20),
            ],
            objects: Vec::new(),
            zones,
            stack: Vec::new(),
            continuous: Vec::new(),
            turn: 1,
            active_player: PlayerId(0),
            priority_player: PlayerId(0),
            step: Step::PrecombatMain,
            consecutive_passes: 0,
            extra_turns: Vec::new(),
            extra_combats: 0,
            first_strike_done: false,
            outcome: GameOutcome::Ongoing,
            pending: Request::GameOver,
            id_gen: IdGen::default(),
            timestamp: 0,
            next_effect_id: 0,
            event_queue: Vec::new(),
            pending_triggers: Vec::new(),
            log: Vec::new(),
        };

        Game {
            state,
            db: Arc::new(db),
            rng: ChaCha8Rng::seed_from_u64(1),
            config: GameConfig::default(),
            agents: Vec::new(),
            match_events: Vec::new(),
            decisions_made: 0,
            seed: 1,
        }
    }

    fn put(game: &mut Game, card: usize, controller: PlayerId) -> ObjectId {
        let id = game.state.id_gen.next_object();
        let ts = game.state.next_timestamp();
        let obj = ObjectState::new(id, CardDefId(card as u32), controller, ZoneId::BATTLEFIELD, ts);
        // `objects` é indexado por `ObjectId.0`, e os ids saem em sequência.
        game.state.objects.push(obj);
        if let Some(z) = game.state.zones.get_mut(&(ZoneKind::Battlefield, u8::MAX)) {
            z.objects.push(id);
        }
        id
    }

    #[test]
    fn anthem_empilha_com_marcador() {
        let bear = def("Bear", vec![CardType::Creature], Some((2, 2)), Vec::new());
        let mut game = empty_game(vec![
            anthem(StaticMod::ModifyPT(Value::Const(1), Value::Const(1))),
            bear,
        ]);
        put(&mut game, 0, PlayerId(0));
        let creature = put(&mut game, 1, PlayerId(0));
        if let Some(o) = game.state.object_mut(creature) {
            o.add_counter(CounterKind::PlusOnePlusOne, 1);
        }

        let base = base_characteristics(&game, creature).expect("base existe");
        assert_eq!((base.power, base.toughness), (2, 2));

        // 2/2 impresso + 1/1 do anthem (camada 7c) + 1/1 do marcador (7d).
        let c = characteristics(&game, creature).expect("características existem");
        assert_eq!((c.power, c.toughness), (4, 4));
    }

    #[test]
    fn set_pt_aplica_antes_de_modify_pt() {
        let bear = def("Bear", vec![CardType::Creature], Some((2, 2)), Vec::new());
        let mut game = empty_game(vec![bear]);
        let creature = put(&mut game, 0, PlayerId(0));

        // ModifyPT com timestamp menor, SetPT com timestamp MAIOR: se a ordem
        // fosse só por timestamp o resultado seria 1/1.
        game.state.continuous.push(ContinuousEffect {
            id: 1,
            source: creature,
            affected: vec![creature],
            modification: StaticModRuntime::ModifyPT(2, 2),
            duration: Duration::EndOfTurn,
            timestamp: 10,
            created_turn: 1,
            controller: PlayerId(0),
        });
        game.state.continuous.push(ContinuousEffect {
            id: 2,
            source: creature,
            affected: vec![creature],
            modification: StaticModRuntime::SetPT(1, 1),
            duration: Duration::EndOfTurn,
            timestamp: 20,
            created_turn: 1,
            controller: PlayerId(0),
        });

        // 7b define 1/1, depois 7c soma +2/+2.
        let c = characteristics(&game, creature).expect("características existem");
        assert_eq!((c.power, c.toughness), (3, 3));
    }

    #[test]
    fn concessao_de_palavra_chave() {
        let bear = def("Bear", vec![CardType::Creature], Some((2, 2)), Vec::new());
        let mut game = empty_game(vec![
            anthem(StaticMod::GrantKeywords(vec![Keyword::Flying])),
            bear,
        ]);
        let lord = put(&mut game, 0, PlayerId(0));
        let creature = put(&mut game, 1, PlayerId(0));

        let c = characteristics(&game, creature).expect("características existem");
        assert!(c.has_keyword(&Keyword::Flying), "camada 6 concede voar");

        let base = base_characteristics(&game, creature).expect("base existe");
        assert!(!base.has_keyword(&Keyword::Flying), "impresso não tem voar");

        // O próprio encantamento não é criatura: fora do seletor.
        let lord_chars = characteristics(&game, lord).expect("características existem");
        assert!(!lord_chars.has_keyword(&Keyword::Flying));
    }

    #[test]
    fn efeito_de_fonte_ausente_expira() {
        let bear = def("Bear", vec![CardType::Creature], Some((2, 2)), Vec::new());
        let mut game = empty_game(vec![bear]);
        let creature = put(&mut game, 0, PlayerId(0));
        game.state.continuous.push(ContinuousEffect {
            id: 7,
            source: ObjectId(999),
            affected: vec![creature],
            modification: StaticModRuntime::ModifyPT(5, 5),
            duration: Duration::WhileSourcePresent,
            timestamp: 3,
            created_turn: 1,
            controller: PlayerId(0),
        });

        expire_continuous_effects(&mut game);
        assert!(game.state.continuous.is_empty());
        let c = characteristics(&game, creature).expect("características existem");
        assert_eq!((c.power, c.toughness), (2, 2));
    }
}
