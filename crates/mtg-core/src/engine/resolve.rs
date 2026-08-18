//! Interpretador do IR de efeitos (CR 608.2).
//!
//! Toda carta do jogo é uma árvore `Effect`; este módulo é o único lugar que
//! sabe executá-la. Nenhuma variante pode entrar em pânico: o que não dá para
//! aplicar vira linha de log e a resolução continua, porque abortar no meio de
//! um efeito deixa o estado do jogo inconsistente — pior que aplicar menos.
//!
//! Regras estruturais seguidas aqui:
//!   - números e condições sempre passam por `query`, nunca são lidos direto;
//!   - efeito contínuo criado por resolução trava seus valores agora (CR 611.2c);
//!   - toda escolha vira uma `Request` para o agente, nunca um chute do motor.
use super::query::{self, EvalCtx};
use super::{cast, stack, turn, Game};
use crate::action::{Action, Request, TargetChoice};
use crate::event::{DamageKind, Defender, GameEvent, LossReason};
use crate::ids::{CardDefId, ObjectId, PlayerId};
use crate::ir::{
    Cost, Duration, Effect, Keyword, ObjRef, StaticModRuntime, TokenSpec,
};
use crate::mana::ManaSymbol;
use crate::state::{ContinuousEffect, ObjectState};
use crate::types::{CardType, CounterKind};
use crate::view::MatchEvent;
use crate::zone::{Zone, ZoneId};
use rand::seq::SliceRandom;

/// Fichas não têm carta impressa. `db.get` devolve `None` para este id e quem
/// precisa das características lê `ObjectState.token_spec`.
pub const TOKEN_NO_CARD: CardDefId = CardDefId(u32::MAX);

/// Teto de opções enumeradas para escolhas combinatórias (modos, seleções).
const MAX_CHOICE_OPTIONS: usize = 60;
/// Teto de arranjos enumerados para scry/surveil.
const MAX_ARRANGE_OPTIONS: usize = 24;
/// Trava contra `Repeat` com valor descontrolado.
const MAX_REPEAT: i32 = 100;
/// Trava de crescimento da lista de objetos lembrados numa mesma resolução.
const MAX_REMEMBERED: usize = 32;

// ---------------------------------------------------------------------------
// Ponto de entrada
// ---------------------------------------------------------------------------

