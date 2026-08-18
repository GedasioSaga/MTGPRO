//! Combate: declarar atacantes e bloqueadores, evasão e dano (CR 506–511).
//!
//! Duas responsabilidades distintas moram aqui:
//!   1. **Enumerar opções** (`attack_options`, `block_options`, ...) — leitura
//!      pura, entregue ao agente para escolher. Limitada por `OPTION_CAP`: o bot
//!      precisa de um leque representativo, não da explosão combinatória (2^n).
//!   2. **Aplicar a escolha** (`declare_*`, `combat_damage_step`) — mutação.
//!
//! O dano de combate é simultâneo (CR 510.2): todas as atribuições são coletadas
//! antes de qualquer ponto de dano ser aplicado. Sem isso, uma criatura que
//! morre no primeiro golpe deixaria de revidar dentro do mesmo passo.

use std::collections::BTreeMap;

use super::query::{self, EvalCtx};
use super::{triggers, Characteristics, Game};
use crate::action::{Action, Request};
use crate::event::{DamageKind, Defender, GameEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::ir::{Keyword, StaticModRuntime};
use crate::types::{CardType, CounterKind};
use crate::view::MatchEvent;

/// Teto de opções enumeradas por decisão de combate.
const OPTION_CAP: usize = 60;
/// Até este número de atacantes, enumeramos todos os 2^n subconjuntos.
const FULL_SUBSET_LIMIT: usize = 5;
/// Marcador de defesa de battle (CR 310.7). Não há variante dedicada em
/// `CounterKind`, então usa-se um marcador nomeado estável.
const DEFENSE_COUNTER: &str = "defense";

// ---------------------------------------------------------------------------
// Leitura: quem pode atacar, quem pode bloquear
// ---------------------------------------------------------------------------

/// Características do objeto se ele for uma criatura no campo de batalha.
fn creature_on_battlefield(game: &Game, id: ObjectId) -> Option<Characteristics> {
    let obj = game.state.object(id)?;
    if !obj.on_battlefield() {
        return None;
    }
    let ch = game.characteristics(id)?;
    if !ch.is_creature() {
        return None;
    }
    Some(ch)
}

/// CR 508.1a — criatura desvirada, sem enjoo de invocação (salvo Pressa), sem
/// Defensor e sem restrição de ataque.
fn can_attack(game: &Game, id: ObjectId, player: PlayerId) -> bool {
    let Some(obj) = game.state.object(id) else {
        return false;
    };
    if obj.tapped {
        return false;
    }
    let Some(ch) = creature_on_battlefield(game, id) else {
        return false;
    };
    if ch.controller != player {
        return false;
    }
    if ch.cant_attack || ch.has_keyword(&Keyword::Defender) {
        return false;
    }
    // CR 302.6 — o enjoo só some com Pressa.
    if obj.summoning_sick && !ch.has_keyword(&Keyword::Haste) {
        return false;
    }
    true
}

pub fn eligible_attackers(game: &Game, player: PlayerId) -> Vec<ObjectId> {
    game.state
        .battlefield()
        .objects
        .iter()
        .copied()
        .filter(|id| can_attack(game, *id, player))
        .collect()
}

/// CR 509.1a — bloqueador precisa estar desvirado e sem restrição de bloqueio.
pub fn eligible_blockers(game: &Game, player: PlayerId) -> Vec<ObjectId> {
    game.state
        .battlefield()
        .objects
        .iter()
        .copied()
        .filter(|id| {
            let Some(obj) = game.state.object(*id) else {
                return false;
            };
            if obj.tapped {
                return false;
            }
            match creature_on_battlefield(game, *id) {
                Some(ch) => ch.controller == player && !ch.cant_block,
                None => false,
            }
        })
        .collect()
}

/// Restrições "só pode ser bloqueada por..." vindas de efeitos contínuos.
/// Não existe campo dedicado em `Characteristics`, então lemos a camada 6
/// diretamente de `state.continuous` (habilidades estáticas também vivem lá).
fn blocked_except_by_filters(game: &Game, attacker: ObjectId) -> Vec<&crate::ir::Filter> {
    game.state
        .continuous
        .iter()
        .filter(|e| e.affected.contains(&attacker))
        .filter_map(|e| match &e.modification {
            StaticModRuntime::CantBeBlockedExceptBy(f) => Some(f),
            _ => None,
        })
        .collect()
}

/// Verdadeiro se o jogador controla algum terreno com o subtipo dado.
fn controls_land_type(game: &Game, player: PlayerId, subtype: &str) -> bool {
    game.state.battlefield().objects.iter().any(|id| {
        game.characteristics(*id).is_some_and(|ch| {
            ch.controller == player
                && ch.type_line.has_type(CardType::Land)
                && ch.type_line.has_subtype(subtype)
        })
    })
}

/// CR 509.1a + palavras-chave de evasão (CR 702). Não valida Ameaçar, que é
/// restrição sobre o conjunto de bloqueadores e não sobre um bloqueador só.
pub fn can_block(game: &Game, blocker: ObjectId, attacker: ObjectId) -> bool {
    let Some(b_obj) = game.state.object(blocker) else {
        return false;
    };
    if b_obj.tapped {
        return false;
    }
    let Some(b) = creature_on_battlefield(game, blocker) else {
        return false;
    };
    if b.cant_block {
        return false;
    }
    let Some(a) = creature_on_battlefield(game, attacker) else {
        return false;
    };
    if a.cant_be_blocked {
        return false;
    }
    // O bloqueador é controlado pelo jogador defensor — é o dono do terreno
    // que interessa para Landwalk.
    let defending = b.controller;

    for kw in &a.keywords {
        match kw {
            // CR 702.9b
            Keyword::Flying => {
                if !b.has_keyword(&Keyword::Flying) && !b.has_keyword(&Keyword::Reach) {
                    return false;
                }
            }
            // CR 702.13a — artefato ou que compartilhe cor.
            Keyword::Intimidate => {
                let artifact = b.type_line.has_type(CardType::Artifact);
                if !artifact && !b.colors.intersects(a.colors) {
                    return false;
                }
            }
            // CR 702.36a — artefato ou preta.
            Keyword::Fear => {
                let artifact = b.type_line.has_type(CardType::Artifact);
                if !artifact && !b.colors.contains(crate::mana::Color::Black) {
                    return false;
                }
            }
            // CR 702.118a — só bloqueia quem tem poder menor ou igual.
            Keyword::Skulk => {
                if b.power > a.power {
                    return false;
                }
            }
            // CR 702.14b — inbloqueável se o defensor controla o tipo de terreno.
            Keyword::Landwalk(subtype) => {
                if controls_land_type(game, defending, subtype) {
                    return false;
                }
            }
            // CR 702.16b — proteção do atacante impede bloqueio pela qualidade.
            Keyword::Protection(color) => {
                if b.colors.contains(*color) {
                    return false;
                }
            }
            _ => {}
        }
    }

    let filters = blocked_except_by_filters(game, attacker);
    if !filters.is_empty() {
        let ctx = EvalCtx::for_source(attacker, a.controller);
        if !filters
            .iter()
            .all(|f| query::matches_filter(game, blocker, f, &ctx))
        {
            return false;
        }
    }
    true
}

/// CR 702.111a — Ameaçar exige dois ou mais bloqueadores.
fn requires_two_blockers(game: &Game, attacker: ObjectId) -> bool {
    game.characteristics(attacker)
        .is_some_and(|ch| ch.has_keyword(&Keyword::Menace))
}

// ---------------------------------------------------------------------------
// Enumeração de opções de ataque
// ---------------------------------------------------------------------------

/// Defensores legais para o jogador ativo: cada oponente vivo e seus
/// planeswalkers (CR 506.2).
fn legal_defenders(game: &Game, player: PlayerId) -> Vec<Defender> {
    let mut out = Vec::new();
    for opp in game.state.opponents(player) {
        if game.state.player(opp).has_lost {
            continue;
        }
        out.push(Defender::Player(opp));
        for id in &game.state.battlefield().objects {
            let is_pw = game.characteristics(*id).is_some_and(|ch| {
                ch.controller == opp && ch.type_line.has_type(CardType::Planeswalker)
            });
            if is_pw {
                out.push(Defender::Planeswalker(*id));
            }
        }
    }
    out
}

fn power_of(game: &Game, id: ObjectId) -> i32 {
    game.characteristics(id).map_or(0, |ch| ch.power)
}

/// Subconjuntos de atacantes, determinísticos e limitados.
/// Até `FULL_SUBSET_LIMIT` criaturas: todos os subconjuntos. Acima disso,
/// prefixos do ranking de poder (o "ataque com os N maiores") mais todos os
/// subconjuntos das `FULL_SUBSET_LIMIT` mais fortes.
fn attacker_subsets(game: &Game, eligible: &[ObjectId]) -> Vec<Vec<ObjectId>> {
    let n = eligible.len();
    let mut out: Vec<Vec<ObjectId>> = Vec::new();
    if n == 0 {
        return out;
    }
    if n <= FULL_SUBSET_LIMIT {
        for mask in 1u32..(1u32 << n) {
            let subset: Vec<ObjectId> = eligible
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, id)| *id)
                .collect();
            out.push(subset);
        }
        return out;
    }

    let mut ranked = eligible.to_vec();
    // Desempate por id mantém a ordem estável entre execuções com a mesma semente.
    ranked.sort_by_key(|id| (std::cmp::Reverse(power_of(game, *id)), *id));

    for k in 1..=n {
        let mut subset: Vec<ObjectId> = ranked[..k].to_vec();
        subset.sort();
        if !out.contains(&subset) {
            out.push(subset);
        }
    }
    let top = &ranked[..FULL_SUBSET_LIMIT];
    for mask in 1u32..(1u32 << FULL_SUBSET_LIMIT) {
        let mut subset: Vec<ObjectId> = top
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, id)| *id)
            .collect();
        subset.sort();
        if !out.contains(&subset) {
            out.push(subset);
        }
    }
    out
}

