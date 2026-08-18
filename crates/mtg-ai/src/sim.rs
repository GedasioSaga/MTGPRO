//! Simulação aproximada de combate e de efeitos, sobre `Snapshot`.
//!
//! Por que um modelo próprio em vez de rodar o motor: `Game` não é clonável
//! (`Vec<Box<dyn Agent>>`) e o motor não expõe "aplique esta ação e devolva o
//! estado" — `turn::run_game` só sabe jogar a partida inteira. Então a busca
//! rasa usa este modelo: exato o bastante para combate (que é onde a maioria
//! das decisões de uma partida se decide) e otimista para o resto.
//!
//! Limites conscientes, documentados em vez de escondidos: gatilhos não
//! disparam, efeitos de substituição não se aplicam, `Conditional` assume o
//! ramo `then`, e `ForEach`/`Repeat` são ignorados.
use mtg_core::action::{Action, TargetChoice};
use mtg_core::engine::Game;
use mtg_core::ids::{ObjectId, PlayerId};
use mtg_core::ir::{Effect, ObjRef, PlayerRef, Value};
use mtg_core::types::CounterKind;

use crate::eval::{CreatureInfo, Side, Snapshot, Traits, LIFE};

// ---------------------------------------------------------------------------
// Combate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Fighter {
    id: ObjectId,
    side: Side,
    power: i32,
    /// Resistência já descontada do dano marcado antes do combate.
    toughness_left: i32,
    traits: Traits,
    alive: bool,
    damage_taken: i32,
    hit_by_deathtouch: bool,
}

impl Fighter {
    fn from(c: &CreatureInfo, side: Side) -> Fighter {
        Fighter {
            id: c.id,
            side,
            power: c.power.max(0),
            toughness_left: c.effective_toughness(),
            traits: c.traits,
            alive: true,
            damage_taken: 0,
            hit_by_deathtouch: false,
        }
    }

    /// Dano necessário para ser letal agora (CR 510.1a).
    fn lethal_needed(&self) -> i32 {
        (self.toughness_left - self.damage_taken).max(1)
    }

    fn should_die(&self) -> bool {
        if self.traits.indestructible {
            return false;
        }
        // CR 704.5g / 702.2b: dano letal ou qualquer dano de toque mortal.
        self.hit_by_deathtouch || self.toughness_left - self.damage_taken <= 0
    }
}

fn find_fighter(fighters: &[Fighter], id: ObjectId) -> Option<usize> {
    fighters.iter().position(|f| f.id == id)
}

