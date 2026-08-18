//! Combate: declaração de atacantes, declaração de bloqueadores e dano de
//! combate (CR 506–511).
//!
//! Três invariantes guiam o módulo:
//!   1. Legalidade é decidida por `layers::characteristics`, nunca por `CardDef`
//!      — uma criatura que ganhou Voar por efeito precisa voar de verdade.
//!   2. O dano de combate é **simultâneo** (CR 510.2): primeiro monta-se a lista
//!      inteira de atribuições, só depois ela é aplicada. Aplicar durante a
//!      coleta faria uma criatura morta parar de causar dano, o que é errado.
//!   3. A enumeração de opções para o bot é limitada e determinística. 2^n de
//!      ataque explode a árvore de decisão sem ganho tático real.
use std::collections::BTreeMap;

use super::query::{self, EvalCtx};
use super::{layers, triggers, Characteristics, Game};
use crate::action::{Action, Request};
use crate::event::{DamageKind, Defender, GameEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::ir::{Filter, Keyword, StaticModRuntime};
use crate::mana::{Color, ColorSet};
use crate::types::{CardType, CounterKind};
use crate::view::MatchEvent;

/// Teto de combinações devolvidas por `attack_options`/`block_options`.
const MAX_COMBAT_OPTIONS: usize = 60;
/// Acima disso, subconjuntos são gerados por ranking de poder em vez de 2^n.
const SUBSET_ENUM_LIMIT: usize = 5;
/// Teto de permutações enumeradas para a ordem de bloqueadores.
const MAX_ORDER_OPTIONS: usize = 24;
/// Dano letal mínimo atribuível a um bloqueador (CR 510.1c).
const MIN_LETHAL: i32 = 1;

// ---------------------------------------------------------------------------
// Leitura auxiliar
// ---------------------------------------------------------------------------

fn chars(game: &Game, id: ObjectId) -> Option<Characteristics> {
    layers::characteristics(game, id)
}

fn is_on_battlefield(game: &Game, id: ObjectId) -> bool {
    game.state.object(id).is_some_and(|o| o.on_battlefield())
}

fn has_kw(ch: &Characteristics, k: &Keyword) -> bool {
    ch.has_keyword(k)
}

/// Nome do jogador para o log. Só leitura: nenhuma regra depende disto.
fn player_label(game: &Game, player: PlayerId) -> String {
    game.state
        .players
        .get(player.index())
        .map(|p| p.name.clone())
        .unwrap_or_else(|| player.to_string())
}

/// Contra quem o ataque foi declarado, em texto.
fn defender_label(game: &Game, defender: Defender) -> String {
    match defender {
        Defender::Player(p) => player_label(game, p),
        Defender::Planeswalker(id) | Defender::Battle(id) => game.card_name(id),
    }
}

fn power_of(game: &Game, id: ObjectId) -> i32 {
    chars(game, id).map(|c| c.power).unwrap_or(0)
}

/// Quem o permanente está atacando, se estiver atacando.
fn attacking_defender(game: &Game, attacker: ObjectId) -> Option<Defender> {
    game.state
        .object(attacker)
        .filter(|o| o.combat.is_attacking())
        .and_then(|o| o.combat.attacking)
}

/// Jogador que está defendendo contra um atacante (CR 506.2).
fn defending_player(game: &Game, defender: Defender) -> Option<PlayerId> {
    match defender {
        Defender::Player(p) => Some(p),
        Defender::Planeswalker(id) | Defender::Battle(id) => {
            game.state.object(id).map(|o| o.controller)
        }
    }
}

// ---------------------------------------------------------------------------
// Elegibilidade
// ---------------------------------------------------------------------------

/// CR 508.1a — criaturas que o jogador ativo pode declarar como atacantes.
pub fn eligible_attackers(game: &Game, player: PlayerId) -> Vec<ObjectId> {
    let candidates: Vec<ObjectId> = game
        .state
        .permanents_controlled_by(player)
        // CR 508.1a — criatura virada não pode ser declarada atacante; uma que
        // já está atacando não é redeclarada numa fase de combate adicional.
        .filter(|o| !o.tapped && !o.combat.is_attacking())
        .map(|o| o.id)
        .collect();

    let mut out: Vec<ObjectId> = candidates
        .into_iter()
        .filter(|id| {
            let Some(ch) = chars(game, *id) else {
                return false;
            };
            if !ch.is_creature() || ch.cant_attack {
                return false;
            }
            // CR 702.3b — Defender não pode atacar.
            if has_kw(&ch, &Keyword::Defender) {
                return false;
            }
            // CR 302.6 — enjoo de invocação, salvo Ímpeto (CR 702.10b).
            let sick = game.state.object(*id).is_some_and(|o| o.summoning_sick);
            !sick || has_kw(&ch, &Keyword::Haste)
        })
        .collect();
    out.sort_unstable();
    out
}

/// CR 509.1a — criaturas desviradas que o defensor pode declarar bloqueadoras.
pub fn eligible_blockers(game: &Game, player: PlayerId) -> Vec<ObjectId> {
    let candidates: Vec<ObjectId> = game
        .state
        .permanents_controlled_by(player)
        .filter(|o| !o.tapped && !o.combat.is_attacking() && !o.combat.is_blocking())
        .map(|o| o.id)
        .collect();

    let mut out: Vec<ObjectId> = candidates
        .into_iter()
        .filter(|id| chars(game, *id).is_some_and(|ch| ch.is_creature() && !ch.cant_block))
        .collect();
    out.sort_unstable();
    out
}

/// Filtros "só pode ser bloqueada por..." vindos de efeitos contínuos.
///
/// `Characteristics` não carrega esse dado (é um predicado, não um booleano),
/// então ele é lido de `state.continuous` — onde `layers` deposita tanto efeitos
/// de resolução quanto habilidades estáticas de permanentes no campo.
fn blocked_except_by(game: &Game, attacker: ObjectId) -> Vec<Filter> {
    game.state
        .continuous
        .iter()
        .filter(|e| e.affected.contains(&attacker))
        .filter_map(|e| match &e.modification {
            StaticModRuntime::CantBeBlockedExceptBy(f) => Some(f.clone()),
            _ => None,
        })
        .collect()
}

/// CR 509.1b — o bloqueio proposto é legal? Cobre a evasão toda menos Ameaçar,
/// que é restrição sobre o conjunto de bloqueadores e vive em `menace_ok`.
pub fn can_block(game: &Game, blocker: ObjectId, attacker: ObjectId) -> bool {
    if blocker == attacker || !is_on_battlefield(game, blocker) || !is_on_battlefield(game, attacker)
    {
        return false;
    }
    let Some(bs) = game.state.object(blocker) else {
        return false;
    };
    if bs.tapped || bs.combat.is_attacking() {
        return false;
    }
    let (Some(bch), Some(ach)) = (chars(game, blocker), chars(game, attacker)) else {
        return false;
    };
    if !bch.is_creature() || !ach.is_creature() || bch.cant_block {
        return false;
    }
    if attacking_defender(game, attacker).is_none() {
        return false;
    }
    // CR 509.1b — "não pode ser bloqueada" encerra o assunto.
    if ach.cant_be_blocked {
        return false;
    }

    // CR 702.9b — Voar só é bloqueado por Voar ou Alcance.
    if has_kw(&ach, &Keyword::Flying)
        && !has_kw(&bch, &Keyword::Flying)
        && !has_kw(&bch, &Keyword::Reach)
    {
        return false;
    }
    // CR 702.36b — Medo: só artefato ou criatura preta bloqueia.
    if has_kw(&ach, &Keyword::Fear) && !is_artifact(&bch) && !bch.colors.contains(Color::Black) {
        return false;
    }
    // CR 702.13b — Intimidar: só artefato ou criatura que compartilhe cor.
    if has_kw(&ach, &Keyword::Intimidate) && !is_artifact(&bch) && !bch.colors.intersects(ach.colors)
    {
        return false;
    }
    // CR 702.118a — Esgueirar: bloqueador de poder maior não pode bloquear.
    if has_kw(&ach, &Keyword::Skulk) && bch.power > ach.power {
        return false;
    }
    // CR 702.14b — Landwalk: o defensor controlando a terra nomeada não bloqueia.
    for kw in &ach.keywords {
        if let Keyword::Landwalk(subtype) = kw {
            if controls_land_subtype(game, bch.controller, subtype) {
                return false;
            }
        }
    }
    // CR 702.16e — Proteção da cor do bloqueador impede o bloqueio.
    for kw in &ach.keywords {
        if let Keyword::Protection(color) = kw {
            if bch.colors.contains(*color) {
                return false;
            }
        }
    }
    // Efeito de "só pode ser bloqueada por criaturas com ...".
    let ctx = EvalCtx::for_source(attacker, ach.controller);
    for filter in blocked_except_by(game, attacker) {
        if !query::matches_filter(game, blocker, &filter, &ctx) {
            return false;
        }
    }
    true
}

fn is_artifact(ch: &Characteristics) -> bool {
    ch.type_line.has_type(CardType::Artifact)
}

fn controls_land_subtype(game: &Game, player: PlayerId, subtype: &str) -> bool {
    let ids: Vec<ObjectId> = game
        .state
        .permanents_controlled_by(player)
        .map(|o| o.id)
        .collect();
    ids.into_iter().any(|id| {
        chars(game, id).is_some_and(|c| c.type_line.is_land() && c.type_line.has_subtype(subtype))
    })
}

/// CR 702.110b — Ameaçar exige dois ou mais bloqueadores.
fn requires_two_blockers(game: &Game, attacker: ObjectId) -> bool {
    chars(game, attacker).is_some_and(|c| has_kw(&c, &Keyword::Menace))
}

fn menace_ok(game: &Game, attacker: ObjectId, count: usize) -> bool {
    count == 0 || !requires_two_blockers(game, attacker) || count >= 2
}

// ---------------------------------------------------------------------------
// Enumeração de ataques
// ---------------------------------------------------------------------------

/// Alvos legais de ataque: cada oponente vivo e cada planeswalker dele (CR 506.2).
fn attack_targets(game: &Game, attacker_controller: PlayerId) -> Vec<Defender> {
    let mut out: Vec<Defender> = game
        .state
        .players
        .iter()
        .filter(|p| p.id != attacker_controller && !p.has_lost)
        .map(|p| Defender::Player(p.id))
        .collect();

    let mut walkers: Vec<ObjectId> = game
        .state
        .permanents()
        .filter(|o| o.controller != attacker_controller)
        .map(|o| o.id)
        .collect();
    walkers
        .retain(|id| chars(game, *id).is_some_and(|c| c.type_line.has_type(CardType::Planeswalker)));
    walkers.sort_unstable();
    out.extend(walkers.into_iter().map(Defender::Planeswalker));
    out
}

/// Subconjuntos candidatos, determinísticos e em número controlado.
///
/// Até `SUBSET_ENUM_LIMIT` candidatos a enumeração é exaustiva (2^5 = 32, o que
/// cabe no teto). Acima disso, gerar 2^n é inviável: usa-se ranking por poder e
/// devolvem-se os prefixos "os k mais fortes", que é onde a decisão real mora.
fn attack_subsets(game: &Game, eligible: &[ObjectId]) -> Vec<Vec<ObjectId>> {
    let mut sorted: Vec<ObjectId> = eligible.to_vec();
    sorted.sort_unstable();
    let mut out: Vec<Vec<ObjectId>> = Vec::new();
    if sorted.is_empty() {
        return out;
    }
    // O conjunto completo é sempre a primeira alternativa oferecida.
    out.push(sorted.clone());

    if sorted.len() <= SUBSET_ENUM_LIMIT {
        let n = sorted.len();
        for mask in 1u32..(1u32 << n) {
            let subset: Vec<ObjectId> = (0..n)
                .filter(|i| mask & (1 << i) != 0)
                .map(|i| sorted[i])
                .collect();
            if subset.len() != n {
                out.push(subset);
            }
        }
    } else {
        let mut ranked = sorted.clone();
        // Poder decrescente, id como desempate — determinismo por semente exige
        // ordem total, não apenas "por poder".
        ranked.sort_by(|a, b| {
            power_of(game, *b)
                .cmp(&power_of(game, *a))
                .then_with(|| a.cmp(b))
        });
        for k in 1..ranked.len() {
            let mut subset: Vec<ObjectId> = ranked[..k].to_vec();
            subset.sort_unstable();
            out.push(subset);
        }
    }
    out
}

pub fn attack_options(game: &Game, player: PlayerId, eligible: &[ObjectId]) -> Vec<Action> {
    // CR 508.1 — não atacar é sempre legal.
    let mut out: Vec<Action> = vec![Action::Attack {
        assignments: Vec::new(),
    }];
    let usable: Vec<ObjectId> = eligible
        .iter()
        .copied()
        .filter(|id| chars(game, *id).is_some_and(|c| c.is_creature() && !c.cant_attack))
        .collect();
    if usable.is_empty() {
        return out;
    }
    let defenders = attack_targets(game, player);
    if defenders.is_empty() {
        return out;
    }

    let subsets = attack_subsets(game, &usable);
    // Primeiro todo mundo contra cada defensor, depois os subconjuntos contra
    // cada defensor: assim o corte por teto nunca remove o caso "todos".
    for defender in &defenders {
        if let Some(all) = subsets.first() {
            push_attack(&mut out, all, *defender);
        }
    }
    'outer: for subset in subsets.iter().skip(1) {
        for defender in &defenders {
            push_attack(&mut out, subset, *defender);
            if out.len() >= MAX_COMBAT_OPTIONS {
                break 'outer;
            }
        }
    }
    out.truncate(MAX_COMBAT_OPTIONS);
    out
}