pub fn attack_options(game: &Game, player: PlayerId, eligible: &[ObjectId]) -> Vec<Action> {
    let usable: Vec<ObjectId> = eligible
        .iter()
        .copied()
        .filter(|id| can_attack(game, *id, player))
        .collect();

    // "Não atacar" é sempre legal (CR 508.1) e vem primeiro.
    let mut out = vec![Action::Attack { assignments: Vec::new() }];

    let defenders = legal_defenders(game, player);
    let Some(default_defender) = defenders.first().copied() else {
        return out;
    };
    if usable.is_empty() {
        return out;
    }

    // Garantia do contrato: "atacar com todos" existe mesmo se o teto cortar.
    let all: Vec<(ObjectId, Defender)> = usable.iter().map(|id| (*id, default_defender)).collect();
    out.push(Action::Attack { assignments: all });

    for subset in attacker_subsets(game, &usable) {
        for def in &defenders {
            if out.len() >= OPTION_CAP {
                return out;
            }
            let assignments: Vec<(ObjectId, Defender)> =
                subset.iter().map(|id| (*id, *def)).collect();
            let action = Action::Attack { assignments };
            if !out.contains(&action) {
                out.push(action);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Enumeração de opções de bloqueio
// ---------------------------------------------------------------------------

/// CR 509.1b — nenhum atacante com Ameaçar pode ter exatamente um bloqueador.
fn blocks_are_legal(game: &Game, assignments: &[(ObjectId, ObjectId)]) -> bool {
    let mut counts: BTreeMap<ObjectId, usize> = BTreeMap::new();
    for (_, attacker) in assignments {
        *counts.entry(*attacker).or_insert(0) += 1;
    }
    counts
        .iter()
        .all(|(attacker, n)| *n != 1 || !requires_two_blockers(game, *attacker))
}

fn block_dfs(
    game: &Game,
    legal_for: &[(ObjectId, Vec<ObjectId>)],
    idx: usize,
    current: &mut Vec<(ObjectId, ObjectId)>,
    out: &mut Vec<Action>,
) {
    if out.len() >= OPTION_CAP {
        return;
    }
    if idx == legal_for.len() {
        if current.is_empty() {
            return; // "não bloquear" já está na lista
        }
        if !blocks_are_legal(game, current) {
            return;
        }
        let action = Action::Block { assignments: current.clone() };
        if !out.contains(&action) {
            out.push(action);
        }
        return;
    }
    // Ramo "este bloqueador não bloqueia".
    block_dfs(game, legal_for, idx + 1, current, out);
    let (blocker, attackers) = &legal_for[idx];
    for attacker in attackers {
        current.push((*blocker, *attacker));
        block_dfs(game, legal_for, idx + 1, current, out);
        current.pop();
        if out.len() >= OPTION_CAP {
            return;
        }
    }
}

/// Plano guloso "todo mundo bloqueia o primeiro atacante que puder", já
/// saneado contra Ameaçar. Serve de opção óbvia no topo da lista.
fn greedy_block(game: &Game, legal_for: &[(ObjectId, Vec<ObjectId>)]) -> Vec<(ObjectId, ObjectId)> {
    let mut plan: Vec<(ObjectId, ObjectId)> = legal_for
        .iter()
        .filter_map(|(b, atts)| atts.first().map(|a| (*b, *a)))
        .collect();

    // Remove bloqueios solitários em atacantes com Ameaçar até estabilizar.
    loop {
        let mut counts: BTreeMap<ObjectId, usize> = BTreeMap::new();
        for (_, a) in &plan {
            *counts.entry(*a).or_insert(0) += 1;
        }
        let bad: Vec<ObjectId> = counts
            .iter()
            .filter(|(a, n)| **n == 1 && requires_two_blockers(game, **a))
            .map(|(a, _)| *a)
            .collect();
        if bad.is_empty() {
            return plan;
        }
        plan.retain(|(_, a)| !bad.contains(a));
    }
}

pub fn block_options(
    game: &Game,
    player: PlayerId,
    eligible: &[ObjectId],
    attackers: &[ObjectId],
) -> Vec<Action> {
    // "Não bloquear" é sempre legal (CR 509.1) e vem primeiro.
    let mut out = vec![Action::Block { assignments: Vec::new() }];

    let legal_for: Vec<(ObjectId, Vec<ObjectId>)> = eligible
        .iter()
        .copied()
        .filter(|b| {
            game.characteristics(*b)
                .is_some_and(|ch| ch.controller == player)
        })
        .map(|b| {
            let atts: Vec<ObjectId> = attackers
                .iter()
                .copied()
                .filter(|a| can_block(game, b, *a))
                .collect();
            (b, atts)
        })
        .filter(|(_, atts)| !atts.is_empty())
        .collect();

    if legal_for.is_empty() {
        return out;
    }

    let greedy = greedy_block(game, &legal_for);
    if !greedy.is_empty() {
        out.push(Action::Block { assignments: greedy });
    }

    let mut current = Vec::new();
    block_dfs(game, &legal_for, 0, &mut current, &mut out);
    out
}

// ---------------------------------------------------------------------------
// Ordenação de bloqueadores e distribuição de dano
// ---------------------------------------------------------------------------

fn permute(items: &[ObjectId], used: &mut [bool], cur: &mut Vec<ObjectId>, out: &mut Vec<Vec<ObjectId>>) {
    if out.len() >= OPTION_CAP {
        return;
    }
    if cur.len() == items.len() {
        out.push(cur.clone());
        return;
    }
    for i in 0..items.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        cur.push(items[i]);
        permute(items, used, cur, out);
        cur.pop();
        used[i] = false;
        if out.len() >= OPTION_CAP {
            return;
        }
    }
}

/// CR 509.2 — o atacante ordena seus bloqueadores. A primeira opção é sempre a
/// ordem em que os bloqueios foram declarados.
pub fn order_options(attacker: ObjectId, blockers: &[ObjectId]) -> Vec<Action> {
    if blockers.len() < 2 {
        return vec![Action::OrderBlockers { attacker, order: blockers.to_vec() }];
    }
    let mut orders = Vec::new();
    let mut used = vec![false; blockers.len()];
    let mut cur = Vec::with_capacity(blockers.len());
    permute(blockers, &mut used, &mut cur, &mut orders);
    orders
        .into_iter()
        .map(|order| Action::OrderBlockers { attacker, order })
        .collect()
}

/// Dano necessário para ser letal a cada bloqueador, na ordem dada.
/// CR 702.2b — com Toque Mortal, qualquer 1 ponto já é letal.
fn lethal_requirements(game: &Game, source: ObjectId, blockers: &[ObjectId]) -> Vec<i32> {
    let deathtouch = game
        .characteristics(source)
        .is_some_and(|ch| ch.has_keyword(&Keyword::Deathtouch));
    blockers
        .iter()
        .map(|b| {
            if deathtouch {
                return 1;
            }
            match (game.characteristics(*b), game.state.object(*b)) {
                (Some(ch), Some(obj)) => (ch.toughness - obj.damage).max(1),
                _ => 1,
            }
        })
        .collect()
}

/// Distribuição padrão: letal em cada bloqueador na ordem, e o excedente vai
/// ao defensor se houver Atropelar (CR 702.19b), senão ao último bloqueador.
fn canonical_assignment(
    game: &Game,
    attacker: ObjectId,
    blockers: &[ObjectId],
    total: i32,
) -> (Vec<(ObjectId, i32)>, i32) {
    let trample = game
        .characteristics(attacker)
        .is_some_and(|ch| ch.has_keyword(&Keyword::Trample));
    let lethal = lethal_requirements(game, attacker, blockers);

    let mut left = total.max(0);
    let mut assignments: Vec<(ObjectId, i32)> = Vec::new();
    for (i, b) in blockers.iter().enumerate() {
        if left <= 0 {
            break;
        }
        let give = lethal[i].min(left);
        assignments.push((*b, give));
        left -= give;
    }
    if left <= 0 {
        return (assignments, 0);
    }
    if trample {
        return (assignments, left);
    }
    if let Some(last) = assignments.last_mut() {
        last.1 += left;
    }
    (assignments, 0)
}

pub fn damage_assignment_options(
    game: &Game,
    attacker: ObjectId,
    blockers: &[ObjectId],
    total: i32,
) -> Vec<Action> {
    let total = total.max(0);
    if blockers.is_empty() || total == 0 {
        return vec![Action::AssignDamage {
            attacker,
            assignments: Vec::new(),
            trample_to_defender: 0,
        }];
    }

    let trample = game
        .characteristics(attacker)
        .is_some_and(|ch| ch.has_keyword(&Keyword::Trample));
    let lethal = lethal_requirements(game, attacker, blockers);

    let (canon, canon_trample) = canonical_assignment(game, attacker, blockers, total);
    let mut out = vec![Action::AssignDamage {
        attacker,
        assignments: canon,
        trample_to_defender: canon_trample,
    }];

    // Variantes por ponto de corte: letal nos `cut` primeiros e todo o resto no
    // seguinte. Toda variante respeita CR 510.1c por construção.
    for cut in 0..blockers.len() {
        if out.len() >= OPTION_CAP {
            return out;
        }
        let spent: i32 = lethal[..cut].iter().sum();
        if spent >= total {
            break;
        }
        let mut assignments: Vec<(ObjectId, i32)> = blockers[..cut]
            .iter()
            .enumerate()
            .map(|(i, b)| (*b, lethal[i]))
            .collect();
        assignments.push((blockers[cut], total - spent));
        let action = Action::AssignDamage { attacker, assignments, trample_to_defender: 0 };
        if !out.contains(&action) {
            out.push(action);
        }
    }

    // Com Atropelar, mandar o mínimo aos bloqueadores é uma opção distinta.
    if trample {
        let needed: i32 = lethal.iter().sum();
        if needed < total {
            let assignments: Vec<(ObjectId, i32)> = blockers
                .iter()
                .enumerate()
                .map(|(i, b)| (*b, lethal[i]))
                .collect();
            let action = Action::AssignDamage {
                attacker,
                assignments,
                trample_to_defender: total - needed,
            };
            if !out.contains(&action) {
                out.push(action);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Declaração de atacantes
// ---------------------------------------------------------------------------

pub fn declare_attackers(game: &mut Game, assignments: &[(ObjectId, Defender)]) {
    let mut declared: Vec<(ObjectId, Defender)> = Vec::new();

    for (creature, defender) in assignments {
        let controller = match game.characteristics(*creature) {
            Some(ch) => ch.controller,
            None => continue,
        };
        if !can_attack(game, *creature, controller) {
            let name = game.card_name(*creature);
            game.state
                .push_log(format!("ataque ilegal ignorado: {name}"), Some(controller));
            continue;
        }
        let vigilance = game
            .characteristics(*creature)
            .is_some_and(|ch| ch.has_keyword(&Keyword::Vigilance));

        if let Some(obj) = game.state.object_mut(*creature) {
            obj.combat.attacking = Some(*defender);
            obj.combat.removed_from_combat = false;
        }
        // CR 508.1f — atacar vira a criatura, salvo Vigilância (CR 702.20b).
        if !vigilance {
            let already_tapped = game.state.object(*creature).is_some_and(|o| o.tapped);
            if !already_tapped {
                if let Some(obj) = game.state.object_mut(*creature) {
                    obj.tapped = true;
                }
                game.state.emit(GameEvent::Tapped { object: *creature });
                game.push_event(MatchEvent::Tapped { card: *creature });
            }
        }
        declared.push((*creature, *defender));
    }

    if declared.is_empty() {
        game.state.push_log("nenhum atacante declarado", None);
        return;
    }

    for (creature, defender) in &declared {
        let name = game.card_name(*creature);
        game.state.push_log(format!("{name} ataca"), None);
        game.state.emit(GameEvent::Attacked { object: *creature, defender: *defender });
    }
    let ids: Vec<ObjectId> = declared.iter().map(|(id, _)| *id).collect();
    game.state.emit(GameEvent::AttackersDeclared { attackers: ids });
    game.push_event(MatchEvent::AttackersDeclared { attackers: declared });

    // CR 508.2 — gatilhos de "ataca" disparam depois da declaração inteira.
    triggers::collect(game);
}

// ---------------------------------------------------------------------------
// Declaração de bloqueadores
// ---------------------------------------------------------------------------

pub fn declare_blockers(game: &mut Game, assignments: &[(ObjectId, ObjectId)]) {
    let mut by_attacker: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
    for (blocker, attacker) in assignments {
        let attacking = game
            .state
            .object(*attacker)
            .is_some_and(|o| o.combat.is_attacking());
        if !attacking || !can_block(game, *blocker, *attacker) {
            let name = game.card_name(*blocker);
            game.state
                .push_log(format!("bloqueio ilegal ignorado: {name}"), None);
            continue;
        }
        let entry = by_attacker.entry(*attacker).or_default();
        if !entry.contains(blocker) {
            entry.push(*blocker);
        }
    }

    // CR 509.1b — Ameaçar torna ilegal o bloqueio por uma criatura só.
    let illegal: Vec<ObjectId> = by_attacker
        .iter()
        .filter(|(a, bs)| bs.len() == 1 && requires_two_blockers(game, **a))
        .map(|(a, _)| *a)
        .collect();
    for attacker in illegal {
        let name = game.card_name(attacker);
        game.state
            .push_log(format!("{name} tem Ameaçar: bloqueio único descartado"), None);
        by_attacker.remove(&attacker);
    }

    let attackers: Vec<ObjectId> = by_attacker.keys().copied().collect();
    let mut blocks: Vec<(ObjectId, Vec<ObjectId>)> = Vec::new();

    for attacker in attackers {
        let blockers = by_attacker.get(&attacker).cloned().unwrap_or_default();
        // CR 509.2 — com dois ou mais bloqueadores, o atacante escolhe a ordem
        // em que o dano será atribuído.
        let ordered = if blockers.len() >= 2 {
            let controller = match game.state.object(attacker) {
                Some(o) => o.controller,
                None => continue,
            };
            let request = Request::OrderBlockers {
                player: controller,
                attacker,
                blockers: blockers.clone(),
            };
            match game.ask(request) {
                Action::OrderBlockers { order, .. }
                    if order.len() == blockers.len() && order.iter().all(|b| blockers.contains(b)) =>
                {
                    order
                }
                _ => blockers.clone(),
            }
        } else {
            blockers.clone()
        };

        if let Some(obj) = game.state.object_mut(attacker) {
            obj.combat.blocked_by = ordered.clone();
            // CR 509.1h — uma vez bloqueada, a criatura segue bloqueada mesmo
            // que os bloqueadores deixem o combate.
            obj.combat.was_blocked = true;
        }
        for blocker in &ordered {
            if let Some(obj) = game.state.object_mut(*blocker) {
                if !obj.combat.blocking.contains(&attacker) {
                    obj.combat.blocking.push(attacker);
                }
            }
        }
        let name = game.card_name(attacker);
        game.state
            .push_log(format!("{name} foi bloqueada por {} criatura(s)", ordered.len()), None);
        game.state.emit(GameEvent::Blocked { attacker, blockers: ordered.clone() });
        blocks.push((attacker, ordered));
    }

    // Atacantes sem bloqueador ficam desbloqueados (CR 509.1h não se aplica).
    let unblocked: Vec<ObjectId> = game
        .state
        .battlefield()
        .objects
        .iter()
        .copied()
        .filter(|id| {
            game.state
                .object(*id)
                .is_some_and(|o| o.combat.is_attacking() && !o.combat.was_blocked)
        })
        .collect();
    for id in unblocked {
        game.state.emit(GameEvent::BecameUnblocked { attacker: id });
    }

    game.state.emit(GameEvent::BlockersDeclared { blocks: blocks.clone() });
    game.push_event(MatchEvent::BlockersDeclared { blocks });

    // CR 509.4 — gatilhos de bloqueio disparam depois da declaração inteira.
    triggers::collect(game);
}

// ---------------------------------------------------------------------------
// Dano de combate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DamageTarget {
    Object(ObjectId),
    Player(PlayerId),
}

#[derive(Debug, Clone, Copy)]
struct DamageAssign {
    source: ObjectId,
    target: DamageTarget,
    amount: i32,
    deathtouch: bool,
    lifelink: bool,
}

/// CR 510.4 — Iniciativa causa dano só no primeiro passo, Golpe Duplo nos dois,
/// e o resto só no passo normal.
fn deals_damage_in_step(ch: &Characteristics, first_strike_step: bool) -> bool {
    let first = ch.has_keyword(&Keyword::FirstStrike);
    let double = ch.has_keyword(&Keyword::DoubleStrike);
    if first_strike_step {
        first || double
    } else {
        !first || double
    }
}

pub fn has_first_strike_creatures(game: &Game) -> bool {
    game.state.battlefield().objects.iter().any(|id| {
        let in_combat = game
            .state
            .object(*id)
            .is_some_and(|o| o.combat.is_attacking() || o.combat.is_blocking());
        if !in_combat {
            return false;
        }
        game.characteristics(*id).is_some_and(|ch| {
            ch.has_keyword(&Keyword::FirstStrike) || ch.has_keyword(&Keyword::DoubleStrike)
        })
    })
}

fn defender_target(game: &Game, defender: Defender) -> DamageTarget {
    match defender {
        Defender::Player(p) => DamageTarget::Player(p),
        Defender::Planeswalker(o) | Defender::Battle(o) => {
            // Planeswalker/battle que já saiu do campo devolve o dano ao seu
            // controlador não é regra: o dano simplesmente não é causado.
            let _ = game;
            DamageTarget::Object(o)
        }
    }
}

/// Bloqueadores que ainda estão no campo e ainda bloqueiam este atacante.
fn live_blockers(game: &Game, attacker: ObjectId) -> Vec<ObjectId> {
    let Some(obj) = game.state.object(attacker) else {
        return Vec::new();
    };
    obj.combat
        .blocked_by
        .iter()
        .copied()
        .filter(|b| {
            game.state.object(*b).is_some_and(|o| {
                o.on_battlefield() && !o.combat.removed_from_combat && o.combat.blocking.contains(&attacker)
            })
        })
        .collect()
}

/// Coleta o dano de um atacante. Pode perguntar ao agente como distribuir.
fn collect_attacker_damage(game: &mut Game, attacker: ObjectId, out: &mut Vec<DamageAssign>) {
    let Some(ch) = game.characteristics(attacker) else {
        return;
    };
    let power = ch.power.max(0);
    let deathtouch = ch.has_keyword(&Keyword::Deathtouch);
    let lifelink = ch.has_keyword(&Keyword::Lifelink);
    let trample = ch.has_keyword(&Keyword::Trample);
    let Some(defender) = game.state.object(attacker).and_then(|o| o.combat.attacking) else {
        return;
    };
    let was_blocked = game
        .state
        .object(attacker)
        .is_some_and(|o| o.combat.was_blocked);
    if power == 0 {
        return;
    }

    if !was_blocked {
        out.push(DamageAssign {
            source: attacker,
            target: defender_target(game, defender),
            amount: power,
            deathtouch,
            lifelink,
        });
        return;
    }

    let blockers = live_blockers(game, attacker);
    if blockers.is_empty() {
        // CR 509.1h — segue bloqueada, então não atinge o defensor...
        if trample {
            // ...salvo Atropelar, que manda tudo ao defensor (CR 702.19b).
            out.push(DamageAssign {
                source: attacker,
                target: defender_target(game, defender),
                amount: power,
                deathtouch,
                lifelink,
            });
        } else {
            let name = game.card_name(attacker);
            game.state
                .push_log(format!("{name} está bloqueada sem bloqueadores: nenhum dano"), None);
        }
        return;
    }

    // Só há decisão real com dois ou mais bloqueadores ou com Atropelar.
    let (assignments, to_defender) = if blockers.len() >= 2 || trample {
        let controller = match game.state.object(attacker) {
            Some(o) => o.controller,
            None => return,
        };
        let request = Request::AssignCombatDamage {
            player: controller,
            attacker,
            blockers: blockers.clone(),
            total: power,
        };
        match game.ask(request) {
            Action::AssignDamage { assignments, trample_to_defender, .. } => {
                let sum: i32 = assignments.iter().map(|(_, n)| *n).sum::<i32>() + trample_to_defender;
                let known = assignments.iter().all(|(b, n)| blockers.contains(b) && *n >= 0);
                if sum <= power && known && (trample || trample_to_defender == 0) {
                    (assignments, trample_to_defender)
                } else {
                    canonical_assignment(game, attacker, &blockers, power)
                }
            }
            _ => canonical_assignment(game, attacker, &blockers, power),
        }
    } else {
        canonical_assignment(game, attacker, &blockers, power)
    };

    for (blocker, amount) in assignments {
        if amount <= 0 {
            continue;
        }
        out.push(DamageAssign {
            source: attacker,
            target: DamageTarget::Object(blocker),
            amount,
            deathtouch,
            lifelink,
        });
    }
    if to_defender > 0 {
        out.push(DamageAssign {
            source: attacker,
            target: defender_target(game, defender),
            amount: to_defender,
            deathtouch,
            lifelink,
        });
    }
}

fn collect_blocker_damage(game: &Game, blocker: ObjectId, out: &mut Vec<DamageAssign>) {
    let Some(ch) = game.characteristics(blocker) else {
        return;
    };
    let power = ch.power.max(0);
    if power == 0 {
        return;
    }
    let Some(obj) = game.state.object(blocker) else {
        return;
    };
    // Bloqueando várias criaturas (raro), o dano vai todo à primeira ainda em
    // combate — divisão livre exigiria uma request que o contrato não prevê.
    let target = obj.combat.blocking.iter().copied().find(|a| {
        game.state
            .object(*a)
            .is_some_and(|o| o.on_battlefield() && o.combat.is_attacking())
    });
    let Some(attacker) = target else {
        return;
    };
    out.push(DamageAssign {
        source: blocker,
        target: DamageTarget::Object(attacker),
        amount: power,
        deathtouch: ch.has_keyword(&Keyword::Deathtouch),
        lifelink: ch.has_keyword(&Keyword::Lifelink),
    });
}

/// CR 702.16e — proteção previne dano da qualidade indicada.
fn damage_is_prevented(game: &Game, victim: &Characteristics, source: ObjectId) -> bool {
    if victim.prevent_all_damage {
        return true;
    }
    let Some(src) = game.characteristics(source) else {
        return false;
    };
    victim
        .keywords
        .iter()
        .any(|k| matches!(k, Keyword::Protection(c) if src.colors.contains(*c)))
}

fn apply_lifelink(game: &mut Game, assign: &DamageAssign) {
    if !assign.lifelink || assign.amount <= 0 {
        return;
    }
    let Some(controller) = game.state.object(assign.source).map(|o| o.controller) else {
        return;
    };
    // CR 702.15a — vínculo com a vida é parte do mesmo evento de dano.
    let player = game.state.player_mut(controller);
    player.life += assign.amount;
    player.life_gained_this_turn += assign.amount;
    let total = player.life;
    game.state
        .emit(GameEvent::LifeGained { player: controller, amount: assign.amount });
    game.push_event(MatchEvent::LifeChanged { player: controller, delta: assign.amount, total });
}

fn apply_damage_to_object(game: &mut Game, assign: &DamageAssign, victim: ObjectId) {
    let Some(ch) = game.characteristics(victim) else {
        return;
    };
    let on_battlefield = game.state.object(victim).is_some_and(|o| o.on_battlefield());
    if !on_battlefield {
        return;
    }
    if damage_is_prevented(game, &ch, assign.source) {
        let name = game.card_name(victim);
        game.state
            .push_log(format!("dano a {name} foi prevenido"), None);
        return;
    }

    let lethal;
    if ch.type_line.has_type(CardType::Planeswalker) {
        // CR 306.8 — dano a planeswalker remove marcadores de lealdade.
        let current = game
            .state
            .object(victim)
            .map_or(0, |o| o.counter(&CounterKind::Loyalty));
        let removed = assign.amount.min(current).max(0);
        if let Some(obj) = game.state.object_mut(victim) {
            obj.add_counter(CounterKind::Loyalty, -removed);
        }
        if removed > 0 {
            game.state.emit(GameEvent::CountersRemoved {
                object: victim,
                kind: CounterKind::Loyalty,
                amount: removed,
            });
            game.push_event(MatchEvent::CountersChanged {
                card: victim,
                kind: "Loyalty".to_string(),
                delta: -removed,
            });
        }
        lethal = current - removed <= 0;
    } else if ch.type_line.has_type(CardType::Battle) {
        // CR 310.8 — dano a battle remove marcadores de defesa.
        let kind = CounterKind::Named(DEFENSE_COUNTER.to_string());
        let current = game.state.object(victim).map_or(0, |o| o.counter(&kind));
        let removed = assign.amount.min(current).max(0);
        if let Some(obj) = game.state.object_mut(victim) {
            obj.add_counter(kind.clone(), -removed);
        }
        if removed > 0 {
            game.state
                .emit(GameEvent::CountersRemoved { object: victim, kind, amount: removed });
        }
        lethal = current - removed <= 0;
    } else {
        if let Some(obj) = game.state.object_mut(victim) {
            obj.damage += assign.amount;
            if assign.deathtouch {
                // CR 704.5h — marcado para a SBA destruir depois.
                obj.deathtouch_damage = true;
            }
        }
        let marked = game.state.object(victim).map_or(0, |o| o.damage);
        lethal = assign.deathtouch || (ch.toughness > 0 && marked >= ch.toughness);
    }

    game.state.emit(GameEvent::DamageDealt {
        source: assign.source,
        target: victim,
        amount: assign.amount,
        kind: DamageKind::Combat,
        deathtouch: assign.deathtouch,
    });
    game.push_event(MatchEvent::DamageDealt {
        source: assign.source,
        target: victim,
        amount: assign.amount,
        lethal,
    });
    apply_lifelink(game, assign);
}

fn apply_damage_to_player(game: &mut Game, assign: &DamageAssign, victim: PlayerId) {
    let player = game.state.player_mut(victim);
    player.life -= assign.amount;
    player.damage_taken_this_turn += assign.amount;
    player.life_lost_this_turn += assign.amount;
    let total = player.life;

    game.state.emit(GameEvent::DamageDealtToPlayer {
        source: assign.source,
        player: victim,
        amount: assign.amount,
        kind: DamageKind::Combat,
    });
    // CR 118.2 — dano a jogador é perda de vida, e gatilhos de perda observam isso.
    game.state
        .emit(GameEvent::LifeLost { player: victim, amount: assign.amount });
    game.push_event(MatchEvent::DamageToPlayer {
        source: assign.source,
        player: victim,
        amount: assign.amount,
    });
    game.push_event(MatchEvent::LifeChanged { player: victim, delta: -assign.amount, total });
    apply_lifelink(game, assign);
}

pub fn combat_damage_step(game: &mut Game, first_strike: bool) {
    let in_combat: Vec<ObjectId> = game
        .state
        .battlefield()
        .objects
        .iter()
        .copied()
        .filter(|id| {
            game.state
                .object(*id)
                .is_some_and(|o| o.combat.is_attacking() || o.combat.is_blocking())
        })
        .collect();

    let mut assigns: Vec<DamageAssign> = Vec::new();

    // Coleta primeiro, aplica depois: o dano de combate é simultâneo (CR 510.2).
    for id in &in_combat {
        let deals = game
            .characteristics(*id)
            .is_some_and(|ch| deals_damage_in_step(&ch, first_strike));
        if !deals {
            continue;
        }
        let attacking = game
            .state
            .object(*id)
            .is_some_and(|o| o.combat.is_attacking());
        if attacking {
            collect_attacker_damage(game, *id, &mut assigns);
        } else {
            collect_blocker_damage(game, *id, &mut assigns);
        }
    }

    if assigns.is_empty() {
        if first_strike {
            game.state.first_strike_done = true;
        }
        return;
    }

    for assign in &assigns {
        match assign.target {
            DamageTarget::Object(o) => apply_damage_to_object(game, assign, o),
            DamageTarget::Player(p) => apply_damage_to_player(game, assign, p),
        }
    }

    game.state.emit(GameEvent::CombatDamageDealt);
    if first_strike {
        game.state.first_strike_done = true;
    }
    triggers::collect(game);
}

// ---------------------------------------------------------------------------
// Fim de combate
// ---------------------------------------------------------------------------

/// CR 511.3 — todas as criaturas deixam o combate quando a fase termina.
pub fn end_combat(game: &mut Game) {
    let ids: Vec<ObjectId> = game.state.battlefield().objects.clone();
    let mut cleared = false;
    for id in ids {
        if let Some(obj) = game.state.object_mut(id) {
            if obj.combat != crate::state::CombatState::default() {
                obj.combat.clear();
                cleared = true;
            }
        }
    }
    game.state.first_strike_done = false;
    if cleared {
        game.state.push_log("fim do combate", None);
        game.push_event(MatchEvent::Log { text: "Fim do combate.".to_string() });
    }
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Ability, CardDatabase, CardDef};
    use crate::engine::{Agent, FirstLegalAgent, GameConfig};
    use crate::ids::{CardDefId, IdGen};
    use crate::ir::Keyword;
    use crate::mana::ManaCost;
    use crate::state::{GameOutcome, GameState, ObjectState, PlayerState};
    use crate::types::{CardType, Rarity, TypeLine};
    use crate::zone::{Zone, ZoneId, ZoneKind};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::sync::Arc;

    fn creature_def(id: u32, name: &str, power: i32, toughness: i32, kws: &[Keyword]) -> CardDef {
        CardDef {
            id: CardDefId(id),
            name: name.to_string(),
            mana_cost: ManaCost::FREE,
            type_line: TypeLine {
                supertypes: Vec::new(),
                types: vec![CardType::Creature],
                subtypes: Vec::new(),
            },
            color_override: None,
            power: Some(power),
            toughness: Some(toughness),
            loyalty: None,
            abilities: kws.iter().cloned().map(Ability::Keyword).collect(),
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

    fn empty_zones(players: u8) -> BTreeMap<(ZoneKind, u8), Zone> {
        let mut zones = BTreeMap::new();
        zones.insert((ZoneKind::Battlefield, u8::MAX), Zone::new(ZoneKind::Battlefield));
        zones.insert((ZoneKind::Stack, u8::MAX), Zone::new(ZoneKind::Stack));
        zones.insert((ZoneKind::Exile, u8::MAX), Zone::new(ZoneKind::Exile));
        zones.insert((ZoneKind::Command, u8::MAX), Zone::new(ZoneKind::Command));
        for p in 0..players {
            zones.insert((ZoneKind::Library, p), Zone::new(ZoneKind::Library));
            zones.insert((ZoneKind::Hand, p), Zone::new(ZoneKind::Hand));
            zones.insert((ZoneKind::Graveyard, p), Zone::new(ZoneKind::Graveyard));
        }
        zones
    }

    /// Monta um jogo mínimo com dois jogadores e nenhum permanente.
    fn make_game(defs: Vec<CardDef>) -> Game {
        let db = CardDatabase { cards: defs };
        let state = GameState {
            players: vec![
                PlayerState::new(PlayerId::P0, "A", 20),
                PlayerState::new(PlayerId::P1, "B", 20),
            ],
            objects: Vec::new(),
            zones: empty_zones(2),
            stack: Vec::new(),
            continuous: Vec::new(),
            turn: 1,
            active_player: PlayerId::P0,
            priority_player: PlayerId::P0,
            step: crate::event::Step::DeclareBlockers,
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
        let agents: Vec<Box<dyn Agent>> =
            vec![Box::new(FirstLegalAgent), Box::new(FirstLegalAgent)];
        Game {
            state,
            db: Arc::new(db),
            rng: ChaCha8Rng::seed_from_u64(7),
            config: GameConfig::default(),
            agents,
            match_events: Vec::new(),
            decisions_made: 0,
            seed: 7,
        }
    }

    /// Coloca uma criatura no campo de batalha e devolve seu id.
    fn put_creature(game: &mut Game, card: u32, controller: PlayerId) -> ObjectId {
        let id = game.state.id_gen.next_object();
        let ts = game.state.next_timestamp();
        let mut obj = ObjectState::new(id, CardDefId(card), controller, ZoneId::BATTLEFIELD, ts);
        obj.controller = controller;
        obj.summoning_sick = false;
        game.state.objects.push(obj);
        game.state.zone_mut(ZoneId::BATTLEFIELD).push_bottom(id);
        id
    }

    fn attack(game: &mut Game, attacker: ObjectId, defender: PlayerId) {
        declare_attackers(game, &[(attacker, Defender::Player(defender))]);
    }

    #[test]
    fn trample_passa_excedente_ao_defensor() {
        let defs = vec![
            creature_def(0, "Rinoceronte", 4, 4, &[Keyword::Trample]),
            creature_def(1, "Muda", 1, 1, &[]),
        ];
        let mut game = make_game(defs);
        let big = put_creature(&mut game, 0, PlayerId::P0);
        let chump = put_creature(&mut game, 1, PlayerId::P1);

        attack(&mut game, big, PlayerId::P1);
        declare_blockers(&mut game, &[(chump, big)]);
        combat_damage_step(&mut game, false);

        // 1 letal no bloqueador, 3 atropelam para o jogador.
        assert_eq!(game.state.object(chump).map(|o| o.damage), Some(1));
        assert_eq!(game.state.player(PlayerId::P1).life, 17);
    }

    #[test]
    fn toque_mortal_torna_um_de_dano_letal() {
        let defs = vec![
            creature_def(0, "Aranha Venenosa", 1, 1, &[Keyword::Deathtouch]),
            creature_def(1, "Gigante", 5, 5, &[]),
        ];
        let mut game = make_game(defs);
        let poisonous = put_creature(&mut game, 0, PlayerId::P0);
        let giant = put_creature(&mut game, 1, PlayerId::P1);

        attack(&mut game, poisonous, PlayerId::P1);
        declare_blockers(&mut game, &[(giant, poisonous)]);
        combat_damage_step(&mut game, false);

        let blocked = game.state.object(giant).expect("bloqueador existe");
        assert_eq!(blocked.damage, 1);
        assert!(blocked.deathtouch_damage, "CR 704.5h precisa da marca");
    }

    #[test]
    fn iniciativa_causa_dano_antes_do_passo_normal() {
        let defs = vec![
            creature_def(0, "Cavaleiro", 2, 2, &[Keyword::FirstStrike]),
            creature_def(1, "Urso", 2, 2, &[]),
        ];
        let mut game = make_game(defs);
        let knight = put_creature(&mut game, 0, PlayerId::P0);
        let bear = put_creature(&mut game, 1, PlayerId::P1);

        attack(&mut game, knight, PlayerId::P1);
        declare_blockers(&mut game, &[(bear, knight)]);

        assert!(has_first_strike_creatures(&game));
        combat_damage_step(&mut game, true);
        assert_eq!(game.state.object(bear).map(|o| o.damage), Some(2));
        assert_eq!(game.state.object(knight).map(|o| o.damage), Some(0));

        // A SBA (fora deste módulo) tiraria o urso do campo; simulamos isso.
        game.state.zone_mut(ZoneId::BATTLEFIELD).remove(bear);
        if let Some(obj) = game.state.object_mut(bear) {
            obj.zone = ZoneId::graveyard(PlayerId::P1);
        }
        combat_damage_step(&mut game, false);

        // Cavaleiro tem só Iniciativa: não causa dano de novo, e não levou nenhum.
        assert_eq!(game.state.object(knight).map(|o| o.damage), Some(0));
    }

    #[test]
    fn ameacar_exige_dois_bloqueadores() {
        let defs = vec![
            creature_def(0, "Ogro Ameaçador", 3, 3, &[Keyword::Menace]),
            creature_def(1, "Soldado", 1, 1, &[]),
        ];
        let mut game = make_game(defs);
        let menacer = put_creature(&mut game, 0, PlayerId::P0);
        let s1 = put_creature(&mut game, 1, PlayerId::P1);
        let s2 = put_creature(&mut game, 1, PlayerId::P1);

        attack(&mut game, menacer, PlayerId::P1);
        let options = block_options(&game, PlayerId::P1, &[s1, s2], &[menacer]);

        let single = options.iter().any(|a| match a {
            Action::Block { assignments } => assignments.len() == 1,
            _ => false,
        });
        assert!(!single, "CR 702.111a proíbe bloqueio único contra Ameaçar");

        let double = options.iter().any(|a| match a {
            Action::Block { assignments } => {
                assignments.len() == 2 && assignments.iter().all(|(_, at)| *at == menacer)
            }
            _ => false,
        });
        assert!(double, "bloqueio duplo precisa estar disponível");
        assert!(options.iter().any(|a| matches!(a, Action::Block { assignments } if assignments.is_empty())));

        // E o bloqueio único é descartado também na aplicação.
        declare_blockers(&mut game, &[(s1, menacer)]);
        assert!(!game.state.object(menacer).is_some_and(|o| o.combat.was_blocked));
    }
}