/// Resolve um combate inteiro sobre o snapshot: dano, mortes, vira atacantes,
/// vínculo com a vida. `blocks` são pares (bloqueador, atacante).
pub fn simulate_combat(
    s: &mut Snapshot,
    attacker_side: Side,
    attackers: &[ObjectId],
    blocks: &[(ObjectId, ObjectId)],
) {
    let defender_side = attacker_side.other();

    let mut fighters: Vec<Fighter> = Vec::new();
    let mut groups: Vec<(ObjectId, Vec<ObjectId>)> = Vec::new();
    for atk in attackers {
        let Some(info) = s.find(*atk) else { continue };
        if info.controller != side_player(s, attacker_side) {
            continue;
        }
        fighters.push(Fighter::from(info, attacker_side));
        let blockers: Vec<ObjectId> = blocks
            .iter()
            .filter(|(_, a)| a == atk)
            .map(|(b, _)| *b)
            .collect();
        for b in &blockers {
            if find_fighter(&fighters, *b).is_none() {
                if let Some(info) = s.find(*b) {
                    fighters.push(Fighter::from(info, defender_side));
                }
            }
        }
        groups.push((*atk, blockers));
    }
    if groups.is_empty() {
        return;
    }

    let mut life_delta: [i32; 2] = [0, 0];
    // Duas passagens: primeiro golpe e depois dano normal (CR 510.4).
    for first_strike_pass in [true, false] {
        let mut pending: Vec<(ObjectId, i32, bool)> = Vec::new();
        let mut player_damage: Vec<(Side, i32, Side, bool)> = Vec::new();

        for (atk_id, blockers) in &groups {
            let Some(ai) = find_fighter(&fighters, *atk_id) else {
                continue;
            };
            let atk = fighters[ai].clone();
            if !atk.alive || !strikes_in(atk.traits, first_strike_pass) {
                continue;
            }
            let live: Vec<ObjectId> = blockers
                .iter()
                .copied()
                .filter(|b| {
                    find_fighter(&fighters, *b).is_some_and(|i| fighters[i].alive)
                })
                .collect();

            if blockers.is_empty() {
                player_damage.push((defender_side, atk.power, attacker_side, atk.traits.lifelink));
                continue;
            }
            if live.is_empty() {
                // Bloqueado e sem bloqueador vivo: não causa dano nenhum,
                // salvo atropelar, que segue para o defensor (CR 702.19b).
                if atk.traits.trample {
                    player_damage.push((
                        defender_side,
                        atk.power,
                        attacker_side,
                        atk.traits.lifelink,
                    ));
                }
                continue;
            }

            let mut remaining = atk.power;
            for b in live {
                if remaining <= 0 {
                    break;
                }
                let Some(bi) = find_fighter(&fighters, b) else {
                    continue;
                };
                let need = if atk.traits.deathtouch {
                    1
                } else {
                    fighters[bi].lethal_needed()
                };
                let give = remaining.min(need);
                remaining -= give;
                pending.push((b, give, atk.traits.deathtouch));
                if atk.traits.lifelink {
                    // Vínculo com a vida credita o controlador da própria criatura
                    // (CR 702.15b), por isso o lado sai do lutador e não do laço.
                    life_delta[side_index(atk.side)] += give;
                }
            }
            if remaining > 0 && atk.traits.trample {
                player_damage.push((defender_side, remaining, attacker_side, atk.traits.lifelink));
            }
        }

        // Bloqueadores batem no atacante que bloqueiam (CR 510.1c).
        for (atk_id, blockers) in &groups {
            for b in blockers {
                let Some(bi) = find_fighter(&fighters, *b) else {
                    continue;
                };
                let blk = fighters[bi].clone();
                if !blk.alive || !strikes_in(blk.traits, first_strike_pass) || blk.power <= 0 {
                    continue;
                }
                pending.push((*atk_id, blk.power, blk.traits.deathtouch));
                if blk.traits.lifelink {
                    // Mesmo motivo do atacante: o crédito segue o lado do lutador.
                    life_delta[side_index(blk.side)] += blk.power;
                }
            }
        }

        for (target, amount, deathtouch) in pending {
            if let Some(i) = find_fighter(&fighters, target) {
                fighters[i].damage_taken += amount;
                if deathtouch && amount > 0 {
                    fighters[i].hit_by_deathtouch = true;
                }
            }
        }
        for (target, amount, source_side, lifelink) in player_damage {
            life_delta[side_index(target)] -= amount;
            if lifelink {
                life_delta[side_index(source_side)] += amount;
            }
        }
        for f in fighters.iter_mut() {
            if f.alive && f.should_die() {
                f.alive = false;
            }
        }
    }

    // Escreve o resultado de volta no snapshot.
    for f in &fighters {
        if !f.alive {
            s.remove_creature(f.id);
            continue;
        }
        if let Some(c) = s.find_mut(f.id) {
            c.damage += f.damage_taken;
            c.attacking = false;
            c.blocking = false;
        }
    }
    for atk in attackers {
        if let Some(c) = s.find_mut(*atk) {
            // Vigilância não vira ao atacar (CR 702.20b).
            if !c.traits.vigilance {
                c.tapped = true;
            }
        }
    }
    s.add_life(Side::Me, life_delta[side_index(Side::Me)]);
    s.add_life(Side::Opponent, life_delta[side_index(Side::Opponent)]);
}

fn strikes_in(t: Traits, first_strike_pass: bool) -> bool {
    if first_strike_pass {
        t.strikes_first()
    } else {
        t.strikes_normal()
    }
}