fn push_attack(out: &mut Vec<Action>, subset: &[ObjectId], defender: Defender) {
    let action = Action::Attack {
        assignments: subset.iter().map(|id| (*id, defender)).collect(),
    };
    if !out.contains(&action) {
        out.push(action);
    }
}

// ---------------------------------------------------------------------------
// Enumeração de bloqueios
// ---------------------------------------------------------------------------

/// Atacantes que este jogador pode bloquear: os que atacam ele ou um permanente
/// dele (CR 509.1a).
fn blockable_attackers(game: &Game, player: PlayerId, attackers: &[ObjectId]) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = attackers
        .iter()
        .copied()
        .filter(|a| {
            attacking_defender(game, *a)
                .and_then(|d| defending_player(game, d))
                .is_some_and(|p| p == player)
        })
        .collect();
    out.sort_unstable();
    out
}

pub fn block_options(
    game: &Game,
    player: PlayerId,
    eligible: &[ObjectId],
    attackers: &[ObjectId],
) -> Vec<Action> {
    // CR 509.1 — não bloquear é sempre legal.
    let mut out: Vec<Action> = vec![Action::Block {
        assignments: Vec::new(),
    }];
    let targets = blockable_attackers(game, player, attackers);
    if targets.is_empty() || eligible.is_empty() {
        return out;
    }

    let mut blockers: Vec<ObjectId> = eligible.to_vec();
    blockers.sort_unstable();

    // Pares legais, por bloqueador. Ordem estável em ambos os eixos.
    let pairs: Vec<(ObjectId, Vec<ObjectId>)> = blockers
        .iter()
        .map(|b| {
            let legal: Vec<ObjectId> = targets
                .iter()
                .copied()
                .filter(|a| can_block(game, *b, *a))
                .collect();
            (*b, legal)
        })
        .filter(|(_, legal)| !legal.is_empty())
        .collect();
    if pairs.is_empty() {
        return out;
    }

    // Caso "todos": cada bloqueador toma o atacante mais forte que consegue.
    let mut greedy: Vec<(ObjectId, ObjectId)> = Vec::new();
    for (b, legal) in &pairs {
        let best = legal.iter().copied().max_by(|x, y| {
            power_of(game, *x)
                .cmp(&power_of(game, *y))
                .then_with(|| y.cmp(x))
        });
        if let Some(a) = best {
            greedy.push((*b, a));
        }
    }
    push_block(game, &mut out, greedy);

    // Bloqueios simples: um bloqueador contra um atacante sem Ameaçar.
    for (b, legal) in &pairs {
        for a in legal {
            if requires_two_blockers(game, *a) {
                continue;
            }
            push_block(game, &mut out, vec![(*b, *a)]);
            if out.len() >= MAX_COMBAT_OPTIONS {
                out.truncate(MAX_COMBAT_OPTIONS);
                return out;
            }
        }
    }

    // Bloqueios duplos: cobrem Ameaçar e também "duas criaturas seguram uma".
    for i in 0..pairs.len() {
        for j in (i + 1)..pairs.len() {
            let (b1, legal1) = &pairs[i];
            let (b2, legal2) = &pairs[j];
            for a in legal1 {
                if !legal2.contains(a) {
                    continue;
                }
                push_block(game, &mut out, vec![(*b1, *a), (*b2, *a)]);
                if out.len() >= MAX_COMBAT_OPTIONS {
                    out.truncate(MAX_COMBAT_OPTIONS);
                    return out;
                }
            }
        }
    }

    out.truncate(MAX_COMBAT_OPTIONS);
    out
}