pub fn resolve_effect(game: &mut Game, effect: &Effect, ctx: &mut EvalCtx) {
    // Efeito de mágica que já ganhou o jogo não continua resolvendo (CR 104.1).
    if game.is_over() {
        return;
    }

    match effect {
        Effect::Nothing => {}

        Effect::Sequence(effects) => {
            for e in effects {
                if game.is_over() {
                    break;
                }
                resolve_effect(game, e, ctx);
            }
        }

        // -------------------------------------------------------------------
        // Dano e vida
        // -------------------------------------------------------------------
        Effect::DealDamage { amount, target } => {
            let amount = query::eval_value(game, amount, ctx);
            if amount <= 0 {
                return;
            }
            // "Alvo de criatura ou jogador": o mesmo `ObjRef::Target` pode ter
            // sido preenchido com um jogador. Respeita o que foi escolhido.
            if let ObjRef::Target(i) = target {
                if let Some(TargetChoice::Player(p)) = ctx.targets.get(*i as usize).copied() {
                    deal_damage_to_player(game, ctx.source, p, amount, DamageKind::Noncombat);
                    return;
                }
            }
            for obj in query::resolve_objects(game, target, ctx) {
                deal_damage_to_object(game, ctx.source, obj, amount, DamageKind::Noncombat);
            }
        }

        Effect::DealDamageToPlayer { amount, player } => {
            let amount = query::eval_value(game, amount, ctx);
            if amount <= 0 {
                return;
            }
            for p in query::resolve_players(game, player, ctx) {
                deal_damage_to_player(game, ctx.source, p, amount, DamageKind::Noncombat);
            }
        }

        Effect::DivideDamage { total, targets } => {
            let total = query::eval_value(game, total, ctx);
            let picks: Vec<TargetChoice> = targets
                .iter()
                .filter_map(|i| ctx.targets.get(*i as usize).copied())
                .collect();
            if total <= 0 || picks.is_empty() {
                return;
            }
            // CR 601.2d: a divisão é anunciada ao lançar. Sem canal para isso no
            // IR, distribui o mais parelho possível — o resto vai aos primeiros.
            let n = picks.len() as i32;
            let base = total / n;
            let extra = total % n;
            for (i, choice) in picks.iter().enumerate() {
                let amount = base + if (i as i32) < extra { 1 } else { 0 };
                match *choice {
                    TargetChoice::Object(o) => {
                        deal_damage_to_object(game, ctx.source, o, amount, DamageKind::Noncombat)
                    }
                    TargetChoice::Player(p) => {
                        deal_damage_to_player(game, ctx.source, p, amount, DamageKind::Noncombat)
                    }
                }
            }
        }

        Effect::GainLife { amount, player } => {
            let amount = query::eval_value(game, amount, ctx);
            for p in query::resolve_players(game, player, ctx) {
                gain_life(game, p, amount);
            }
        }

        Effect::LoseLife { amount, player } => {
            let amount = query::eval_value(game, amount, ctx);
            for p in query::resolve_players(game, player, ctx) {
                lose_life(game, p, amount);
            }
        }

        Effect::SetLife { amount, player } => {
            let target_life = query::eval_value(game, amount, ctx);
            for p in query::resolve_players(game, player, ctx) {
                let current = game.state.players.get(p.index()).map(|x| x.life);
                let Some(current) = current else { continue };
                match target_life.cmp(&current) {
                    std::cmp::Ordering::Greater => gain_life(game, p, target_life - current),
                    std::cmp::Ordering::Less => lose_life(game, p, current - target_life),
                    std::cmp::Ordering::Equal => {}
                }
            }
        }

        // -------------------------------------------------------------------
        // Cartas
        // -------------------------------------------------------------------
        Effect::DrawCards { count, player } => {
            let n = query::eval_value(game, count, ctx).max(0);
            for p in query::resolve_players(game, player, ctx) {
                for _ in 0..n {
                    if game.is_over() {
                        break;
                    }
                    // CR 704.5b: comprar de biblioteca vazia não é erro aqui —
                    // a derrota vem da SBA no próximo check.
                    match turn::draw_card(game, p) {
                        Some(id) => remember(ctx, id),
                        None => break,
                    }
                }
            }
        }

        Effect::Discard { count, player, filter, random } => {
            let n = query::eval_value(game, count, ctx).max(0);
            if n == 0 {
                return;
            }
            for p in query::resolve_players(game, player, ctx) {
                let hand = objects_in_zone(game, ZoneId::hand(p));
                let candidates: Vec<ObjectId> = hand
                    .into_iter()
                    .filter(|id| query::matches_filter(game, *id, filter, ctx))
                    .collect();
                if candidates.is_empty() {
                    continue;
                }
                let k = (n as usize).min(candidates.len());
                let chosen = if *random {
                    // Aleatoriedade só pelo rng semeado — replay tem que bater.
                    let mut pool = candidates.clone();
                    pool.shuffle(&mut game.rng);
                    pool.truncate(k);
                    pool
                } else {
                    let k8 = clamp_u8(k);
                    ask_selection(game, p, "descarte".to_string(), candidates, k8, k8)
                };
                for id in chosen {
                    discard_card(game, p, id);
                }
            }
        }

        Effect::Mill { count, player } => {
            let n = query::eval_value(game, count, ctx).max(0);
            for p in query::resolve_players(game, player, ctx) {
                for _ in 0..n {
                    let Some(top) = zone_ref(game, ZoneId::library(p)).and_then(Zone::peek_top)
                    else {
                        break;
                    };
                    turn::move_object(game, top, ZoneId::graveyard(p));
                    game.state.emit(GameEvent::CardMilled { player: p, object: top });
                }
            }
        }

        Effect::Scry { count, player } => {
            let n = query::eval_value(game, count, ctx).max(0);
            for p in query::resolve_players(game, player, ctx) {
                arrange_top_of_library(game, p, n as usize, false);
            }
        }

        Effect::Surveil { count, player } => {
            let n = query::eval_value(game, count, ctx).max(0);
            for p in query::resolve_players(game, player, ctx) {
                arrange_top_of_library(game, p, n as usize, true);
            }
        }

        Effect::SearchLibrary { count, filter, player, to_hand } => {
            let n = clamp_u8(query::eval_value(game, count, ctx).max(0) as usize);
            for p in query::resolve_players(game, player, ctx) {
                let library = objects_in_zone(game, ZoneId::library(p));
                let candidates: Vec<ObjectId> = library
                    .into_iter()
                    .filter(|id| query::matches_filter(game, *id, filter, ctx))
                    .collect();
                // CR 701.19c: embaralha mesmo se nada for encontrado.
                if !candidates.is_empty() && n > 0 {
                    let chosen = ask_selection(
                        game,
                        p,
                        "procure na biblioteca".to_string(),
                        candidates,
                        0,
                        n,
                    );
                    let dest = if *to_hand { ZoneId::hand(p) } else { ZoneId::BATTLEFIELD };
                    for id in chosen {
                        turn::move_object(game, id, dest);
                        remember(ctx, id);
                    }
                }
                shuffle_library(game, p);
            }
        }

        Effect::ShuffleLibrary { player } => {
            for p in query::resolve_players(game, player, ctx) {
                shuffle_library(game, p);
            }
        }

        Effect::PutOnTopOfLibrary { target } => {
            for obj in query::resolve_objects(game, target, ctx) {
                put_into_library(game, obj, true);
            }
        }

        Effect::PutOnBottomOfLibrary { target } => {
            for obj in query::resolve_objects(game, target, ctx) {
                put_into_library(game, obj, false);
            }
        }

        Effect::ReturnToHand { target } => {
            for obj in query::resolve_objects(game, target, ctx) {
                let Some(owner) = game.state.object(obj).map(|o| o.owner) else { continue };
                turn::move_object(game, obj, ZoneId::hand(owner));
            }
        }

        Effect::ReturnFromGraveyardToBattlefield { target } => {
            for obj in query::resolve_objects(game, target, ctx) {
                let in_graveyard = game
                    .state
                    .object(obj)
                    .is_some_and(|o| o.zone.kind == crate::zone::ZoneKind::Graveyard);
                if !in_graveyard {
                    game.state.push_log(
                        format!("{} não está mais no cemitério", game.card_name(obj)),
                        Some(ctx.controller),
                    );
                    continue;
                }
                turn::move_object(game, obj, ZoneId::BATTLEFIELD);
                // O controlador passa a ser quem resolveu o efeito (CR 110.2a).
                if let Some(o) = game.state.object_mut(obj) {
                    o.controller = ctx.controller;
                }
                remember(ctx, obj);
            }
        }

        // -------------------------------------------------------------------
        // Permanentes
        // -------------------------------------------------------------------
        Effect::Destroy { target, no_regeneration } => {
            for obj in query::resolve_objects(game, target, ctx) {
                destroy_object(game, obj, *no_regeneration);
            }
        }

        Effect::Exile { target, until_source_leaves } => {
            for obj in query::resolve_objects(game, target, ctx) {
                turn::move_object(game, obj, ZoneId::EXILE);
                game.state.emit(GameEvent::Exiled { object: obj });
                game.push_event(MatchEvent::Exiled { card: obj });
                if *until_source_leaves {
                    // Precisa de zona de "exílio vinculado" no estado, que não existe.
                    game.state.push_log(
                        "exílio temporário: retorno automático não é rastreado",
                        Some(ctx.controller),
                    );
                }
            }
        }

        Effect::Sacrifice { player, count, filter } => {
            let n = query::eval_value(game, count, ctx).max(0);
            if n == 0 {
                return;
            }
            for p in query::resolve_players(game, player, ctx) {
                let candidates: Vec<ObjectId> = objects_in_zone(game, ZoneId::BATTLEFIELD)
                    .into_iter()
                    .filter(|id| game.state.object(*id).is_some_and(|o| o.controller == p))
                    .filter(|id| query::matches_filter(game, *id, filter, ctx))
                    .collect();
                if candidates.is_empty() {
                    continue;
                }
                // Sacrifício não é opcional: escolhe-se o máximo possível (CR 701.17b).
                let k = clamp_u8((n as usize).min(candidates.len()));
                let chosen = ask_selection(game, p, "sacrifique".to_string(), candidates, k, k);
                for id in chosen {
                    sacrifice_object(game, id);
                }
            }
        }

        Effect::Tap { target } => {
            for obj in query::resolve_objects(game, target, ctx) {
                tap_object(game, obj);
            }
        }

        Effect::Untap { target } => {
            for obj in query::resolve_objects(game, target, ctx) {
                untap_object(game, obj);
            }
        }

        Effect::Freeze { target } => {
            for obj in query::resolve_objects(game, target, ctx) {
                if let Some(o) = game.state.object_mut(obj) {
                    o.frozen = true;
                }
                game.state
                    .push_log(format!("{} não desvira", game.card_name(obj)), Some(ctx.controller));
            }
        }

        Effect::GainControl { target, player, duration } => {
            let Some(new_controller) = query::resolve_players(game, player, ctx).first().copied()
            else {
                return;
            };
            let objs = query::resolve_objects(game, target, ctx);
            match duration {
                // Troca definitiva pode ser gravada no objeto; o resto é camada 2.
                Duration::Instant | Duration::Permanent => {
                    for obj in objs {
                        let Some(from) = game.state.object(obj).map(|o| o.controller) else {
                            continue;
                        };
                        if from == new_controller {
                            continue;
                        }
                        if let Some(o) = game.state.object_mut(obj) {
                            o.controller = new_controller;
                            o.summoning_sick = true;
                        }
                        game.state.emit(GameEvent::ControlChanged {
                            object: obj,
                            from,
                            to: new_controller,
                        });
                    }
                }
                _ => add_continuous(
                    game,
                    ctx,
                    objs,
                    StaticModRuntime::GainControl(new_controller),
                    *duration,
                ),
            }
        }

        Effect::AttachTo { equipment, target } => {
            let equips = query::resolve_objects(game, equipment, ctx);
            let Some(to) = query::resolve_objects(game, target, ctx).first().copied() else {
                game.state
                    .push_log("nada para anexar: alvo ausente", Some(ctx.controller));
                return;
            };
            for eq in equips {
                attach_object(game, eq, to);
            }
        }

        Effect::Unattach { equipment } => {
            for eq in query::resolve_objects(game, equipment, ctx) {
                unattach_object(game, eq);
            }
        }

        Effect::Transform { target } => {
            for obj in query::resolve_objects(game, target, ctx) {
                let Some(o) = game.state.object_mut(obj) else { continue };
                o.flipped = !o.flipped;
                game.state.emit(GameEvent::Transformed { object: obj });
            }
        }

        Effect::CreateToken { spec, count, controller } => {
            let n = query::eval_value(game, count, ctx).max(0);
            if n == 0 {
                return;
            }
            for p in query::resolve_players(game, controller, ctx) {
                for _ in 0..n {
                    if let Some(id) = create_token(game, spec, p) {
                        remember(ctx, id);
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Modificações contínuas — CR 611.2c: os números travam agora
        // -------------------------------------------------------------------
        Effect::ModifyPT { target, power, toughness, duration } => {
            let p = query::eval_value(game, power, ctx);
            let t = query::eval_value(game, toughness, ctx);
            let objs = query::resolve_objects(game, target, ctx);
            add_continuous(game, ctx, objs, StaticModRuntime::ModifyPT(p, t), *duration);
        }

        Effect::SetPT { target, power, toughness, duration } => {
            let p = query::eval_value(game, power, ctx);
            let t = query::eval_value(game, toughness, ctx);
            let objs = query::resolve_objects(game, target, ctx);
            add_continuous(game, ctx, objs, StaticModRuntime::SetPT(p, t), *duration);
        }

        Effect::GrantKeywords { target, keywords, duration } => {
            let objs = query::resolve_objects(game, target, ctx);
            add_continuous(
                game,
                ctx,
                objs,
                StaticModRuntime::GrantKeywords(keywords.clone()),
                *duration,
            );
        }

        Effect::LoseKeywords { target, keywords, duration } => {
            let objs = query::resolve_objects(game, target, ctx);
            add_continuous(
                game,
                ctx,
                objs,
                StaticModRuntime::LoseKeywords(keywords.clone()),
                *duration,
            );
        }

        Effect::CantBeBlocked { target, duration } => {
            let objs = query::resolve_objects(game, target, ctx);
            add_continuous(game, ctx, objs, StaticModRuntime::CantBeBlocked, *duration);
        }

        Effect::CantAttackOrBlock { target, duration } => {
            let objs = query::resolve_objects(game, target, ctx);
            add_continuous(game, ctx, objs, StaticModRuntime::CantAttackOrBlock, *duration);
        }

        Effect::AddCounters { target, kind, count } => {
            let n = query::eval_value(game, count, ctx);
            if n <= 0 {
                return;
            }
            for obj in query::resolve_objects(game, target, ctx) {
                let Some(o) = game.state.object_mut(obj) else { continue };
                o.add_counter(kind.clone(), n);
                game.state.emit(GameEvent::CountersAdded {
                    object: obj,
                    kind: kind.clone(),
                    amount: n,
                });
                game.push_event(MatchEvent::CountersChanged {
                    card: obj,
                    kind: counter_label(kind),
                    delta: n,
                });
            }
        }

        Effect::RemoveCounters { target, kind, count } => {
            let n = query::eval_value(game, count, ctx);
            if n <= 0 {
                return;
            }
            for obj in query::resolve_objects(game, target, ctx) {
                let Some(o) = game.state.object_mut(obj) else { continue };
                // Não existe marcador negativo: remove só o que está lá.
                let removed = o.counter(kind).min(n);
                if removed <= 0 {
                    continue;
                }
                o.add_counter(kind.clone(), -removed);
                game.state.emit(GameEvent::CountersRemoved {
                    object: obj,
                    kind: kind.clone(),
                    amount: removed,
                });
                game.push_event(MatchEvent::CountersChanged {
                    card: obj,
                    kind: counter_label(kind),
                    delta: -removed,
                });
            }
        }

        // -------------------------------------------------------------------
        // Pilha
        // -------------------------------------------------------------------
        Effect::CounterSpell { target, unless_pays } => {
            for obj in query::resolve_objects(game, target, ctx) {
                counter_spell(game, ctx, obj, unless_pays.as_ref());
            }
        }

        Effect::CopySpell { target, count, may_choose_new_targets } => {
            let n = query::eval_value(game, count, ctx).max(0);
            if n == 0 {
                return;
            }
            if *may_choose_new_targets {
                // Reescolher alvo exige o `TargetSpec` original, que a pilha não guarda.
                game.state
                    .push_log("cópia mantém os alvos originais", Some(ctx.controller));
            }
            for obj in query::resolve_objects(game, target, ctx) {
                for _ in 0..n {
                    if let Some(id) = copy_stack_item(game, obj, ctx.controller) {
                        remember(ctx, id);
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Mana
        // -------------------------------------------------------------------
        Effect::AddMana { symbols, player } => {
            for p in query::resolve_players(game, player, ctx) {
                for sym in symbols {
                    add_symbol_to_pool(game, p, *sym);
                }
            }
        }

        Effect::AddManaAnyColor { count, player } => {
            let n = query::eval_value(game, count, ctx).max(0);
            for p in query::resolve_players(game, player, ctx) {
                for _ in 0..n {
                    let color = ask_color(game, p, "escolha a cor do mana".to_string());
                    if let Some(ps) = game.state.players.get_mut(p.index()) {
                        ps.mana_pool.add(Some(color), 1);
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Combate
        // -------------------------------------------------------------------
        Effect::Fight { a, b } => {
            let first = query::resolve_objects(game, a, ctx).first().copied();
            let second = query::resolve_objects(game, b, ctx).first().copied();
            let (Some(x), Some(y)) = (first, second) else {
                game.state
                    .push_log("luta cancelada: falta um dos participantes", Some(ctx.controller));
                return;
            };
            fight(game, x, y);
        }

        Effect::PutOntoBattlefieldAttacking { spec, controller } => {
            for p in query::resolve_players(game, controller, ctx) {
                let Some(id) = create_token(game, spec, p) else { continue };
                let Some(defender) = game.state.opponents(p).first().copied() else { continue };
                let defender = Defender::Player(defender);
                if let Some(o) = game.state.object_mut(id) {
                    o.combat.attacking = Some(defender);
                    // Colocado atacando não "ataca" para efeito de custo/haste,
                    // mas dispara "quando ataca" (CR 508.3a).
                    o.tapped = false;
                }
                game.state.emit(GameEvent::Attacked { object: id, defender });
                remember(ctx, id);
            }
        }

        Effect::ExtraCombatPhase { player } => {
            for p in query::resolve_players(game, player, ctx) {
                if p != game.state.active_player {
                    // Fase extra só cabe no turno de quem a recebe.
                    game.state
                        .push_log("fase de combate extra ignorada: não é o turno do jogador", Some(p));
                    continue;
                }
                game.state.extra_combats = game.state.extra_combats.saturating_add(1);
                game.state.push_log("fase de combate adicional", Some(p));
            }
        }

        Effect::ExtraTurn { player } => {
            for p in query::resolve_players(game, player, ctx) {
                game.state.extra_turns.push(p);
                game.state.push_log("turno extra", Some(p));
            }
        }

        // -------------------------------------------------------------------
        // Controle de fluxo
        // -------------------------------------------------------------------
        Effect::Conditional { cond, then_do, else_do } => {
            if query::eval_condition(game, cond, ctx) {
                resolve_effect(game, then_do, ctx);
            } else if let Some(other) = else_do {
                resolve_effect(game, other, ctx);
            }
        }

        Effect::ForEach { over, do_ } => {
            let items = query::select(game, over, ctx);
            let previous = ctx.selected;
            for id in items {
                if game.is_over() {
                    break;
                }
                ctx.selected = Some(id);
                resolve_effect(game, do_, ctx);
            }
            ctx.selected = previous;
        }

        Effect::Repeat { times, do_ } => {
            let n = query::eval_value(game, times, ctx).clamp(0, MAX_REPEAT);
            for _ in 0..n {
                if game.is_over() {
                    break;
                }
                resolve_effect(game, do_, ctx);
            }
        }

        Effect::May { do_, prompt } => {
            if ask_confirm(game, ctx.controller, prompt.clone()) {
                resolve_effect(game, do_, ctx);
            }
        }

        Effect::Modal { choose, options } => {
            if options.is_empty() {
                return;
            }
            let labels: Vec<String> = options.iter().map(|(l, _)| l.clone()).collect();
            let choose = (*choose).max(1);
            let request = Request::ChooseModes {
                player: ctx.controller,
                prompt: format!("escolha {choose} modo(s)"),
                options: labels,
                choose,
            };
            let modes = match game.ask(request) {
                Action::ChooseModes { modes } => modes,
                // Agente mudo: primeiros modos, para o efeito não sumir.
                _ => (0..(choose as usize).min(options.len()) as u8).collect(),
            };
            for m in modes {
                let Some((label, eff)) = options.get(m as usize) else { continue };
                game.state.push_log(format!("modo: {label}"), Some(ctx.controller));
                resolve_effect(game, eff, ctx);
            }
        }

        // -------------------------------------------------------------------
        // Fim de jogo
        // -------------------------------------------------------------------
        Effect::WinGame { player } => {
            for p in query::resolve_players(game, player, ctx) {
                game.state.push_log("vence a partida por efeito", Some(p));
                // CR 104.2a: vitória por efeito é derrota de todos os outros.
                for opponent in game.state.opponents(p) {
                    turn::lose_game(game, opponent, LossReason::Effect);
                }
            }
        }

        Effect::LoseGame { player } => {
            for p in query::resolve_players(game, player, ctx) {
                turn::lose_game(game, p, LossReason::Effect);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Enumeração de escolhas
// ---------------------------------------------------------------------------

/// Todas as combinações de `choose` modos entre `count` disponíveis.
pub fn mode_options(count: usize, choose: u8) -> Vec<Action> {
    let k = (choose as usize).min(count);
    let mut out: Vec<Action> = combinations(count, k, MAX_CHOICE_OPTIONS)
        .into_iter()
        .map(|combo| Action::ChooseModes { modes: combo.into_iter().map(|i| i as u8).collect() })
        .collect();
    if out.is_empty() {
        out.push(Action::ChooseModes { modes: Vec::new() });
    }
    out
}

/// Todas as seleções de tamanho entre `min` e `max`, em ordem lexicográfica de
/// índice — determinística, que é o que o replay por semente exige.
pub fn selection_options(candidates: &[ObjectId], min: u8, max: u8) -> Vec<Action> {
    let n = candidates.len();
    let lo = (min as usize).min(n);
    let hi = (max as usize).min(n);
    let mut out: Vec<Action> = Vec::new();
    for k in lo..=hi.max(lo) {
        if k > n || out.len() >= MAX_CHOICE_OPTIONS {
            break;
        }
        let budget = MAX_CHOICE_OPTIONS - out.len();
        for combo in combinations(n, k, budget) {
            let objects: Vec<ObjectId> =
                combo.iter().filter_map(|i| candidates.get(*i).copied()).collect();
            out.push(Action::SelectObjects { objects });
        }
    }
    if out.is_empty() {
        out.push(Action::SelectObjects { objects: Vec::new() });
    }
    out
}

/// Partições ordenadas de `cards` em (topo, alternativo) para scry/surveil.
///
/// Primeiro passe enumera um representante por tamanho de topo mantendo a ordem
/// original: garante que "mandar k cartas embora" sempre exista como opção,
/// mesmo quando o teto corta as reordenações.
pub fn arrange_options(cards: &[ObjectId]) -> Vec<Action> {
    let n = cards.len();
    if n == 0 {
        return vec![Action::ArrangeCards { top: Vec::new(), alt: Vec::new() }];
    }

    let mut out: Vec<Action> = Vec::new();
    for k in (0..=n).rev() {
        for combo in combinations(n, k, MAX_ARRANGE_OPTIONS) {
            out.push(build_arrangement(cards, &combo, &combo));
            if out.len() >= MAX_ARRANGE_OPTIONS {
                return out;
            }
        }
    }
    // Segundo passe: reordenações do topo, dos topos maiores para os menores.
    for k in (2..=n).rev() {
        for combo in combinations(n, k, MAX_ARRANGE_OPTIONS) {
            for perm in permutations(&combo, MAX_ARRANGE_OPTIONS) {
                if perm == combo {
                    continue;
                }
                out.push(build_arrangement(cards, &combo, &perm));
                if out.len() >= MAX_ARRANGE_OPTIONS {
                    return out;
                }
            }
        }
    }
    out
}

fn build_arrangement(cards: &[ObjectId], keep: &[usize], order: &[usize]) -> Action {
    let top: Vec<ObjectId> = order.iter().filter_map(|i| cards.get(*i).copied()).collect();
    let alt: Vec<ObjectId> = (0..cards.len())
        .filter(|i| !keep.contains(i))
        .filter_map(|i| cards.get(i).copied())
        .collect();
    Action::ArrangeCards { top, alt }
}

/// Combinações de `k` índices entre `n`, em ordem lexicográfica, até `cap`.
fn combinations(n: usize, k: usize, cap: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    if k > n || cap == 0 {
        return out;
    }
    let mut idx: Vec<usize> = (0..k).collect();
    loop {
        out.push(idx.clone());
        if out.len() >= cap {
            return out;
        }
        // Posição mais à direita que ainda pode avançar sem estourar o limite.
        let Some(pos) = (0..k).rev().find(|i| idx[*i] != *i + n - k) else {
            return out;
        };
        idx[pos] += 1;
        for j in (pos + 1)..k {
            idx[j] = idx[j - 1] + 1;
        }
    }
}

/// Permutações de `items`, começando pela identidade, até `cap`.
fn permutations(items: &[usize], cap: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut current = items.to_vec();
    permute_into(&mut current, 0, cap, &mut out);
    out
}

fn permute_into(current: &mut Vec<usize>, start: usize, cap: usize, out: &mut Vec<Vec<usize>>) {
    if out.len() >= cap {
        return;
    }
    if start >= current.len() {
        out.push(current.clone());
        return;
    }
    for i in start..current.len() {
        current.swap(start, i);
        permute_into(current, start + 1, cap, out);
        current.swap(start, i);
        if out.len() >= cap {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Perguntas ao agente
// ---------------------------------------------------------------------------

fn ask_selection(
    game: &mut Game,
    player: PlayerId,
    prompt: String,
    candidates: Vec<ObjectId>,
    min: u8,
    max: u8,
) -> Vec<ObjectId> {
    if candidates.is_empty() || max == 0 {
        return Vec::new();
    }
    let fallback: Vec<ObjectId> = candidates.iter().copied().take(min as usize).collect();
    let request =
        Request::SelectObjects { player, prompt, candidates: candidates.clone(), min, max };
    match game.ask(request) {
        // Filtra contra a lista original: agente não escolhe fora do conjunto.
        Action::SelectObjects { objects } => {
            objects.into_iter().filter(|id| candidates.contains(id)).collect()
        }
        _ => fallback,
    }
}

fn ask_confirm(game: &mut Game, player: PlayerId, prompt: String) -> bool {
    matches!(
        game.ask(Request::ConfirmOptional { player, prompt }),
        Action::Confirm { yes: true }
    )
}

fn ask_color(game: &mut Game, player: PlayerId, prompt: String) -> crate::mana::Color {
    match game.ask(Request::ChooseColor { player, prompt }) {
        Action::ChooseColor { color } => color,
        _ => crate::mana::Color::White,
    }
}

// ---------------------------------------------------------------------------
// Dano e vida
// ---------------------------------------------------------------------------

fn deal_damage_to_object(
    game: &mut Game,
    source: Option<ObjectId>,
    target: ObjectId,
    amount: i32,
    kind: DamageKind,
) {
    if amount <= 0 {
        return;
    }
    let Some(chars) = game.characteristics(target) else {
        game.state.push_log(format!("dano ignorado: {target} sem características"), None);
        return;
    };
    // CR 615.1: prevenção transforma o dano em zero antes de qualquer marcação.
    if chars.prevent_all_damage {
        game.state
            .push_log(format!("dano prevenido em {}", chars.name), None);
        return;
    }

    let source_chars = source.and_then(|s| game.characteristics(s));
    let deathtouch = source_chars
        .as_ref()
        .is_some_and(|c| c.has_keyword(&Keyword::Deathtouch));
    let lifelink = source_chars
        .as_ref()
        .is_some_and(|c| c.has_keyword(&Keyword::Lifelink));
    let source_id = source.unwrap_or(ObjectId::NONE);

    let mut lethal = false;
    if chars.is_creature() {
        if let Some(o) = game.state.object_mut(target) {
            o.damage += amount;
            // CR 704.5h: qualquer dano de toque mortal já é letal.
            if deathtouch {
                o.deathtouch_damage = true;
            }
            lethal = deathtouch || chars.remaining_toughness(o.damage) <= 0;
        }
    } else if chars.type_line.has_type(CardType::Planeswalker) {
        // CR 306.8: dano a planeswalker remove marcadores de lealdade.
        if let Some(o) = game.state.object_mut(target) {
            let removed = o.counter(&CounterKind::Loyalty).min(amount);
            o.add_counter(CounterKind::Loyalty, -removed);
            lethal = o.counter(&CounterKind::Loyalty) <= 0;
        }
    } else if chars.type_line.has_type(CardType::Battle) {
        // Batalha usa marcadores de defesa; o vocabulário só tem marcador nomeado.
        if let Some(o) = game.state.object_mut(target) {
            let defense = CounterKind::Named("defense".to_string());
            let removed = o.counter(&defense).min(amount);
            o.add_counter(defense, -removed);
        }
    } else {
        game.state
            .push_log(format!("{} não pode receber dano", chars.name), None);
        return;
    }

    game.state.emit(GameEvent::DamageDealt {
        source: source_id,
        target,
        amount,
        kind,
        deathtouch,
    });
    game.push_event(MatchEvent::DamageDealt { source: source_id, target, amount, lethal });

    if lifelink {
        // CR 702.15b: a vida vai para quem controla a fonte, não para quem a possui.
        if let Some(controller) = source_chars.map(|c| c.controller) {
            gain_life(game, controller, amount);
        }
    }
}

fn deal_damage_to_player(
    game: &mut Game,
    source: Option<ObjectId>,
    player: PlayerId,
    amount: i32,
    kind: DamageKind,
) {
    if amount <= 0 {
        return;
    }
    let source_chars = source.and_then(|s| game.characteristics(s));
    let lifelink = source_chars
        .as_ref()
        .is_some_and(|c| c.has_keyword(&Keyword::Lifelink));
    let source_id = source.unwrap_or(ObjectId::NONE);

    let Some(ps) = game.state.players.get_mut(player.index()) else { return };
    ps.life -= amount;
    ps.life_lost_this_turn += amount;
    ps.damage_taken_this_turn += amount;
    let total = ps.life;

    game.state
        .emit(GameEvent::DamageDealtToPlayer { source: source_id, player, amount, kind });
    // CR 120.3: dano a jogador é também perda de vida — gatilhos dos dois tipos veem.
    game.state.emit(GameEvent::LifeLost { player, amount });
    game.push_event(MatchEvent::DamageToPlayer { source: source_id, player, amount });
    game.push_event(MatchEvent::LifeChanged { player, delta: -amount, total });

    if lifelink {
        if let Some(controller) = source_chars.map(|c| c.controller) {
            gain_life(game, controller, amount);
        }
    }
}

fn gain_life(game: &mut Game, player: PlayerId, amount: i32) {
    if amount <= 0 {
        return;
    }
    let Some(ps) = game.state.players.get_mut(player.index()) else { return };
    ps.life += amount;
    ps.life_gained_this_turn += amount;
    let total = ps.life;
    game.state.emit(GameEvent::LifeGained { player, amount });
    game.push_event(MatchEvent::LifeChanged { player, delta: amount, total });
}

fn lose_life(game: &mut Game, player: PlayerId, amount: i32) {
    if amount <= 0 {
        return;
    }
    let Some(ps) = game.state.players.get_mut(player.index()) else { return };
    ps.life -= amount;
    ps.life_lost_this_turn += amount;
    let total = ps.life;
    game.state.emit(GameEvent::LifeLost { player, amount });
    game.push_event(MatchEvent::LifeChanged { player, delta: -amount, total });
}

/// CR 701.12a: as duas criaturas causam dano igual ao próprio poder ao mesmo
/// tempo, então os dois poderes são lidos antes de qualquer dano ser aplicado.
fn fight(game: &mut Game, a: ObjectId, b: ObjectId) {
    let Some(ca) = game.characteristics(a) else { return };
    let Some(cb) = game.characteristics(b) else { return };
    if !ca.is_creature() || !cb.is_creature() {
        game.state.push_log("luta cancelada: participante não é criatura", None);
        return;
    }
    let (power_a, power_b) = (ca.power, cb.power);
    deal_damage_to_object(game, Some(a), b, power_a, DamageKind::Noncombat);
    deal_damage_to_object(game, Some(b), a, power_b, DamageKind::Noncombat);
}

// ---------------------------------------------------------------------------
// Permanentes
// ---------------------------------------------------------------------------

fn destroy_object(game: &mut Game, id: ObjectId, no_regeneration: bool) {
    let on_battlefield = game.state.object(id).is_some_and(ObjectState::on_battlefield);
    if !on_battlefield {
        return;
    }
    let Some(chars) = game.characteristics(id) else { return };

    // CR 701.7b: indestrutível ignora "destrua" mesmo quando o efeito diz que
    // não pode ser regenerado — as duas coisas são independentes.
    if chars.has_keyword(&Keyword::Indestructible) {
        game.state
            .push_log(format!("{} é indestrutível", chars.name), None);
        return;
    }

    if !no_regeneration {
        let regenerated = match game.state.object_mut(id) {
            Some(o) if o.regeneration_shields > 0 => {
                // CR 701.15a: vira, remove de combate e limpa o dano marcado.
                o.regeneration_shields -= 1;
                o.tapped = true;
                o.damage = 0;
                o.deathtouch_damage = false;
                o.combat.attacking = None;
                o.combat.blocking.clear();
                o.combat.blocked_by.clear();
                o.combat.removed_from_combat = true;
                true
            }
            _ => false,
        };
        if regenerated {
            game.state
                .push_log(format!("{} regenera", chars.name), Some(chars.controller));
            return;
        }
    }

    let Some(owner) = game.state.object(id).map(|o| o.owner) else { return };
    game.push_event(MatchEvent::Destroyed { card: id });
    turn::move_object(game, id, ZoneId::graveyard(owner));
}

fn sacrifice_object(game: &mut Game, id: ObjectId) {
    let Some(obj) = game.state.object(id) else { return };
    if !obj.on_battlefield() {
        return;
    }
    let (owner, controller) = (obj.owner, obj.controller);
    // CR 701.17: sacrifício não é destruição — indestrutível não protege.
    game.state.emit(GameEvent::Sacrificed { object: id, controller });
    game.state
        .push_log(format!("sacrifica {}", game.card_name(id)), Some(controller));
    turn::move_object(game, id, ZoneId::graveyard(owner));
}

fn tap_object(game: &mut Game, id: ObjectId) {
    let should = game
        .state
        .object(id)
        .is_some_and(|o| o.on_battlefield() && !o.tapped);
    if !should {
        return;
    }
    if let Some(o) = game.state.object_mut(id) {
        o.tapped = true;
    }
    game.state.emit(GameEvent::Tapped { object: id });
    game.push_event(MatchEvent::Tapped { card: id });
}

fn untap_object(game: &mut Game, id: ObjectId) {
    let should = game
        .state
        .object(id)
        .is_some_and(|o| o.on_battlefield() && o.tapped);
    if !should {
        return;
    }
    if let Some(o) = game.state.object_mut(id) {
        o.tapped = false;
    }
    game.state.emit(GameEvent::Untapped { object: id });
    game.push_event(MatchEvent::Untapped { card: id });
}

fn attach_object(game: &mut Game, equipment: ObjectId, to: ObjectId) {
    if equipment == to {
        return;
    }
    // CR 701.3c: anexar a outro objeto desanexa do anterior primeiro.
    unattach_object(game, equipment);
    if let Some(o) = game.state.object_mut(equipment) {
        o.attached_to = Some(to);
    }
    if let Some(o) = game.state.object_mut(to) {
        if !o.attachments.contains(&equipment) {
            o.attachments.push(equipment);
        }
    }
    game.state.emit(GameEvent::Attached { equipment, to });
}

fn unattach_object(game: &mut Game, equipment: ObjectId) {
    let Some(previous) = game.state.object(equipment).and_then(|o| o.attached_to) else {
        return;
    };
    if let Some(o) = game.state.object_mut(previous) {
        o.attachments.retain(|x| *x != equipment);
    }
    if let Some(o) = game.state.object_mut(equipment) {
        o.attached_to = None;
    }
    game.state
        .emit(GameEvent::Unattached { equipment, from: previous });
}

/// Cria a ficha e a coloca no campo. O exílio serve de limbo: a ficha precisa
/// existir numa zona antes de `move_object` poder movê-la, e é a única zona
/// compartilhada cuja saída não dispara nada.
fn create_token(game: &mut Game, spec: &TokenSpec, controller: PlayerId) -> Option<ObjectId> {
    let card = game
        .db
        .cards
        .iter()
        .find(|c| c.name == spec.name)
        .map_or(TOKEN_NO_CARD, |c| c.id);

    let id = push_object(game, card, controller, ZoneId::EXILE);
    if let Some(o) = game.state.object_mut(id) {
        o.controller = controller;
        o.is_token = true;
        o.token_spec = Some(Box::new(spec.clone()));
    }
    zone_mut_opt(game, ZoneId::EXILE)?.push_bottom(id);

    turn::move_object(game, id, ZoneId::BATTLEFIELD);
    if let Some(o) = game.state.object_mut(id) {
        o.controller = controller;
    }
    game.push_event(MatchEvent::TokenCreated { card: id, controller });
    game.state
        .push_log(format!("cria ficha {}", spec.name), Some(controller));
    Some(id)
}

/// Reserva um id novo e mantém `state.objects` indexado pela posição — o
/// `IdGen` é compartilhado com os ids de efeito, então pode haver buracos.
fn push_object(game: &mut Game, card: CardDefId, owner: PlayerId, zone: ZoneId) -> ObjectId {
    let id = game.state.id_gen.next_object();
    let timestamp = game.state.next_timestamp();
    let turn = game.state.turn;

    let index = id.0 as usize;
    while game.state.objects.len() < index {
        let filler = ObjectId(game.state.objects.len() as u32);
        game.state
            .objects
            .push(ObjectState::new(filler, card, owner, ZoneId::EXILE, 0));
    }

    let mut object = ObjectState::new(id, card, owner, zone, timestamp);
    object.entered_turn = turn;
    if index < game.state.objects.len() {
        game.state.objects[index] = object;
    } else {
        game.state.objects.push(object);
    }
    id
}

// ---------------------------------------------------------------------------
// Cartas e biblioteca
// ---------------------------------------------------------------------------

fn discard_card(game: &mut Game, player: PlayerId, id: ObjectId) {
    let Some(owner) = game.state.object(id).map(|o| o.owner) else { return };
    game.state
        .emit(GameEvent::CardDiscarded { player, object: id });
    game.state
        .push_log(format!("descarta {}", game.card_name(id)), Some(player));
    turn::move_object(game, id, ZoneId::graveyard(owner));
}

fn shuffle_library(game: &mut Game, player: PlayerId) {
    let library = ZoneId::library(player);
    let Some(mut objects) = zone_ref(game, library).map(|z| z.objects.clone()) else {
        return;
    };
    objects.shuffle(&mut game.rng);
    if let Some(zone) = zone_mut_opt(game, library) {
        zone.objects = objects;
    }
}

fn put_into_library(game: &mut Game, id: ObjectId, on_top: bool) {
    let Some(owner) = game.state.object(id).map(|o| o.owner) else { return };
    let library = ZoneId::library(owner);
    turn::move_object(game, id, library);
    // `move_object` não sabe de topo/fundo; reposiciona depois da mudança de zona.
    if let Some(zone) = zone_mut_opt(game, library) {
        zone.remove(id);
        if on_top {
            zone.push_top(id);
        } else {
            zone.push_bottom(id);
        }
    }
}

/// Scry (CR 701.18) e Surveil (CR 701.43) só diferem no destino alternativo.
fn arrange_top_of_library(game: &mut Game, player: PlayerId, count: usize, to_graveyard: bool) {
    if count == 0 {
        return;
    }
    let library = ZoneId::library(player);
    let Some(cards) = zone_ref(game, library)
        .map(|z| z.objects.iter().take(count).copied().collect::<Vec<_>>())
    else {
        return;
    };
    if cards.is_empty() {
        return;
    }

    let alt_label = if to_graveyard { "cemitério" } else { "fundo" };
    let request = Request::ArrangeCards {
        player,
        prompt: format!("{} {}", if to_graveyard { "surveil" } else { "scry" }, cards.len()),
        cards: cards.clone(),
        alt_label: alt_label.to_string(),
    };
    let (mut top, mut alt) = match game.ask(request) {
        Action::ArrangeCards { top, alt } => (top, alt),
        _ => (cards.clone(), Vec::new()),
    };

    // Saneamento: só ids do conjunto original, sem repetição, sem sumiço.
    top.retain(|id| cards.contains(id));
    top.dedup();
    alt.retain(|id| cards.contains(id) && !top.contains(id));
    alt.dedup();
    for id in &cards {
        if !top.contains(id) && !alt.contains(id) {
            top.push(*id);
        }
    }

    if let Some(zone) = zone_mut_opt(game, library) {
        for id in &cards {
            zone.remove(*id);
        }
        // Insere de trás para frente: `push_top` empilha no índice 0.
        for id in top.iter().rev() {
            zone.push_top(*id);
        }
        if !to_graveyard {
            for id in &alt {
                zone.push_bottom(*id);
            }
        }
    }

    if to_graveyard {
        for id in alt.iter().copied() {
            turn::move_object(game, id, ZoneId::graveyard(player));
            game.state
                .emit(GameEvent::CardMilled { player, object: id });
        }
    }

    game.state.push_log(
        format!("{} {}: {} no topo, {} para o {}", if to_graveyard { "surveil" } else { "scry" },
            cards.len(), top.len(), alt.len(), alt_label),
        Some(player),
    );
}

// ---------------------------------------------------------------------------
// Pilha
// ---------------------------------------------------------------------------

fn counter_spell(game: &mut Game, ctx: &EvalCtx, target: ObjectId, unless_pays: Option<&Cost>) {
    // O alvo pode ter sido dado como id do item de pilha ou como id do objeto-carta.
    let found = game
        .state
        .stack
        .iter()
        .find(|item| item.id == target || item.kind.source() == target)
        .map(|item| (item.id, item.controller));
    let Some((stack_id, owner)) = found else {
        game.state
            .push_log("alvo não está mais na pilha", Some(ctx.controller));
        return;
    };

    if let Some(cost) = unless_pays {
        if cast::can_pay(game, owner, cost) {
            let prompt = "pagar para evitar que sua magia seja anulada?".to_string();
            if ask_confirm(game, owner, prompt) {
                match cast::pay_cost(game, owner, cost, &[]) {
                    Ok(()) => {
                        game.state.push_log("custo pago: não é anulada", Some(owner));
                        return;
                    }
                    Err(err) => {
                        game.state
                            .push_log(format!("pagamento falhou ({err}): será anulada"), Some(owner));
                    }
                }
            }
        }
    }

    stack::counter_item(game, stack_id);
}

/// Copia um item da pilha (CR 707.10). A cópia usa os mesmos alvos, X e modos.
fn copy_stack_item(game: &mut Game, target: ObjectId, controller: PlayerId) -> Option<ObjectId> {
    let source = game
        .state
        .stack
        .iter()
        .find(|item| item.id == target || item.kind.source() == target)?;

    let original = source.kind.source();
    let mut copy = source.clone();
    copy.kind = crate::state::StackItemKind::CopiedSpell { original, source_card: source.card };
    copy.controller = controller;

    // A cópia é um objeto novo: precisa de id próprio para poder ser alvo.
    let owner = source.controller;
    let card = source.card;
    let id = push_object(game, card, owner, ZoneId::STACK);
    copy.id = id;
    game.state.stack.push(copy);
    game.state
        .push_log(format!("copia {}", game.card_name(original)), Some(controller));
    Some(id)
}

// ---------------------------------------------------------------------------
// Mana
// ---------------------------------------------------------------------------

fn add_symbol_to_pool(game: &mut Game, player: PlayerId, symbol: ManaSymbol) {
    let Some(ps) = game.state.players.get_mut(player.index()) else { return };
    match symbol {
        ManaSymbol::Colored(c) | ManaSymbol::MonoHybrid(c) | ManaSymbol::Phyrexian(c) => {
            ps.mana_pool.add(Some(c), 1)
        }
        // Híbrido em produção fixa é raro; a primeira cor é escolha estável.
        ManaSymbol::Hybrid(a, _) => ps.mana_pool.add(Some(a), 1),
        ManaSymbol::Generic(n) => ps.mana_pool.add(None, n as u16),
        ManaSymbol::Colorless | ManaSymbol::Snow => ps.mana_pool.add(None, 1),
        // X não produz nada por si: quem sabe o valor é quem montou a lista.
        ManaSymbol::X => {}
    }
}

// ---------------------------------------------------------------------------
// Efeitos contínuos
// ---------------------------------------------------------------------------

/// CR 611.2c: efeito contínuo criado por resolução trava seus valores agora.
fn add_continuous(
    game: &mut Game,
    ctx: &EvalCtx,
    affected: Vec<ObjectId>,
    modification: StaticModRuntime,
    duration: Duration,
) {
    if affected.is_empty() {
        return;
    }
    if duration == Duration::Instant {
        game.state
            .push_log("modificação sem duração ignorada", Some(ctx.controller));
        return;
    }
    let id = game.state.next_effect_id;
    game.state.next_effect_id = game.state.next_effect_id.wrapping_add(1);
    let timestamp = game.state.next_timestamp();
    let created_turn = game.state.turn;
    game.state.continuous.push(ContinuousEffect {
        id,
        source: ctx.source.unwrap_or(ObjectId::NONE),
        affected,
        modification,
        duration,
        timestamp,
        created_turn,
        controller: ctx.controller,
    });
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn zone_key(z: ZoneId) -> (crate::zone::ZoneKind, u8) {
    (z.kind, z.owner.map_or(u8::MAX, |p| p.0))
}

/// Acesso a zona sem `expect` — zona ausente é bug de setup, não de resolução.
fn zone_ref(game: &Game, z: ZoneId) -> Option<&Zone> {
    game.state.zones.get(&zone_key(z))
}

fn zone_mut_opt(game: &mut Game, z: ZoneId) -> Option<&mut Zone> {
    game.state.zones.get_mut(&zone_key(z))
}

fn objects_in_zone(game: &Game, z: ZoneId) -> Vec<ObjectId> {
    zone_ref(game, z).map(|zone| zone.objects.clone()).unwrap_or_default()
}

fn clamp_u8(n: usize) -> u8 {
    n.min(u8::MAX as usize) as u8
}

fn counter_label(kind: &CounterKind) -> String {
    match kind {
        CounterKind::PlusOnePlusOne => "+1/+1".to_string(),
        CounterKind::MinusOneMinusOne => "-1/-1".to_string(),
        CounterKind::Named(name) => name.clone(),
        other => format!("{other:?}"),
    }
}

/// Guarda o objeto para `ObjRef::Remembered` dos efeitos seguintes da sequência.
fn remember(ctx: &mut EvalCtx, id: ObjectId) {
    if ctx.remembered.len() >= MAX_REMEMBERED {
        return;
    }
    if !ctx.remembered.contains(&id) {
        ctx.remembered.push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinacoes_sao_lexicograficas_e_completas() {
        let all = combinations(4, 2, 100);
        assert_eq!(all.len(), 6);
        assert_eq!(all[0], vec![0, 1]);
        assert_eq!(all[5], vec![2, 3]);
    }

    #[test]
    fn combinacao_vazia_existe_uma_vez() {
        assert_eq!(combinations(5, 0, 100), vec![Vec::<usize>::new()]);
    }

    #[test]
    fn selecao_respeita_min_max_e_teto() {
        let ids: Vec<ObjectId> = (0..8).map(ObjectId).collect();
        let opts = selection_options(&ids, 1, 3);
        assert!(opts.len() <= MAX_CHOICE_OPTIONS);
        assert!(opts.iter().all(|a| match a {
            Action::SelectObjects { objects } => (1..=3).contains(&objects.len()),
            _ => false,
        }));
    }

    #[test]
    fn selecao_sem_candidatos_devolve_opcao_vazia() {
        assert_eq!(selection_options(&[], 1, 2).len(), 1);
    }

    #[test]
    fn modos_enumeram_combinacoes() {
        assert_eq!(mode_options(3, 1).len(), 3);
        assert_eq!(mode_options(3, 2).len(), 3);
    }

    #[test]
    fn arranjo_cobre_todo_tamanho_de_topo() {
        let ids: Vec<ObjectId> = (0..3).map(ObjectId).collect();
        let opts = arrange_options(&ids);
        assert!(opts.len() <= MAX_ARRANGE_OPTIONS);
        for k in 0..=3 {
            assert!(opts.iter().any(|a| matches!(a, Action::ArrangeCards { top, .. } if top.len() == k)));
        }
    }

    #[test]
    fn arranjo_e_deterministico() {
        let ids: Vec<ObjectId> = (0..4).map(ObjectId).collect();
        assert_eq!(arrange_options(&ids), arrange_options(&ids));
    }
}