fn side_index(s: Side) -> usize {
    match s {
        Side::Me => 0,
        Side::Opponent => 1,
    }
}

fn side_player(s: &Snapshot, side: Side) -> PlayerId {
    match side {
        Side::Me => s.me,
        Side::Opponent => s.opponent,
    }
}

// ---------------------------------------------------------------------------
// Plano de bloqueio
// ---------------------------------------------------------------------------

/// Quanto vale este bloqueio, do ponto de vista de quem bloqueia.
/// `forced` liga o modo "vou morrer se não bloquear": aí bloquear de graça
/// (chump block) passa a valer mais que o corpo perdido.
pub fn block_score(
    blockers: &[&CreatureInfo],
    attacker: &CreatureInfo,
    forced: bool,
) -> i64 {
    if blockers.is_empty() {
        return 0;
    }
    let total_power: i32 = blockers.iter().map(|b| b.power.max(0)).sum();
    let any_deathtouch = blockers.iter().any(|b| b.traits.deathtouch);
    let kills_attacker = !attacker.traits.indestructible
        && ((any_deathtouch && total_power > 0)
            || total_power >= attacker.effective_toughness());

    // Atropelar entrega o excedente ao jogador de qualquer jeito.
    let absorbed: i32 = blockers
        .iter()
        .map(|b| b.effective_toughness().max(0))
        .sum();
    let prevented = if attacker.traits.trample {
        attacker.power.max(0).min(absorbed)
    } else {
        attacker.power.max(0)
    };

    // O atacante distribui o dano matando do mais frágil para o mais caro.
    let mut ordered: Vec<&&CreatureInfo> = blockers.iter().collect();
    ordered.sort_by(|a, b| {
        a.effective_toughness()
            .cmp(&b.effective_toughness())
            .then(a.id.cmp(&b.id))
    });
    let mut remaining = attacker.power.max(0);
    let mut lost = 0i64;
    for b in ordered {
        let need = if attacker.traits.deathtouch {
            1
        } else {
            b.effective_toughness().max(1)
        };
        if remaining >= need && !b.traits.indestructible {
            remaining -= need;
            lost += b.value();
        }
    }
    // Primeiro golpe muda quem morre: se o atacante bate antes e mata, os
    // bloqueadores mortos não devolvem dano (CR 510.4).
    let attacker_strikes_first = attacker.traits.strikes_first()
        && !blockers.iter().any(|b| b.traits.strikes_first());
    let kills_attacker = kills_attacker && !(attacker_strikes_first && lost > 0 && blockers.len() == 1);

    let mut score = prevented as i64 * LIFE;
    if kills_attacker {
        score += attacker.value();
    }
    score -= lost;
    if forced {
        // Sobreviver vale mais que material: triplica o valor do dano evitado.
        score += prevented as i64 * LIFE * 3;
    }
    score
}

/// Bloqueios que o lado `defender` faria contra estes atacantes.
/// Usado tanto para escolher os meus bloqueios quanto para prever os do
/// oponente ao avaliar um ataque — o mesmo cérebro dos dois lados.
pub fn plan_blocks(
    s: &Snapshot,
    defender: Side,
    attackers: &[ObjectId],
) -> Vec<(ObjectId, ObjectId)> {
    let mut order: Vec<CreatureInfo> = attackers
        .iter()
        .filter_map(|id| s.find(*id).cloned())
        .collect();
    order.sort_by(|a, b| b.power.cmp(&a.power).then(a.id.cmp(&b.id)));

    let incoming: i32 = order.iter().map(|a| a.power.max(0)).sum();
    let forced = incoming >= s.life(defender);

    let mut available: Vec<CreatureInfo> = s
        .creatures(defender)
        .iter()
        .filter(|c| c.can_block_now())
        .cloned()
        .collect();
    available.sort_by_key(|c| c.id);

    let mut pairs: Vec<(ObjectId, ObjectId)> = Vec::new();
    for atk in &order {
        let needed = if atk.traits.menace { 2 } else { 1 };
        let candidates: Vec<usize> = available
            .iter()
            .enumerate()
            .filter(|(_, b)| b.can_block_attacker(atk))
            .map(|(i, _)| i)
            .collect();
        if candidates.len() < needed {
            continue;
        }

        let chosen = if needed == 1 {
            best_single_blocker(&available, &candidates, atk, forced)
        } else {
            best_pair_blockers(&available, &candidates, atk, forced)
        };
        let Some(indices) = chosen else { continue };
        for &i in &indices {
            pairs.push((available[i].id, atk.id));
        }
        let mut sorted = indices;
        sorted.sort_unstable();
        for i in sorted.into_iter().rev() {
            available.remove(i);
        }
    }
    pairs
}