/// Adiciona um bloqueio se ele for legal como conjunto (inclui Ameaçar).
fn push_block(game: &Game, out: &mut Vec<Action>, assignments: Vec<(ObjectId, ObjectId)>) {
    if assignments.is_empty() {
        return;
    }
    let legal = legalize_block(game, &assignments);
    if legal.is_empty() {
        return;
    }
    let action = Action::Block { assignments: legal };
    if !out.contains(&action) {
        out.push(action);
    }
}

/// Descarta os bloqueios ilegais de um conjunto proposto e devolve o resto.
/// Um atacante com Ameaçar bloqueado por um só bloqueador perde o bloqueio
/// inteiro — meio bloqueio não existe (CR 509.1b).
fn legalize_block(game: &Game, assignments: &[(ObjectId, ObjectId)]) -> Vec<(ObjectId, ObjectId)> {
    let mut seen_blockers: Vec<ObjectId> = Vec::new();
    let mut kept: Vec<(ObjectId, ObjectId)> = Vec::new();
    for (blocker, attacker) in assignments {
        if seen_blockers.contains(blocker) {
            continue;
        }
        if !can_block(game, *blocker, *attacker) {
            continue;
        }
        seen_blockers.push(*blocker);
        kept.push((*blocker, *attacker));
    }
    let mut counts: BTreeMap<ObjectId, usize> = BTreeMap::new();
    for (_, attacker) in &kept {
        *counts.entry(*attacker).or_insert(0) += 1;
    }
    kept.retain(|(_, attacker)| {
        menace_ok(game, *attacker, counts.get(attacker).copied().unwrap_or(0))
    });
    kept
}

// ---------------------------------------------------------------------------
// Ordem de bloqueadores e distribuição de dano
// ---------------------------------------------------------------------------

pub fn order_options(attacker: ObjectId, blockers: &[ObjectId]) -> Vec<Action> {
    if blockers.len() < 2 {
        return vec![Action::OrderBlockers {
            attacker,
            order: blockers.to_vec(),
        }];
    }
    let mut out: Vec<Action> = Vec::new();
    if blockers.len() <= 4 {
        // Permutações completas, geradas em ordem determinística de índice.
        let mut indices: Vec<usize> = (0..blockers.len()).collect();
        let mut perms: Vec<Vec<usize>> = Vec::new();
        permute(&mut indices, 0, &mut perms);
        for idx in perms {
            let order: Vec<ObjectId> = idx.iter().map(|i| blockers[*i]).collect();
            let action = Action::OrderBlockers { attacker, order };
            if !out.contains(&action) {
                out.push(action);
            }
        }
    } else {
        // Rotações: n opções em vez de n!, mantendo toda criatura como primeira.
        for shift in 0..blockers.len() {
            let mut order: Vec<ObjectId> = Vec::with_capacity(blockers.len());
            for i in 0..blockers.len() {
                order.push(blockers[(i + shift) % blockers.len()]);
            }
            out.push(Action::OrderBlockers { attacker, order });
        }
    }
    out.truncate(MAX_ORDER_OPTIONS);
    out
}

