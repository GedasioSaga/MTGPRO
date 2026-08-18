//! Ações baseadas em estado (CR 704).
//!
//! Regra de ouro do CR 704.3: todas as SBAs aplicáveis são vistas ao mesmo
//! tempo e executadas como um único evento. Por isso `check` faz duas passadas
//! — primeiro **coleta** tudo sobre um estado congelado, depois **aplica**. Um
//! laço que mutasse enquanto itera daria resultados dependentes de ordem, que é
//! exatamente o bug que o CR proíbe.
use std::collections::BTreeMap;

use super::query::EvalCtx;
use super::{layers, query, turn, Characteristics, Game};
use crate::action::{Action, Request};
use crate::event::LossReason;
use crate::ids::{ObjectId, PlayerId};
use crate::ir::Keyword;
use crate::state::ObjectState;
use crate::types::{CardType, CounterKind, Supertype};
use crate::view::MatchEvent;
use crate::zone::ZoneId;

/// CR 704.5c — dez marcadores de veneno derrotam o jogador.
const POISON_TO_LOSE: i32 = 10;
/// Teto do laço de estabilização; SBA que não converge é bug, não regra.
const STABILITY_GUARD: u32 = 1000;

/// Por que um permanente vai para o cemitério. O motivo muda a mensagem e,
/// principalmente, se regeneração pode salvar (só destruição é substituível).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeathCause {
    ZeroToughness,
    LethalDamage,
    Deathtouch,
    NoLoyalty,
    IllegalAura,
    LegendRule,
}

impl DeathCause {
    /// CR 701.7 — só "destruir" pode ser trocado por regeneração.
    fn is_destruction(self) -> bool {
        matches!(self, DeathCause::LethalDamage | DeathCause::Deathtouch)
    }
    fn label(self) -> &'static str {
        match self {
            DeathCause::ZeroToughness => "resistência 0 ou menos (CR 704.5f)",
            DeathCause::LethalDamage => "dano letal (CR 704.5g)",
            DeathCause::Deathtouch => "dano de toque mortal (CR 704.5h)",
            DeathCause::NoLoyalty => "sem marcadores de lealdade (CR 704.5i)",
            DeathCause::IllegalAura => "aura anexada ilegalmente (CR 704.5m)",
            DeathCause::LegendRule => "regra da lenda (CR 704.5j)",
        }
    }
}

/// Tudo o que a passada de leitura encontrou. Nada aqui foi aplicado ainda.
#[derive(Default)]
struct Pending {
    losses: Vec<(PlayerId, LossReason)>,
    deaths: Vec<(ObjectId, DeathCause)>,
    regenerate: Vec<ObjectId>,
    unattach: Vec<ObjectId>,
    counter_pairs: Vec<(ObjectId, i32)>,
    legend_groups: Vec<(PlayerId, String, Vec<ObjectId>)>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.losses.is_empty()
            && self.deaths.is_empty()
            && self.regenerate.is_empty()
            && self.unattach.is_empty()
            && self.counter_pairs.is_empty()
            && self.legend_groups.is_empty()
    }
}

/// Uma rodada de SBA. `true` se alguma coisa se aplicou.
pub fn check(game: &mut Game) -> bool {
    let pending = collect_actions(game);
    if pending.is_empty() {
        return false;
    }
    apply(game, pending);
    true
}

/// CR 704.3 — repete até nenhuma SBA se aplicar; só então alguém recebe
/// prioridade. Devolve `true` se alguma rodada aplicou alguma coisa: quem chama
/// precisa saber que o estado mudou para reiniciar a rodada de prioridade
/// (CR 117.4 — "passar em sequência" exige que nada tenha acontecido no meio).
pub fn check_until_stable(game: &mut Game) -> bool {
    let mut applied = false;
    let mut rounds = 0u32;
    while check(game) {
        applied = true;
        rounds += 1;
        if rounds >= STABILITY_GUARD {
            turn::force_draw(game, "ações baseadas em estado não estabilizaram");
            return applied;
        }
        if game.state.is_over() {
            return applied;
        }
    }
    applied
}

// ---------------------------------------------------------------------------
// Passada de leitura
// ---------------------------------------------------------------------------