fn best_single_blocker(
    available: &[CreatureInfo],
    candidates: &[usize],
    attacker: &CreatureInfo,
    forced: bool,
) -> Option<Vec<usize>> {
    let mut best: Option<(i64, usize)> = None;
    for &i in candidates {
        let score = block_score(&[&available[i]], attacker, forced);
        // Desempate pelo menor índice mantém a escolha estável.
        if best.is_none_or(|(bs, bi)| score > bs || (score == bs && i < bi)) {
            best = Some((score, i));
        }
    }
    match best {
        Some((score, i)) if score > 0 => Some(vec![i]),
        _ => None,
    }
}

fn best_pair_blockers(
    available: &[CreatureInfo],
    candidates: &[usize],
    attacker: &CreatureInfo,
    forced: bool,
) -> Option<Vec<usize>> {
    let mut best: Option<(i64, usize, usize)> = None;
    for (n, &i) in candidates.iter().enumerate() {
        for &j in candidates.iter().skip(n + 1) {
            let score = block_score(&[&available[i], &available[j]], attacker, forced);
            if best.is_none_or(|(bs, bi, bj)| score > bs || (score == bs && (i, j) < (bi, bj))) {
                best = Some((score, i, j));
            }
        }
    }
    match best {
        Some((score, i, j)) if score > 0 => Some(vec![i, j]),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Aplicação aproximada de uma ação
// ---------------------------------------------------------------------------

/// Aplica uma ação ao snapshot, prevendo o efeito. É a função que dá um passo
/// à frente para a busca rasa. Ação desconhecida é no-op (não inventa efeito).
pub fn apply_action(s: &mut Snapshot, game: &Game, action: &Action) {
    match action {
        Action::PlayLand { .. } => {
            s.my_lands += 1;
            s.my_mana_sources += 1;
            s.my_hand = s.my_hand.saturating_sub(1);
            s.lands_played_this_turn = s.lands_played_this_turn.saturating_add(1);
        }
        Action::CastSpell {
            object,
            targets,
            x,
            ..
        } => {
            s.my_hand = s.my_hand.saturating_sub(1);
            apply_spell(s, game, *object, targets, *x);
        }
        Action::ActivateAbility {
            source,
            index,
            targets,
            x,
            ..
        } => {
            apply_activated(s, game, *source, *index, targets, *x);
        }
        Action::Attack { assignments } => {
            let attackers: Vec<ObjectId> = assignments.iter().map(|(id, _)| *id).collect();
            let blocks = plan_blocks(s, Side::Opponent, &attackers);
            simulate_combat(s, Side::Me, &attackers, &blocks);
        }
        Action::Block { assignments } => {
            let attackers: Vec<ObjectId> = s
                .opp_creatures
                .iter()
                .filter(|c| c.attacking)
                .map(|c| c.id)
                .collect();
            simulate_combat(s, Side::Opponent, &attackers, assignments);
        }
        _ => {}
    }
}

fn apply_spell(s: &mut Snapshot, game: &Game, object: ObjectId, targets: &[TargetChoice], x: u32) {
    let Some(state_obj) = game.state.object(object) else {
        return;
    };
    let Some(def) = game.db.get(state_obj.card) else {
        return;
    };

    if def.type_line.is_creature() {
        let mut c = CreatureInfo::vanilla(
            object,
            s.me,
            def.power.unwrap_or(0),
            def.toughness.unwrap_or(0),
        );
        c.mana_value = def.mana_value();
        c.traits = Traits::from_keywords(&def.keywords().cloned().collect::<Vec<_>>());
        c.summoning_sick = true;
        s.my_creatures.push(c);
    } else if def.type_line.is_permanent() && !def.type_line.is_land() {
        s.my_nonland_permanents += 1;
    }

    if let Some(effect) = &def.spell_effect {
        predict_effect(s, effect, targets, x, Side::Me, Some(object));
    }
}

fn apply_activated(
    s: &mut Snapshot,
    game: &Game,
    source: ObjectId,
    index: u16,
    targets: &[TargetChoice],
    x: u32,
) {
    let Some(state_obj) = game.state.object(source) else {
        return;
    };
    let Some(def) = game.db.get(state_obj.card) else {
        return;
    };
    let Some((_, ability)) = def.activated().find(|(i, _)| *i == index as usize) else {
        return;
    };
    predict_effect(s, &ability.effect, targets, x, Side::Me, Some(source));
}

/// Valor numérico previsível sem contexto de motor. O que depende do estado
/// (contagens, poder de outro objeto) vira 0: previsão conservadora é melhor
/// que previsão inventada.
fn value_of(v: &Value, x: u32) -> i32 {
    match v {
        Value::Const(n) => *n,
        Value::X => x as i32,
        Value::Add(a, b) => value_of(a, x) + value_of(b, x),
        Value::Sub(a, b) => value_of(a, x) - value_of(b, x),
        Value::Mul(a, b) => value_of(a, x) * value_of(b, x),
        Value::Neg(a) => -value_of(a, x),
        Value::Max(a, b) => value_of(a, x).max(value_of(b, x)),
        Value::Min(a, b) => value_of(a, x).min(value_of(b, x)),
        _ => 0,
    }
}

fn target_object(targets: &[TargetChoice], r: &ObjRef, self_obj: Option<ObjectId>) -> Option<ObjectId> {
    match r {
        ObjRef::SelfObject => self_obj,
        ObjRef::Target(i) => match targets.get(*i as usize) {
            Some(TargetChoice::Object(id)) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

fn target_side(
    s: &Snapshot,
    targets: &[TargetChoice],
    r: &PlayerRef,
    controller: Side,
) -> Option<Side> {
    match r {
        PlayerRef::You => Some(controller),
        PlayerRef::Opponents => Some(controller.other()),
        PlayerRef::Target(i) => match targets.get(*i as usize) {
            Some(TargetChoice::Player(p)) => {
                if *p == s.me {
                    Some(Side::Me)
                } else {
                    Some(Side::Opponent)
                }
            }
            _ => None,
        },
        _ => None,
    }
}

fn damage_creature(s: &mut Snapshot, id: ObjectId, amount: i32, deathtouch: bool) {
    let dies = match s.find_mut(id) {
        Some(c) => {
            c.damage += amount;
            !c.traits.indestructible
                && amount > 0
                && (deathtouch || c.effective_toughness() <= 0)
        }
        None => false,
    };
    if dies {
        s.remove_creature(id);
    }
}

/// Prevê o efeito de uma mágica/habilidade sobre o snapshot. Cobre o
/// vocabulário que muda avaliação; o resto é ignorado de propósito.
pub fn predict_effect(
    s: &mut Snapshot,
    effect: &Effect,
    targets: &[TargetChoice],
    x: u32,
    controller: Side,
    self_obj: Option<ObjectId>,
) {
    match effect {
        Effect::Sequence(list) => {
            for e in list {
                predict_effect(s, e, targets, x, controller, self_obj);
            }
        }
        // O bot só escolhe "sim" quando o efeito o favorece, então prever o
        // ramo executado é coerente com como ele decide de fato.
        Effect::May { do_, .. } => predict_effect(s, do_, targets, x, controller, self_obj),
        Effect::Conditional { then_do, .. } => {
            predict_effect(s, then_do, targets, x, controller, self_obj)
        }
        Effect::DealDamage { amount, target } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                damage_creature(s, id, value_of(amount, x), false);
            }
        }
        Effect::DealDamageToPlayer { amount, player } => {
            if let Some(side) = target_side(s, targets, player, controller) {
                s.add_life(side, -value_of(amount, x));
            }
        }
        Effect::DivideDamage { total, targets: idx } => {
            // Sem saber a divisão escolhida, concentra no primeiro alvo:
            // é o pior caso para o oponente e o mais comum na prática.
            if let Some(first) = idx.first() {
                if let Some(TargetChoice::Object(id)) = targets.get(*first as usize) {
                    damage_creature(s, *id, value_of(total, x), false);
                }
            }
        }
        Effect::GainLife { amount, player } => {
            if let Some(side) = target_side(s, targets, player, controller) {
                s.add_life(side, value_of(amount, x));
            }
        }
        Effect::LoseLife { amount, player } => {
            if let Some(side) = target_side(s, targets, player, controller) {
                s.add_life(side, -value_of(amount, x));
            }
        }
        Effect::SetLife { amount, player } => {
            if let Some(side) = target_side(s, targets, player, controller) {
                let delta = value_of(amount, x) - s.life(side);
                s.add_life(side, delta);
            }
        }
        Effect::DrawCards { count, player } => {
            if let Some(side) = target_side(s, targets, player, controller) {
                let n = value_of(count, x).max(0) as usize;
                match side {
                    Side::Me => {
                        s.my_hand += n;
                        s.my_library = s.my_library.saturating_sub(n);
                    }
                    Side::Opponent => {
                        s.opp_hand += n;
                        s.opp_library = s.opp_library.saturating_sub(n);
                    }
                }
            }
        }
        Effect::Discard { count, player, .. } => {
            if let Some(side) = target_side(s, targets, player, controller) {
                let n = value_of(count, x).max(0) as usize;
                match side {
                    Side::Me => s.my_hand = s.my_hand.saturating_sub(n),
                    Side::Opponent => s.opp_hand = s.opp_hand.saturating_sub(n),
                }
            }
        }
        Effect::Mill { count, player } => {
            if let Some(side) = target_side(s, targets, player, controller) {
                let n = value_of(count, x).max(0) as usize;
                match side {
                    Side::Me => s.my_library = s.my_library.saturating_sub(n),
                    Side::Opponent => s.opp_library = s.opp_library.saturating_sub(n),
                }
            }
        }
        Effect::Destroy { target, .. } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                let indestructible = s.find(id).is_some_and(|c| c.traits.indestructible);
                if !indestructible {
                    s.remove_creature(id);
                }
            }
        }
        Effect::Exile { target, .. } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                s.remove_creature(id);
            }
        }
        Effect::ReturnToHand { target } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                if let Some(side) = s.side_of(id) {
                    match side {
                        Side::Me => s.my_hand += 1,
                        Side::Opponent => s.opp_hand += 1,
                    }
                }
                s.remove_creature(id);
            }
        }
        Effect::Tap { target } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                if let Some(c) = s.find_mut(id) {
                    c.tapped = true;
                }
            }
        }
        Effect::Untap { target } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                if let Some(c) = s.find_mut(id) {
                    c.tapped = false;
                }
            }
        }
        Effect::ModifyPT {
            target,
            power,
            toughness,
            ..
        } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                let (dp, dt) = (value_of(power, x), value_of(toughness, x));
                if let Some(c) = s.find_mut(id) {
                    c.power += dp;
                    c.toughness += dt;
                }
                // Redutor de resistência pode matar (CR 704.5f).
                if s.find(id).is_some_and(|c| c.effective_toughness() <= 0) {
                    s.remove_creature(id);
                }
            }
        }
        Effect::SetPT {
            target,
            power,
            toughness,
            ..
        } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                let (np, nt) = (value_of(power, x), value_of(toughness, x));
                if let Some(c) = s.find_mut(id) {
                    c.power = np;
                    c.toughness = nt;
                }
            }
        }
        Effect::GrantKeywords {
            target, keywords, ..
        } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                let extra = Traits::from_keywords(keywords);
                if let Some(c) = s.find_mut(id) {
                    c.traits = merge_traits(c.traits, extra);
                }
            }
        }
        Effect::AddCounters {
            target,
            kind,
            count,
        } => {
            if let Some(id) = target_object(targets, target, self_obj) {
                let n = value_of(count, x);
                if let Some(c) = s.find_mut(id) {
                    match kind {
                        CounterKind::PlusOnePlusOne => {
                            c.power += n;
                            c.toughness += n;
                        }
                        CounterKind::MinusOneMinusOne => {
                            c.power -= n;
                            c.toughness -= n;
                        }
                        _ => {}
                    }
                }
                if s.find(id).is_some_and(|c| c.effective_toughness() <= 0) {
                    s.remove_creature(id);
                }
            }
        }
        Effect::CreateToken {
            spec,
            count,
            controller: who,
        } => {
            if let Some(side) = target_side(s, targets, who, controller) {
                let n = value_of(count, x).max(0).min(8);
                for k in 0..n {
                    // Id sintético fora da faixa real: só existe na simulação.
                    let id = ObjectId(u32::MAX - 1 - k as u32);
                    let owner = if side == Side::Me { s.me } else { s.opponent };
                    let mut c = CreatureInfo::vanilla(id, owner, spec.power, spec.toughness);
                    c.traits = Traits::from_keywords(&spec.keywords);
                    c.summoning_sick = true;
                    s.creatures_mut(side).push(c);
                }
            }
        }
        Effect::Fight { a, b } => {
            let (ida, idb) = (
                target_object(targets, a, self_obj),
                target_object(targets, b, self_obj),
            );
            if let (Some(ida), Some(idb)) = (ida, idb) {
                let pa = s.find(ida).map_or(0, |c| c.power);
                let pb = s.find(idb).map_or(0, |c| c.power);
                let dta = s.find(ida).is_some_and(|c| c.traits.deathtouch);
                let dtb = s.find(idb).is_some_and(|c| c.traits.deathtouch);
                damage_creature(s, idb, pa, dta);
                damage_creature(s, ida, pb, dtb);
            }
        }
        _ => {}
    }
}