fn permute(items: &mut Vec<usize>, k: usize, out: &mut Vec<Vec<usize>>) {
    if k + 1 >= items.len() {
        out.push(items.clone());
        return;
    }
    for i in k..items.len() {
        items.swap(k, i);
        permute(items, k + 1, out);
        items.swap(k, i);
    }
}

/// Quanto falta para matar `blocker`, contando dano já marcado e dano já
/// atribuído neste mesmo passo (CR 510.1c).
fn lethal_needed(game: &Game, blocker: ObjectId, pending: &BTreeMap<ObjectId, i32>) -> i32 {
    let toughness = chars(game, blocker).map(|c| c.toughness).unwrap_or(0);
    let marked = game.state.object(blocker).map(|o| o.damage).unwrap_or(0);
    let already = pending.get(&blocker).copied().unwrap_or(0);
    (toughness - marked - already).max(MIN_LETHAL)
}

pub fn damage_assignment_options(
    game: &Game,
    attacker: ObjectId,
    blockers: &[ObjectId],
    total: i32,
) -> Vec<Action> {
    let mut out: Vec<Action> = Vec::new();
    if total <= 0 || blockers.is_empty() {
        out.push(Action::AssignDamage {
            attacker,
            assignments: Vec::new(),
            trample_to_defender: total.max(0),
        });
        return out;
    }
    let ch = chars(game, attacker);
    let deathtouch = ch.as_ref().is_some_and(|c| has_kw(c, &Keyword::Deathtouch));
    let trample = ch.as_ref().is_some_and(|c| has_kw(c, &Keyword::Trample));

    // Opção canônica: letal em ordem, excedente ao defensor se houver Atropelar.
    let mut pending: BTreeMap<ObjectId, i32> = BTreeMap::new();
    let mut assignments: Vec<(ObjectId, i32)> = Vec::new();
    let mut remaining = total;
    for b in blockers {
        if remaining <= 0 {
            break;
        }
        // CR 702.2c — com Toque Mortal, 1 já é letal.
        let need = if deathtouch {
            MIN_LETHAL
        } else {
            lethal_needed(game, *b, &pending)
        };
        let give = remaining.min(need);
        assignments.push((*b, give));
        *pending.entry(*b).or_insert(0) += give;
        remaining -= give;
    }
    if remaining > 0 && !trample {
        if let Some(last) = assignments.last_mut() {
            last.1 += remaining;
            remaining = 0;
        }
    }
    push_assign(
        &mut out,
        Action::AssignDamage {
            attacker,
            assignments,
            trample_to_defender: if trample { remaining } else { 0 },
        },
    );

    // Alternativa: concentrar tudo no primeiro bloqueador.
    if let Some(first) = blockers.first() {
        push_assign(
            &mut out,
            Action::AssignDamage {
                attacker,
                assignments: vec![(*first, total)],
                trample_to_defender: 0,
            },
        );
    }

    // Alternativa: o mínimo letal em cada bloqueador e o resto no defensor.
    if trample {
        let mut pending2: BTreeMap<ObjectId, i32> = BTreeMap::new();
        let mut minimal: Vec<(ObjectId, i32)> = Vec::new();
        let mut left = total;
        for b in blockers {
            if left <= 0 {
                break;
            }
            let need = if deathtouch {
                MIN_LETHAL
            } else {
                lethal_needed(game, *b, &pending2)
            };
            let give = left.min(need);
            minimal.push((*b, give));
            *pending2.entry(*b).or_insert(0) += give;
            left -= give;
        }
        push_assign(
            &mut out,
            Action::AssignDamage {
                attacker,
                assignments: minimal,
                trample_to_defender: left.max(0),
            },
        );
    }
    out.truncate(MAX_COMBAT_OPTIONS);
    out
}

fn push_assign(out: &mut Vec<Action>, action: Action) {
    if !out.contains(&action) {
        out.push(action);
    }
}

// ---------------------------------------------------------------------------
// Declaração de atacantes (CR 508)
// ---------------------------------------------------------------------------

pub fn declare_attackers(game: &mut Game, assignments: &[(ObjectId, Defender)]) {
    let active = game.state.active_player;
    let eligible = eligible_attackers(game, active);
    let targets = attack_targets(game, active);

    let mut declared: Vec<(ObjectId, Defender)> = Vec::new();
    for (creature, defender) in assignments {
        if !eligible.contains(creature) || !targets.contains(defender) {
            game.state
                .push_log(format!("ataque ilegal ignorado: {creature}"), Some(active));
            continue;
        }
        if declared.iter().any(|(id, _)| id == creature) {
            continue;
        }
        declared.push((*creature, *defender));
    }
    if declared.is_empty() {
        return;
    }

    for (creature, defender) in &declared {
        let vigilance = chars(game, *creature).is_some_and(|c| has_kw(&c, &Keyword::Vigilance));
        if let Some(obj) = game.state.object_mut(*creature) {
            obj.combat.attacking = Some(*defender);
            obj.combat.removed_from_combat = false;
        }
        // CR 508.1f — atacar vira a criatura, salvo Vigilância (CR 702.20b).
        if !vigilance {
            tap_for_attack(game, *creature);
        }
        game.state.emit(GameEvent::Attacked {
            object: *creature,
            defender: *defender,
        });
        let name = game.card_name(*creature);
        let against = defender_label(game, *defender);
        game.state
            .push_log(format!("{name} ataca {against}"), Some(active));
    }

    let ids: Vec<ObjectId> = declared.iter().map(|(id, _)| *id).collect();
    game.state
        .emit(GameEvent::AttackersDeclared { attackers: ids });
    game.push_event(MatchEvent::AttackersDeclared {
        attackers: declared,
    });
    // CR 508.2 — gatilhos de "quando ataca" disparam depois da declaração toda.
    triggers::collect(game);
}

fn tap_for_attack(game: &mut Game, id: ObjectId) {
    let already = game.state.object(id).map(|o| o.tapped).unwrap_or(true);
    if already {
        return;
    }
    if let Some(obj) = game.state.object_mut(id) {
        obj.tapped = true;
    }
    game.state.emit(GameEvent::Tapped { object: id });
    game.push_event(MatchEvent::Tapped { card: id });
}

// ---------------------------------------------------------------------------
// Declaração de bloqueadores (CR 509)
// ---------------------------------------------------------------------------