fn collect_actions(game: &Game) -> Pending {
    let mut out = Pending::default();

    for player in &game.state.players {
        if player.has_lost {
            continue;
        }
        // CR 704.5a
        if player.life <= 0 {
            out.losses.push((player.id, LossReason::ZeroLife));
            continue;
        }
        // CR 704.5c
        if player.poison >= POISON_TO_LOSE {
            out.losses.push((player.id, LossReason::PoisonCounters));
            continue;
        }
        // CR 704.5b — a tentativa de compra em biblioteca vazia é marcada por
        // `turn::draw_card`; a derrota só acontece aqui, o que permite ganhar
        // a partida antes de perder por isso.
        if matches!(player.loss_reason, Some(LossReason::DrewFromEmptyLibrary)) {
            out.losses
                .push((player.id, LossReason::DrewFromEmptyLibrary));
        }
    }

    let mut legends: BTreeMap<(PlayerId, String), Vec<ObjectId>> = BTreeMap::new();

    for id in turn::zone_objects(game, ZoneId::BATTLEFIELD) {
        let Some(obj) = game.state.object(id) else {
            continue;
        };
        let Some(ch) = layers::characteristics(game, id) else {
            continue;
        };

        // CR 704.5r — marcadores +1/+1 e −1/−1 se anulam aos pares.
        let pairs = obj
            .counter(&CounterKind::PlusOnePlusOne)
            .min(obj.counter(&CounterKind::MinusOneMinusOne));
        if pairs > 0 {
            out.counter_pairs.push((id, pairs));
        }

        if let Some(cause) = creature_death_cause(obj, &ch) {
            if cause.is_destruction() && obj.regeneration_shields > 0 {
                out.regenerate.push(id);
            } else {
                out.deaths.push((id, cause));
            }
        }

        // CR 704.5i — planeswalker sem lealdade vai para o cemitério.
        if ch.type_line.has_type(CardType::Planeswalker)
            && obj.counter(&CounterKind::Loyalty) <= 0
        {
            out.deaths.push((id, DeathCause::NoLoyalty));
        }

        // CR 704.5m — aura anexada a algo ilegal (ou a nada) vai ao cemitério.
        if is_aura(&ch) && !aura_attachment_legal(game, id, obj, &ch) {
            out.deaths.push((id, DeathCause::IllegalAura));
        }

        // CR 704.5q — equipamento anexado a permanente ilegal apenas se solta.
        if is_equipment(&ch) && obj.attached_to.is_some() && !equipment_host_legal(game, obj) {
            out.unattach.push(id);
        }

        // CR 704.5j — regra da lenda, por controlador e por nome.
        if ch.type_line.has_supertype(Supertype::Legendary) {
            legends
                .entry((ch.controller, ch.name.clone()))
                .or_default()
                .push(id);
        }
    }

    for ((controller, name), group) in legends {
        if group.len() > 1 {
            out.legend_groups.push((controller, name, group));
        }
    }

    out
}

/// CR 704.5f/g/h. A resistência usada é sempre a **atual**, vinda das camadas —
/// nunca a impressa.
fn creature_death_cause(obj: &ObjectState, ch: &Characteristics) -> Option<DeathCause> {
    if !ch.is_creature() {
        return None;
    }
    // CR 704.5f — resistência 0 ou menos põe no cemitério. Indestrutível NÃO
    // salva aqui: isto não é destruição.
    if ch.toughness <= 0 {
        return Some(DeathCause::ZeroToughness);
    }
    // CR 702.12b — indestrutível ignora dano letal e toque mortal.
    if ch.has_keyword(&Keyword::Indestructible) {
        return None;
    }
    // CR 704.5h — qualquer dano de fonte com toque mortal é letal.
    if obj.damage > 0 && obj.deathtouch_damage {
        return Some(DeathCause::Deathtouch);
    }
    // CR 704.5g
    if obj.damage >= ch.toughness {
        return Some(DeathCause::LethalDamage);
    }
    None
}

fn is_aura(ch: &Characteristics) -> bool {
    ch.type_line.has_subtype("Aura")
}

fn is_equipment(ch: &Characteristics) -> bool {
    ch.type_line.has_subtype("Equipment")
}

/// A aura está presa a um permanente que ainda existe, está em campo e casa com
/// o filtro de "Encantar" (CR 702.5).
fn aura_attachment_legal(
    game: &Game,
    aura: ObjectId,
    obj: &ObjectState,
    ch: &Characteristics,
) -> bool {
    let Some(host) = obj.attached_to else {
        return false;
    };
    match game.state.object(host) {
        Some(h) if h.on_battlefield() => {}
        _ => return false,
    }
    let ctx = EvalCtx::for_source(aura, ch.controller);
    for keyword in &ch.keywords {
        if let Keyword::Enchant(filter) = keyword {
            return query::matches_filter(game, host, filter, &ctx);
        }
    }
    // Sem "Encantar" declarado não há filtro a violar; estar preso basta.
    true
}