fn merge_traits(base: Traits, extra: Traits) -> Traits {
    Traits {
        flying: base.flying || extra.flying,
        reach: base.reach || extra.reach,
        trample: base.trample || extra.trample,
        first_strike: base.first_strike || extra.first_strike,
        double_strike: base.double_strike || extra.double_strike,
        deathtouch: base.deathtouch || extra.deathtouch,
        lifelink: base.lifelink || extra.lifelink,
        vigilance: base.vigilance || extra.vigilance,
        haste: base.haste || extra.haste,
        menace: base.menace || extra.menace,
        defender: base.defender || extra.defender,
        indestructible: base.indestructible || extra.indestructible,
        hexproof: base.hexproof || extra.hexproof,
        shroud: base.shroud || extra.shroud,
        protection: base.protection || extra.protection,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::Snapshot;
    use mtg_core::ids::PlayerId;

    const ME: PlayerId = PlayerId(0);
    const OPP: PlayerId = PlayerId(1);

    /// Snapshot com criaturas de teste em cada lado.
    fn board(mine: &[(u32, i32, i32)], theirs: &[(u32, i32, i32)]) -> Snapshot {
        let mut s = Snapshot::empty(ME, OPP);
        for (id, p, t) in mine {
            s.my_creatures.push(CreatureInfo::vanilla(ObjectId(*id), ME, *p, *t));
        }
        for (id, p, t) in theirs {
            s.opp_creatures.push(CreatureInfo::vanilla(ObjectId(*id), OPP, *p, *t));
        }
        s
    }

    fn set_trait(s: &mut Snapshot, id: u32, f: impl Fn(&mut Traits)) {
        if let Some(c) = s.find_mut(ObjectId(id)) {
            f(&mut c.traits);
        }
    }

    #[test]
    fn atacante_sem_bloqueio_tira_vida_do_defensor() {
        let mut s = board(&[(1, 3, 3)], &[]);
        simulate_combat(&mut s, Side::Me, &[ObjectId(1)], &[]);
        assert_eq!(s.life(Side::Opponent), 17);
        assert_eq!(s.life(Side::Me), 20);
    }

    #[test]
    fn vinculo_com_a_vida_credita_o_dono_do_atacante() {
        // CR 702.15b: quem ganha vida é o controlador da fonte do dano, não o
        // "lado atacante" por definição — é o que a regressão aqui protege.
        let mut s = board(&[(1, 3, 3)], &[]);
        set_trait(&mut s, 1, |t| t.lifelink = true);
        simulate_combat(&mut s, Side::Me, &[ObjectId(1)], &[]);
        assert_eq!(s.life(Side::Me), 23, "atacante com vínculo não creditou o dono");
        assert_eq!(s.life(Side::Opponent), 17);
    }

    #[test]
    fn vinculo_com_a_vida_do_bloqueador_credita_o_defensor() {
        let mut s = board(&[(1, 1, 4)], &[(2, 2, 4)]);
        set_trait(&mut s, 2, |t| t.lifelink = true);
        simulate_combat(&mut s, Side::Me, &[ObjectId(1)], &[(ObjectId(2), ObjectId(1))]);
        assert_eq!(s.life(Side::Opponent), 22, "bloqueador com vínculo não creditou o dono");
        assert_eq!(s.life(Side::Me), 20, "bloqueado não deveria passar dano");
    }

    #[test]
    fn toque_mortal_mata_bloqueador_maior() {
        // CR 702.2b: qualquer dano de toque mortal é letal.
        let mut s = board(&[(1, 1, 1)], &[(2, 0, 9)]);
        set_trait(&mut s, 1, |t| t.deathtouch = true);
        simulate_combat(&mut s, Side::Me, &[ObjectId(1)], &[(ObjectId(2), ObjectId(1))]);
        assert!(s.creatures(Side::Opponent).is_empty(), "toque mortal não matou o bloqueador");
    }

    #[test]
    fn indestrutivel_sobrevive_a_dano_letal() {
        // CR 702.12b: indestrutível ignora dano letal e toque mortal.
        let mut s = board(&[(1, 9, 9)], &[(2, 0, 1)]);
        set_trait(&mut s, 1, |t| t.deathtouch = true);
        set_trait(&mut s, 2, |t| t.indestructible = true);
        simulate_combat(&mut s, Side::Me, &[ObjectId(1)], &[(ObjectId(2), ObjectId(1))]);
        assert_eq!(s.creatures(Side::Opponent).len(), 1, "indestrutível morreu");
    }

    #[test]
    fn atropelar_passa_o_excedente() {
        // CR 702.19b: só o que sobra depois do letal ao bloqueador passa.
        let mut s = board(&[(1, 5, 5)], &[(2, 0, 2)]);
        set_trait(&mut s, 1, |t| t.trample = true);
        simulate_combat(&mut s, Side::Me, &[ObjectId(1)], &[(ObjectId(2), ObjectId(1))]);
        assert_eq!(s.life(Side::Opponent), 17, "excedente de atropelar errado");
    }

    #[test]
    fn vigilancia_nao_vira_ao_atacar() {
        // CR 702.20b.
        let mut s = board(&[(1, 2, 2), (2, 2, 2)], &[]);
        set_trait(&mut s, 1, |t| t.vigilance = true);
        simulate_combat(&mut s, Side::Me, &[ObjectId(1), ObjectId(2)], &[]);
        let tapped: Vec<bool> = s.creatures(Side::Me).iter().map(|c| c.tapped).collect();
        assert_eq!(tapped, vec![false, true]);
    }
}