pub fn declare_blockers(game: &mut Game, assignments: &[(ObjectId, ObjectId)]) {
    let kept = legalize_block(game, assignments);
    if kept.is_empty() {
        return;
    }

    let mut by_attacker: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
    for (blocker, attacker) in &kept {
        if let Some(obj) = game.state.object_mut(*blocker) {
            if !obj.combat.blocking.contains(attacker) {
                obj.combat.blocking.push(*attacker);
            }
        }
        by_attacker.entry(*attacker).or_default().push(*blocker);
    }

    for (attacker, blockers) in &by_attacker {
        if let Some(obj) = game.state.object_mut(*attacker) {
            for b in blockers {
                if !obj.combat.blocked_by.contains(b) {
                    obj.combat.blocked_by.push(*b);
                }
            }
            // CR 509.1h — uma vez bloqueada, a criatura segue bloqueada mesmo
            // que todos os bloqueadores saiam do combate; e nesse caso ela não
            // causa dano ao jogador.
            obj.combat.was_blocked = true;
        }
    }

    // CR 509.2 — o atacante escolhe a ordem de dano entre dois ou mais bloqueadores.
    let ordered: Vec<ObjectId> = by_attacker.keys().copied().collect();
    for attacker in ordered {
        let (current, controller) = match game.state.object(attacker) {
            Some(o) if o.combat.blocked_by.len() > 1 => (o.combat.blocked_by.clone(), o.controller),
            _ => continue,
        };
        let action = game.ask(Request::OrderBlockers {
            player: controller,
            attacker,
            blockers: current.clone(),
        });
        let Action::OrderBlockers {
            attacker: which,
            order,
        } = action
        else {
            continue;
        };
        let same_set = which == attacker
            && order.len() == current.len()
            && order.iter().all(|b| current.contains(b));
        if !same_set {
            continue;
        }
        if let Some(obj) = game.state.object_mut(attacker) {
            obj.combat.blocked_by = order;
        }
    }

    let mut blocks: Vec<(ObjectId, Vec<ObjectId>)> = Vec::new();
    for attacker in by_attacker.keys() {
        let list = game
            .state
            .object(*attacker)
            .map(|o| o.combat.blocked_by.clone())
            .unwrap_or_default();
        if list.is_empty() {
            continue;
        }
        let attacker_name = game.card_name(*attacker);
        let blocker_names: Vec<String> = list.iter().map(|b| game.card_name(*b)).collect();
        let blocker_controller = list
            .first()
            .and_then(|b| game.state.object(*b))
            .map(|o| o.controller);
        game.state.push_log(
            format!("{} bloqueia {attacker_name}", blocker_names.join(" e ")),
            blocker_controller,
        );
        game.state.emit(GameEvent::Blocked {
            attacker: *attacker,
            blockers: list.clone(),
        });
        blocks.push((*attacker, list));
    }
    game.state.emit(GameEvent::BlockersDeclared {
        blocks: blocks.clone(),
    });
    game.push_event(MatchEvent::BlockersDeclared { blocks });
    // CR 509.4 — gatilhos de "quando bloqueia / torna-se bloqueada".
    triggers::collect(game);
}

// ---------------------------------------------------------------------------
// Dano de combate (CR 510)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignTarget {
    Object(ObjectId),
    Player(PlayerId),
}

#[derive(Debug, Clone, Copy)]
struct Assign {
    source: ObjectId,
    controller: PlayerId,
    target: AssignTarget,
    amount: i32,
    deathtouch: bool,
    lifelink: bool,
    colors: ColorSet,
}

/// Esta criatura causa dano neste passo? (CR 510.5)
fn deals_damage_now(game: &Game, ch: &Characteristics, first_strike: bool) -> bool {
    let fs = has_kw(ch, &Keyword::FirstStrike);
    let ds = has_kw(ch, &Keyword::DoubleStrike);
    if first_strike {
        return fs || ds;
    }
    // Sem passo de primeiro golpe nesta fase, todo mundo causa no passo normal.
    if !game.state.first_strike_done {
        return true;
    }
    // Golpe Duplo causa nos dois passos; primeiro golpe puro já causou.
    ds || !fs
}

pub fn combat_damage_step(game: &mut Game, first_strike: bool) {
    let mut attackers: Vec<ObjectId> = game
        .state
        .permanents()
        .filter(|o| o.combat.is_attacking())
        .map(|o| o.id)
        .collect();
    attackers.sort_unstable();
    let mut blockers: Vec<ObjectId> = game
        .state
        .permanents()
        .filter(|o| o.combat.is_blocking())
        .map(|o| o.id)
        .collect();
    blockers.sort_unstable();
    if attackers.is_empty() && blockers.is_empty() {
        return;
    }

    let mut pending: BTreeMap<ObjectId, i32> = BTreeMap::new();
    let mut assigns: Vec<Assign> = Vec::new();

    for attacker in &attackers {
        collect_attacker_damage(game, *attacker, first_strike, &mut pending, &mut assigns);
    }
    for blocker in &blockers {
        collect_blocker_damage(game, *blocker, first_strike, &mut pending, &mut assigns);
    }

    // CR 510.2 — só agora o dano acontece, e acontece tudo ao mesmo tempo.
    if !assigns.is_empty() {
        let label = if first_strike {
            "dano de primeiro golpe"
        } else {
            "dano de combate"
        };
        game.state.push_log(label, None);
    }
    apply_damage(game, &assigns);
    game.state.emit(GameEvent::CombatDamageDealt);
    triggers::collect(game);
}