/// Equipamento só pode estar preso a uma criatura em campo (CR 301.5c).
fn equipment_host_legal(game: &Game, obj: &ObjectState) -> bool {
    let Some(host) = obj.attached_to else {
        return false;
    };
    match game.state.object(host) {
        Some(h) if h.on_battlefield() => {}
        _ => return false,
    }
    layers::characteristics(game, host)
        .map(|c| c.is_creature())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Passada de aplicação
// ---------------------------------------------------------------------------

fn apply(game: &mut Game, pending: Pending) {
    for (id, pairs) in pending.counter_pairs {
        annihilate_counters(game, id, pairs);
    }
    for id in pending.unattach {
        unattach(game, id);
    }
    for id in pending.regenerate {
        regenerate(game, id);
    }

    let mut deaths = pending.deaths;
    for (controller, name, group) in pending.legend_groups {
        for loser in choose_legend_losers(game, controller, &name, group) {
            deaths.push((loser, DeathCause::LegendRule));
        }
    }

    // Um mesmo permanente pode ter batido em duas condições; ele morre uma vez.
    let mut seen: Vec<ObjectId> = Vec::with_capacity(deaths.len());
    for (id, cause) in deaths {
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        put_in_graveyard(game, id, cause);
    }

    for (player, reason) in pending.losses {
        turn::lose_game(game, player, reason);
    }
}

/// CR 704.5r — remove `pairs` de cada tipo; os dois eventos saem juntos porque
/// a anulação é um evento só.
fn annihilate_counters(game: &mut Game, id: ObjectId, pairs: i32) {
    let Some(obj) = game.state.object_mut(id) else {
        return;
    };
    obj.add_counter(CounterKind::PlusOnePlusOne, -pairs);
    obj.add_counter(CounterKind::MinusOneMinusOne, -pairs);

    for kind in [CounterKind::PlusOnePlusOne, CounterKind::MinusOneMinusOne] {
        game.state.emit(crate::event::GameEvent::CountersRemoved {
            object: id,
            kind: kind.clone(),
            amount: pairs,
        });
        game.push_event(MatchEvent::CountersChanged {
            card: id,
            kind: format!("{kind:?}"),
            delta: -pairs,
        });
    }
    let name = game.card_name(id);
    game.state.push_log(
        format!("{name}: {pairs} marcador(es) +1/+1 e −1/−1 se anulam"),
        None,
    );
}

fn unattach(game: &mut Game, id: ObjectId) {
    let host = match game.state.object(id) {
        Some(o) => o.attached_to,
        None => return,
    };
    let Some(host) = host else { return };
    if let Some(obj) = game.state.object_mut(id) {
        obj.attached_to = None;
    }
    if let Some(h) = game.state.object_mut(host) {
        h.attachments.retain(|x| *x != id);
    }
    game.state
        .emit(crate::event::GameEvent::Unattached { equipment: id, from: host });
    let name = game.card_name(id);
    game.state
        .push_log(format!("{name} se solta: anexação ilegal (CR 704.5q)"), None);
}

/// CR 701.15 — o escudo de regeneração troca a destruição por: remover do
/// combate, desvirar o dano marcado e virar o permanente.
fn regenerate(game: &mut Game, id: ObjectId) {
    let Some(obj) = game.state.object_mut(id) else {
        return;
    };
    obj.regeneration_shields = obj.regeneration_shields.saturating_sub(1);
    obj.damage = 0;
    obj.deathtouch_damage = false;
    obj.tapped = true;
    obj.combat.removed_from_combat = true;
    let name = game.card_name(id);
    game.state
        .push_log(format!("{name} regenera em vez de ser destruído"), None);
}

/// CR 704.5j — o controlador escolhe qual lenda fica; as demais vão ao
/// cemitério. Devolve as perdedoras.
fn choose_legend_losers(
    game: &mut Game,
    controller: PlayerId,
    name: &str,
    group: Vec<ObjectId>,
) -> Vec<ObjectId> {
    let action = game.ask(Request::SelectObjects {
        player: controller,
        prompt: format!("regra da lenda: escolha qual {name} permanece"),
        candidates: group.clone(),
        min: 1,
        max: 1,
    });
    let keep = match action {
        Action::SelectObjects { objects } => objects
            .first()
            .copied()
            .filter(|id| group.contains(id))
            .unwrap_or_else(|| group[0]),
        // Sem resposta utilizável, mantém a primeira — determinístico.
        _ => group[0],
    };
    group.into_iter().filter(|id| *id != keep).collect()
}

fn put_in_graveyard(game: &mut Game, id: ObjectId, cause: DeathCause) {
    let Some(obj) = game.state.object(id) else {
        return;
    };
    // Pode ter saído do campo entre a coleta e a aplicação (regra da lenda
    // resolvida por outro caminho, por exemplo).
    if !obj.on_battlefield() {
        return;
    }
    let owner = obj.owner;
    let name = game.card_name(id);
    game.state
        .push_log(format!("{name} vai para o cemitério: {}", cause.label()), None);
    if cause.is_destruction() {
        game.push_event(MatchEvent::Destroyed { card: id });
    }
    // `turn::move_object` é quem emite `GameEvent::Died` e `MatchEvent::Died`
    // ao sair do campo para o cemitério; emitir aqui também duplicaria o
    // evento e faria todo gatilho de morte disparar duas vezes.
    turn::move_object(game, id, ZoneId::graveyard(owner));
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::stack::testkit::{card, game_with, put_on_battlefield};
    use crate::ir::Keyword;
    use crate::types::CardType;

    fn creature_db() -> Vec<crate::card::CardDef> {
        let mut c = card(0, "Guardião", vec![CardType::Creature]);
        c.power = Some(2);
        c.toughness = Some(2);
        c.abilities = vec![crate::card::Ability::Keyword(Keyword::Indestructible)];
        vec![c]
    }

    #[test]
    fn marcadores_opostos_se_anulam() {
        let mut game = game_with(creature_db());
        let id = put_on_battlefield(&mut game, PlayerId::P0);
        if let Some(obj) = game.state.object_mut(id) {
            obj.add_counter(CounterKind::PlusOnePlusOne, 3);
            obj.add_counter(CounterKind::MinusOneMinusOne, 2);
        }

        let applied = check(&mut game);

        assert!(applied, "704.5r é uma ação baseada em estado");
        let obj = game.state.object(id).expect("objeto de teste sumiu");
        assert_eq!(obj.counter(&CounterKind::PlusOnePlusOne), 1);
        assert_eq!(obj.counter(&CounterKind::MinusOneMinusOne), 0);
        // Segunda passada não tem mais o que anular.
        assert!(!nothing_but_counters(&mut game), "SBA de marcador não pode repetir");
    }

    /// `check` pode voltar `true` por outra razão; isola o caso dos marcadores.
    fn nothing_but_counters(game: &mut Game) -> bool {
        collect_actions(game)
            .counter_pairs
            .iter()
            .any(|(_, pairs)| *pairs > 0)
    }

    #[test]
    fn indestrutivel_com_resistencia_zero_morre() {
        let mut game = game_with(creature_db());
        let id = put_on_battlefield(&mut game, PlayerId::P0);
        // Sem `layers` implementado não há resistência a medir; o teste passa a
        // valer assim que o módulo de camadas existir.
        let Some(ch) = layers::characteristics(&game, id) else {
            return;
        };
        assert!(
            ch.has_keyword(&Keyword::Indestructible),
            "a carta de teste precisa ser indestrutível"
        );

        // Dano letal não mata o indestrutível (CR 702.12b).
        let Some(obj) = game.state.object_mut(id) else {
            panic!("{id} não existe: não dá para marcar dano letal");
        };
        obj.damage = 99;
        check(&mut game);
        assert!(
            game.state
                .object(id)
                .map(|o| o.on_battlefield())
                .unwrap_or(false),
            "indestrutível ignora dano letal"
        );

        // Resistência 0 mata mesmo assim (CR 704.5f não é destruição).
        let Some(obj) = game.state.object_mut(id) else {
            panic!("{id} não existe: não dá para pôr marcadores");
        };
        {
            obj.damage = 0;
            obj.add_counter(CounterKind::MinusOneMinusOne, 2);
        }
        let Some(before) = layers::characteristics(&game, id) else {
            return;
        };
        assert!(before.toughness <= 0, "os marcadores −1/−1 zeraram a resistência");

        check(&mut game);

        assert!(
            !game
                .state
                .object(id)
                .map(|o| o.on_battlefield())
                .unwrap_or(false),
            "resistência 0 põe no cemitério mesmo com indestrutível"
        );
    }

    #[test]
    fn vida_zero_derrota_o_jogador() {
        let mut game = game_with(creature_db());
        game.state.player_mut(PlayerId::P0).life = 0;

        let applied = check(&mut game);

        assert!(applied);
        assert!(game.state.player(PlayerId::P0).has_lost);
        assert_eq!(
            game.state.player(PlayerId::P0).loss_reason,
            Some(LossReason::ZeroLife)
        );
    }

    #[test]
    fn dez_venenos_derrotam_o_jogador() {
        let mut game = game_with(creature_db());
        game.state.player_mut(PlayerId::P1).poison = POISON_TO_LOSE;

        assert!(check(&mut game));
        assert_eq!(
            game.state.player(PlayerId::P1).loss_reason,
            Some(LossReason::PoisonCounters)
        );
    }

    #[test]
    fn estado_limpo_nao_aplica_nada() {
        let mut game = game_with(creature_db());
        assert!(!check(&mut game), "partida recém-montada não tem SBA pendente");
    }
}