fn collect_attacker_damage(
    game: &Game,
    attacker: ObjectId,
    first_strike: bool,
    pending: &mut BTreeMap<ObjectId, i32>,
    out: &mut Vec<Assign>,
) {
    let Some(ch) = chars(game, attacker) else {
        return;
    };
    // CR 510.1a — dano de 0 ou menos não é atribuído.
    if !ch.is_creature() || !deals_damage_now(game, &ch, first_strike) || ch.power <= 0 {
        return;
    }
    let Some(defender) = attacking_defender(game, attacker) else {
        return;
    };
    let deathtouch = has_kw(&ch, &Keyword::Deathtouch);
    let trample = has_kw(&ch, &Keyword::Trample);
    let lifelink = has_kw(&ch, &Keyword::Lifelink);

    let was_blocked = game
        .state
        .object(attacker)
        .is_some_and(|o| o.combat.was_blocked);
    let live_blockers: Vec<ObjectId> = game
        .state
        .object(attacker)
        .map(|o| o.combat.blocked_by.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|b| {
            is_on_battlefield(game, *b)
                && game
                    .state
                    .object(*b)
                    .is_some_and(|o| o.combat.blocking.contains(&attacker))
        })
        .collect();

    let mut remaining = ch.power;

    if !live_blockers.is_empty() {
        // CR 510.1c — letal em cada bloqueador antes de passar ao próximo.
        for b in &live_blockers {
            if remaining <= 0 {
                break;
            }
            // CR 702.2c — Toque Mortal torna 1 de dano letal, o que basta para
            // liberar o excedente de Atropelar.
            let need = if deathtouch {
                MIN_LETHAL
            } else {
                lethal_needed(game, *b, pending)
            };
            let give = remaining.min(need);
            *pending.entry(*b).or_insert(0) += give;
            out.push(Assign {
                source: attacker,
                controller: ch.controller,
                target: AssignTarget::Object(*b),
                amount: give,
                deathtouch,
                lifelink,
                colors: ch.colors,
            });
            remaining -= give;
        }
        if remaining > 0 && !trample {
            // Sem Atropelar o excedente não vaza: sobra no último bloqueador.
            if let Some(last) = live_blockers.last() {
                *pending.entry(*last).or_insert(0) += remaining;
                out.push(Assign {
                    source: attacker,
                    controller: ch.controller,
                    target: AssignTarget::Object(*last),
                    amount: remaining,
                    deathtouch,
                    lifelink,
                    colors: ch.colors,
                });
            }
            remaining = 0;
        }
    } else if was_blocked && !trample {
        // CR 509.1h — bloqueada e sem bloqueadores restantes: nenhum dano.
        // Com Atropelar (CR 702.19b) o total vai para o defensor.
        return;
    }

    if remaining > 0 {
        push_defender_damage(
            game,
            defender,
            attacker,
            ch.controller,
            remaining,
            deathtouch,
            lifelink,
            ch.colors,
            out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push_defender_damage(
    game: &Game,
    defender: Defender,
    source: ObjectId,
    controller: PlayerId,
    amount: i32,
    deathtouch: bool,
    lifelink: bool,
    colors: ColorSet,
    out: &mut Vec<Assign>,
) {
    let target = match defender {
        Defender::Player(p) => AssignTarget::Player(p),
        Defender::Planeswalker(id) | Defender::Battle(id) => {
            if !is_on_battlefield(game, id) {
                return;
            }
            AssignTarget::Object(id)
        }
    };
    out.push(Assign {
        source,
        controller,
        target,
        amount,
        deathtouch,
        lifelink,
        colors,
    });
}

fn collect_blocker_damage(
    game: &Game,
    blocker: ObjectId,
    first_strike: bool,
    pending: &mut BTreeMap<ObjectId, i32>,
    out: &mut Vec<Assign>,
) {
    let Some(ch) = chars(game, blocker) else {
        return;
    };
    if !ch.is_creature() || !deals_damage_now(game, &ch, first_strike) || ch.power <= 0 {
        return;
    }
    let blocked: Vec<ObjectId> = game
        .state
        .object(blocker)
        .map(|o| o.combat.blocking.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|a| {
            is_on_battlefield(game, *a)
                && game.state.object(*a).is_some_and(|o| o.combat.is_attacking())
        })
        .collect();
    let Some(target) = blocked.first().copied() else {
        return;
    };
    // Bloqueando mais de um atacante o dano seria dividido pelo controlador;
    // a escolha determinística é concentrar tudo no primeiro da ordem.
    *pending.entry(target).or_insert(0) += ch.power;
    out.push(Assign {
        source: blocker,
        controller: ch.controller,
        target: AssignTarget::Object(target),
        amount: ch.power,
        deathtouch: has_kw(&ch, &Keyword::Deathtouch),
        lifelink: has_kw(&ch, &Keyword::Lifelink),
        colors: ch.colors,
    });
}

/// CR 615 — dano prevenido nunca é causado, então não gera vínculo com a vida.
fn damage_prevented(game: &Game, target: ObjectId, colors: ColorSet) -> bool {
    let Some(ch) = chars(game, target) else {
        return true;
    };
    if ch.prevent_all_damage {
        return true;
    }
    // CR 702.16e — proteção previne dano de fonte da cor nomeada.
    ch.keywords.iter().any(|kw| match kw {
        Keyword::Protection(color) => colors.contains(*color),
        _ => false,
    })
}

fn apply_damage(game: &mut Game, assigns: &[Assign]) {
    let mut lifelink_gain: BTreeMap<PlayerId, i32> = BTreeMap::new();

    for assign in assigns {
        if assign.amount <= 0 {
            continue;
        }
        let dealt = match assign.target {
            AssignTarget::Object(id) => apply_damage_to_object(game, assign, id),
            AssignTarget::Player(p) => {
                apply_damage_to_player(game, assign, p);
                true
            }
        };
        // CR 702.15b — vínculo com a vida só conta dano efetivamente causado.
        if dealt && assign.lifelink {
            *lifelink_gain.entry(assign.controller).or_insert(0) += assign.amount;
        }
    }

    for (player, amount) in lifelink_gain {
        gain_life(game, player, amount);
    }
}

fn apply_damage_to_object(game: &mut Game, assign: &Assign, target: ObjectId) -> bool {
    if !is_on_battlefield(game, target) || damage_prevented(game, target, assign.colors) {
        return false;
    }
    let Some(ch) = chars(game, target) else {
        return false;
    };

    let mut lethal = assign.deathtouch;
    if ch.is_creature() {
        if let Some(obj) = game.state.object_mut(target) {
            obj.damage += assign.amount;
            if assign.deathtouch {
                // CR 702.2b — a marca é o que faz a SBA 704.5h destruir depois.
                obj.deathtouch_damage = true;
            }
            lethal = lethal || obj.damage >= ch.toughness;
        }
    }

    // CR 306.7 — dano a planeswalker remove marcadores de lealdade;
    // CR 310.6 — dano a batalha remove marcadores de defesa.
    let counter = if ch.type_line.has_type(CardType::Planeswalker) {
        Some(CounterKind::Loyalty)
    } else if ch.type_line.has_type(CardType::Battle) {
        Some(CounterKind::Named("Defense".to_string()))
    } else {
        None
    };
    if let Some(kind) = counter {
        let current = game
            .state
            .object(target)
            .map(|o| o.counter(&kind))
            .unwrap_or(0);
        let removed = assign.amount.min(current.max(0));
        if removed > 0 {
            if let Some(obj) = game.state.object_mut(target) {
                obj.add_counter(kind.clone(), -removed);
            }
            game.state.emit(GameEvent::CountersRemoved {
                object: target,
                kind: kind.clone(),
                amount: removed,
            });
            game.push_event(MatchEvent::CountersChanged {
                card: target,
                kind: format!("{kind:?}"),
                delta: -removed,
            });
        }
        lethal = lethal || (current > 0 && removed >= current);
    }

    let source_name = game.card_name(assign.source);
    let target_name = game.card_name(target);
    game.state.push_log(
        format!(
            "{source_name} causa {} de dano a {target_name}{}",
            assign.amount,
            if lethal { " (letal)" } else { "" }
        ),
        Some(assign.controller),
    );
    game.state.emit(GameEvent::DamageDealt {
        source: assign.source,
        target,
        amount: assign.amount,
        kind: DamageKind::Combat,
        deathtouch: assign.deathtouch,
    });
    game.push_event(MatchEvent::DamageDealt {
        source: assign.source,
        target,
        amount: assign.amount,
        lethal,
    });
    true
}

fn apply_damage_to_player(game: &mut Game, assign: &Assign, player: PlayerId) {
    let Some(p) = game.state.players.get_mut(player.index()) else {
        return;
    };
    // CR 119.3 — dano causado a um jogador é perda de vida.
    p.life -= assign.amount;
    p.life_lost_this_turn += assign.amount;
    p.damage_taken_this_turn += assign.amount;
    let total = p.life;

    let source_name = game.card_name(assign.source);
    let player_name = player_label(game, player);
    game.state.push_log(
        format!(
            "{source_name} causa {} de dano a {player_name} (vida: {total})",
            assign.amount
        ),
        Some(assign.controller),
    );
    game.state.emit(GameEvent::DamageDealtToPlayer {
        source: assign.source,
        player,
        amount: assign.amount,
        kind: DamageKind::Combat,
    });
    game.state.emit(GameEvent::LifeLost {
        player,
        amount: assign.amount,
    });
    game.push_event(MatchEvent::DamageToPlayer {
        source: assign.source,
        player,
        amount: assign.amount,
    });
    game.push_event(MatchEvent::LifeChanged {
        player,
        delta: -assign.amount,
        total,
    });
}

fn gain_life(game: &mut Game, player: PlayerId, amount: i32) {
    if amount <= 0 {
        return;
    }
    let Some(p) = game.state.players.get_mut(player.index()) else {
        return;
    };
    p.life += amount;
    p.life_gained_this_turn += amount;
    let total = p.life;
    game.state.emit(GameEvent::LifeGained { player, amount });
    game.push_event(MatchEvent::LifeChanged {
        player,
        delta: amount,
        total,
    });
}

// ---------------------------------------------------------------------------
// Fim de combate (CR 511)
// ---------------------------------------------------------------------------

/// CR 511.3 — no fim do passo de fim de combate todas as criaturas deixam o
/// combate; nenhum permanente continua marcado como atacante ou bloqueador.
pub fn end_combat(game: &mut Game) {
    let had_combat = game
        .state
        .objects
        .iter()
        .any(|o| o.combat.is_attacking() || o.combat.is_blocking());

    for obj in game.state.objects.iter_mut() {
        obj.combat.clear();
    }
    game.state.first_strike_done = false;

    if had_combat {
        game.state.push_log("fim de combate", None);
        game.push_event(MatchEvent::Log {
            text: "fim de combate".to_string(),
        });
    }
}

/// Existe alguém em combate com primeiro golpe? Decide se o passo extra roda.
pub fn has_first_strike_creatures(game: &Game) -> bool {
    let ids: Vec<ObjectId> = game
        .state
        .permanents()
        .filter(|o| o.combat.is_attacking() || o.combat.is_blocking())
        .map(|o| o.id)
        .collect();
    ids.into_iter().any(|id| {
        chars(game, id).is_some_and(|c| {
            has_kw(&c, &Keyword::FirstStrike) || has_kw(&c, &Keyword::DoubleStrike)
        })
    })
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Ability, CardDatabase, CardDef};
    use crate::engine::turn;
    use crate::engine::{Agent, FirstLegalAgent, GameConfig, PlayerConfig};
    use crate::ids::CardDefId;
    use crate::mana::ManaCost;
    use crate::types::{Rarity, TypeLine};
    use crate::zone::ZoneId;
    use std::sync::Arc;

    fn creature(id: u32, name: &str, power: i32, toughness: i32, kws: &[Keyword]) -> CardDef {
        CardDef {
            id: CardDefId(id),
            name: name.to_string(),
            mana_cost: ManaCost::FREE,
            type_line: TypeLine {
                supertypes: Vec::new(),
                types: vec![CardType::Creature],
                subtypes: vec!["Test".to_string()],
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

    fn build(defs: Vec<CardDef>) -> Game {
        let db = CardDatabase { cards: defs };
        let deck: Vec<CardDefId> = (0..db.cards.len() as u32)
            .flat_map(|i| std::iter::repeat(CardDefId(i)).take(4))
            .collect();
        let config = GameConfig {
            starting_life: 20,
            starting_hand_size: 0,
            allow_mulligan: false,
            max_turns: 60,
            max_decisions: 100_000,
        };
        let players = vec![
            PlayerConfig {
                name: "A".to_string(),
                deck: deck.clone(),
            },
            PlayerConfig {
                name: "B".to_string(),
                deck,
            },
        ];
        let agents: Vec<Box<dyn Agent>> = vec![Box::new(FirstLegalAgent), Box::new(FirstLegalAgent)];
        match Game::new(Arc::new(db), players, agents, config, 7) {
            Ok(g) => g,
            Err(e) => panic!("montagem de teste falhou: {e}"),
        }
    }

    /// Coloca uma cópia de `def` no campo sob controle de `player`, já sem enjoo.
    fn deploy(game: &mut Game, player: PlayerId, def: CardDefId) -> ObjectId {
        let card = turn::zone_objects(game, ZoneId::library(player))
            .into_iter()
            .find(|id| game.state.object(*id).is_some_and(|o| o.card == def));
        let Some(card) = card else {
            panic!("nenhuma cópia de {def} restante na biblioteca");
        };
        turn::move_object(game, card, ZoneId::BATTLEFIELD);
        if let Some(obj) = game.state.object_mut(card) {
            obj.summoning_sick = false;
        }
        card
    }

    fn set_attacking(game: &mut Game, attacker: ObjectId, defender: PlayerId) {
        if let Some(obj) = game.state.object_mut(attacker) {
            obj.combat.attacking = Some(Defender::Player(defender));
        }
    }

    fn set_block(game: &mut Game, attacker: ObjectId, blockers: &[ObjectId]) {
        for b in blockers {
            if let Some(obj) = game.state.object_mut(*b) {
                obj.combat.blocking.push(attacker);
            }
        }
        if let Some(obj) = game.state.object_mut(attacker) {
            obj.combat.blocked_by = blockers.to_vec();
            obj.combat.was_blocked = true;
        }
    }

    fn damage_on(game: &Game, id: ObjectId) -> i32 {
        game.state.object(id).map(|o| o.damage).unwrap_or(-1)
    }

    #[test]
    fn trample_com_bloqueador_menor_vaza_para_o_jogador() {
        let mut game = build(vec![
            creature(0, "Atropelador", 5, 5, &[Keyword::Trample]),
            creature(1, "Bichinho", 1, 1, &[]),
        ]);
        let attacker = deploy(&mut game, PlayerId::P0, CardDefId(0));
        let blocker = deploy(&mut game, PlayerId::P1, CardDefId(1));
        set_attacking(&mut game, attacker, PlayerId::P1);
        set_block(&mut game, attacker, &[blocker]);

        combat_damage_step(&mut game, false);

        assert_eq!(damage_on(&game, blocker), 1, "letal exato no bloqueador");
        assert_eq!(
            game.state.player(PlayerId::P1).life,
            16,
            "os 4 de excedente atravessam com Atropelar"
        );
    }

    #[test]
    fn deathtouch_marca_com_um_de_dano() {
        let mut game = build(vec![
            creature(0, "Serpente", 1, 1, &[Keyword::Deathtouch]),
            creature(1, "Gigante", 4, 4, &[]),
        ]);
        let attacker = deploy(&mut game, PlayerId::P0, CardDefId(0));
        let blocker = deploy(&mut game, PlayerId::P1, CardDefId(1));
        set_attacking(&mut game, attacker, PlayerId::P1);
        set_block(&mut game, attacker, &[blocker]);

        combat_damage_step(&mut game, false);

        assert_eq!(damage_on(&game, blocker), 1);
        assert!(
            game.state
                .object(blocker)
                .is_some_and(|o| o.deathtouch_damage),
            "CR 702.2b — o dano precisa ficar marcado como de toque mortal"
        );
    }

    #[test]
    fn primeiro_golpe_causa_dano_antes_do_passo_normal() {
        let mut game = build(vec![
            creature(0, "Lanceiro", 2, 2, &[Keyword::FirstStrike]),
            creature(1, "Recruta", 2, 2, &[]),
        ]);
        let attacker = deploy(&mut game, PlayerId::P0, CardDefId(0));
        let blocker = deploy(&mut game, PlayerId::P1, CardDefId(1));
        set_attacking(&mut game, attacker, PlayerId::P1);
        set_block(&mut game, attacker, &[blocker]);

        assert!(has_first_strike_creatures(&game));
        combat_damage_step(&mut game, true);

        assert_eq!(damage_on(&game, blocker), 2, "o primeiro golpe acerta");
        assert_eq!(
            damage_on(&game, attacker),
            0,
            "o bloqueador sem primeiro golpe ainda não revidou"
        );

        // No passo normal o primeiro golpe puro não bate de novo.
        game.state.first_strike_done = true;
        combat_damage_step(&mut game, false);
        assert_eq!(damage_on(&game, blocker), 2);
        assert_eq!(damage_on(&game, attacker), 2);
    }

    #[test]
    fn menace_exige_dois_bloqueadores() {
        let mut game = build(vec![
            creature(0, "Bruto", 3, 3, &[Keyword::Menace]),
            creature(1, "Guarda", 2, 2, &[]),
        ]);
        let attacker = deploy(&mut game, PlayerId::P0, CardDefId(0));
        let b1 = deploy(&mut game, PlayerId::P1, CardDefId(1));
        set_attacking(&mut game, attacker, PlayerId::P1);

        let one = block_options(&game, PlayerId::P1, &[b1], &[attacker]);
        assert!(
            one.iter()
                .all(|a| matches!(a, Action::Block { assignments } if assignments.is_empty())),
            "CR 702.110b — um bloqueador sozinho não bloqueia Ameaçar"
        );

        let b2 = deploy(&mut game, PlayerId::P1, CardDefId(1));
        let two = block_options(&game, PlayerId::P1, &[b1, b2], &[attacker]);
        assert!(
            two.iter()
                .any(|a| matches!(a, Action::Block { assignments } if assignments.len() == 2)),
            "com dois bloqueadores o bloqueio duplo tem de aparecer"
        );

        // A declaração também precisa rejeitar o bloqueio solitário.
        declare_blockers(&mut game, &[(b1, attacker)]);
        assert!(game
            .state
            .object(attacker)
            .is_some_and(|o| o.combat.blocked_by.is_empty()));
    }

    #[test]
    fn trample_com_deathtouch_atribui_um_por_bloqueador() {
        let mut game = build(vec![
            creature(0, "Hidra", 5, 5, &[Keyword::Trample, Keyword::Deathtouch]),
            creature(1, "Muralha", 3, 3, &[]),
        ]);
        let attacker = deploy(&mut game, PlayerId::P0, CardDefId(0));
        let b1 = deploy(&mut game, PlayerId::P1, CardDefId(1));
        let b2 = deploy(&mut game, PlayerId::P1, CardDefId(1));
        set_attacking(&mut game, attacker, PlayerId::P1);
        set_block(&mut game, attacker, &[b1, b2]);

        combat_damage_step(&mut game, false);

        assert_eq!(damage_on(&game, b1), 1, "CR 702.2c — 1 já é letal");
        assert_eq!(damage_on(&game, b2), 1);
        assert_eq!(
            game.state.player(PlayerId::P1).life,
            17,
            "os 3 restantes atropelam"
        );
    }

    #[test]
    fn atacante_bloqueado_sem_bloqueadores_nao_atinge_o_jogador() {
        let mut game = build(vec![creature(0, "Soldado", 3, 3, &[])]);
        let attacker = deploy(&mut game, PlayerId::P0, CardDefId(0));
        set_attacking(&mut game, attacker, PlayerId::P1);
        let Some(obj) = game.state.object_mut(attacker) else {
            panic!("{attacker} não existe: o cenário exige o atacante em campo");
        };
        // CR 509.1h — foi bloqueada, e depois ficou sem bloqueadores.
        obj.combat.was_blocked = true;

        combat_damage_step(&mut game, false);
        assert_eq!(game.state.player(PlayerId::P1).life, 20);
    }

    #[test]
    fn opcoes_de_ataque_contem_vazio_e_todos_dentro_do_teto() {
        let mut game = build(vec![creature(0, "Recruta", 1, 1, &[])]);
        let mut ids = Vec::new();
        for _ in 0..4 {
            ids.push(deploy(&mut game, PlayerId::P0, CardDefId(0)));
        }
        let options = attack_options(&game, PlayerId::P0, &ids);
        assert!(options.len() <= MAX_COMBAT_OPTIONS);
        assert!(options
            .iter()
            .any(|a| matches!(a, Action::Attack { assignments } if assignments.is_empty())));
        assert!(options
            .iter()
            .any(|a| matches!(a, Action::Attack { assignments } if assignments.len() == ids.len())));
    }

    #[test]
    fn ordem_de_bloqueadores_gera_permutacoes_determinsticas() {
        let a = ObjectId(1);
        let blockers = vec![ObjectId(10), ObjectId(11), ObjectId(12)];
        let first = order_options(a, &blockers);
        let second = order_options(a, &blockers);
        assert_eq!(first, second, "mesma entrada, mesma saída");
        assert_eq!(first.len(), 6, "3! permutações");
        assert!(first.len() <= MAX_ORDER_OPTIONS);
    }
}
