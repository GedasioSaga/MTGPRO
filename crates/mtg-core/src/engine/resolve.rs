//! Interpretador do IR de efeitos (CR 608.2).
//!
//! Este módulo é a ponte entre a carta-como-dado (`ir::Effect`) e a mutação de
//! estado. Ele nunca decide *quem* é alvo nem *quanto* é um valor: isso vem de
//! `query`. Aqui só acontece o efeito em si, com os eventos correspondentes.
//!
//! Duas invariantes valem para todo o arquivo:
//!   - variante que não dá para executar direito registra no log e segue;
//!     resolução de mágica não pode derrubar a partida (CR 608.2 — o que não
//!     puder ser feito simplesmente não é feito);
//!   - efeito contínuo criado por resolução trava seus números na hora
//!     (CR 611.2c), por isso `StaticModRuntime` guarda `i32`, não `Value`.
use rand::Rng;

use super::query::{self, EvalCtx};
use super::{cast, layers, stack, turn, Game};
use crate::action::{Action, Request, TargetChoice};
use crate::event::{DamageKind, Defender, GameEvent, LossReason};
use crate::ids::{CardDefId, ObjectId, PlayerId};
use crate::ir::{
    Cost, Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, StaticModRuntime, TokenSpec, Value,
};
use crate::mana::{Color, ManaSymbol};
use crate::state::{ContinuousEffect, ObjectState, StackItem, StackItemKind};
use crate::types::{CardType, CounterKind};
use crate::view::MatchEvent;
use crate::zone::{ZoneId, ZoneKind};

/// Chave das zonas compartilhadas em `state.zones` (espelha `state.rs`).
const SHARED_ZONE_KEY: u8 = u8::MAX;
/// Teto de opções enumeradas numa seleção. Acima disso a árvore de decisão do
/// bot explode sem ganho tático.
const MAX_SELECTION_OPTIONS: usize = 60;
/// Teto de arranjos oferecidos num scry/surveil.
const MAX_ARRANGE_OPTIONS: usize = 24;
/// Teto de combinações de modos.
const MAX_MODE_OPTIONS: usize = 60;
/// Trava contra `Repeat`/`CreateToken` com valor absurdo vindo de dado ruim.
const REPEAT_GUARD: i32 = 100;
/// Teto de fichas criadas por uma única resolução.
const MAX_TOKENS: i32 = 100;
/// Teto de cópias de mágica por resolução.
const MAX_COPIES: i32 = 20;
/// Teto de mana produzido por `AddManaAnyColor` numa resolução.
const MAX_ANY_COLOR_MANA: i32 = 20;

// ---------------------------------------------------------------------------
// Despacho
// ---------------------------------------------------------------------------

/// Executa um efeito do IR. Ponto de entrada único: `stack::resolve_top`,
/// `cast` (custos adicionais) e o próprio interpretador recursam por aqui.
pub fn resolve_effect(game: &mut Game, effect: &Effect, ctx: &mut EvalCtx) {
    // CR 104.1 — partida acabada não resolve mais nada.
    if game.state.is_over() {
        return;
    }
    match effect {
        Effect::Nothing => {}
        Effect::Sequence(list) => {
            for step in list {
                if game.state.is_over() {
                    return;
                }
                resolve_effect(game, step, ctx);
            }
        }

        // --- dano e vida -------------------------------------------------
        Effect::DealDamage { amount, target } => {
            let n = value_of(game, amount, ctx);
            if n <= 0 {
                return;
            }
            for obj in objects_of(game, target, ctx) {
                deal_damage_to_object(game, ctx.source, obj, n, DamageKind::Noncombat);
            }
        }
        Effect::DealDamageToPlayer { amount, player } => {
            let n = value_of(game, amount, ctx);
            if n <= 0 {
                return;
            }
            for p in players_of(game, player, ctx) {
                deal_damage_to_player(game, ctx.source, p, n, DamageKind::Noncombat);
            }
        }
        Effect::DivideDamage { total, targets } => divide_damage(game, total, targets, ctx),
        Effect::GainLife { amount, player } => {
            let n = value_of(game, amount, ctx);
            for p in players_of(game, player, ctx) {
                gain_life(game, p, n);
            }
        }
        Effect::LoseLife { amount, player } => {
            let n = value_of(game, amount, ctx);
            for p in players_of(game, player, ctx) {
                lose_life(game, p, n);
            }
        }
        Effect::SetLife { amount, player } => {
            let n = value_of(game, amount, ctx);
            for p in players_of(game, player, ctx) {
                set_life(game, p, n);
            }
        }

        // --- cartas ------------------------------------------------------
        Effect::DrawCards { count, player } => {
            let n = count_of(game, count, ctx);
            for p in players_of(game, player, ctx) {
                for _ in 0..n {
                    if turn::draw_card(game, p).is_none() {
                        break;
                    }
                }
            }
        }
        Effect::Discard { count, player, filter, random } => {
            let n = count_of(game, count, ctx);
            for p in players_of(game, player, ctx) {
                discard(game, p, n, filter, *random, ctx);
            }
        }
        Effect::Mill { count, player } => {
            let n = count_of(game, count, ctx);
            for p in players_of(game, player, ctx) {
                mill(game, p, n);
            }
        }
        Effect::Scry { count, player } => {
            let n = count_of(game, count, ctx);
            for p in players_of(game, player, ctx) {
                look_at_top(game, p, n, false);
            }
        }
        Effect::Surveil { count, player } => {
            let n = count_of(game, count, ctx);
            for p in players_of(game, player, ctx) {
                look_at_top(game, p, n, true);
            }
        }
        Effect::SearchLibrary { count, filter, player, to_hand } => {
            let n = count_of(game, count, ctx);
            for p in players_of(game, player, ctx) {
                search_library(game, p, n, filter, *to_hand, ctx);
            }
        }
        Effect::ShuffleLibrary { player } => {
            for p in players_of(game, player, ctx) {
                turn::shuffle_library(game, p);
            }
        }
        Effect::PutOnTopOfLibrary { target } => {
            for obj in objects_of(game, target, ctx) {
                if let Some(owner) = owner_of(game, obj) {
                    turn::move_object_top(game, obj, ZoneId::library(owner));
                }
            }
        }
        Effect::PutOnBottomOfLibrary { target } => {
            for obj in objects_of(game, target, ctx) {
                if let Some(owner) = owner_of(game, obj) {
                    turn::move_object(game, obj, ZoneId::library(owner));
                }
            }
        }
        Effect::ReturnToHand { target } => {
            for obj in objects_of(game, target, ctx) {
                if let Some(owner) = owner_of(game, obj) {
                    turn::move_object(game, obj, ZoneId::hand(owner));
                }
            }
        }
        Effect::ReturnFromGraveyardToBattlefield { target } => {
            for obj in objects_of(game, target, ctx) {
                // Só volta o que ainda está no cemitério: alvo que já mudou de
                // zona é outro objeto (CR 400.7).
                let in_graveyard = game
                    .state
                    .object(obj)
                    .is_some_and(|o| o.zone.kind == ZoneKind::Graveyard);
                if in_graveyard {
                    turn::move_object(game, obj, ZoneId::BATTLEFIELD);
                    if let Some(o) = game.state.object_mut(obj) {
                        o.controller = ctx.controller;
                    }
                }
            }
        }

        // --- permanentes -------------------------------------------------
        Effect::Destroy { target, no_regeneration } => {
            for obj in objects_of(game, target, ctx) {
                destroy(game, obj, *no_regeneration);
            }
        }
        Effect::Exile { target, until_source_leaves } => {
            for obj in objects_of(game, target, ctx) {
                turn::move_object(game, obj, ZoneId::EXILE);
            }
            if *until_source_leaves {
                // Exílio com retorno depende de rastrear a saída da fonte, que
                // ainda não existe no estado: o objeto fica exilado.
                game.state.push_log(
                    "exílio \"até a fonte sair\" tratado como exílio permanente",
                    Some(ctx.controller),
                );
            }
        }
        Effect::Sacrifice { player, count, filter } => {
            let n = count_of(game, count, ctx);
            for p in players_of(game, player, ctx) {
                sacrifice(game, p, n, filter, ctx);
            }
        }
        Effect::Tap { target } => {
            for obj in objects_of(game, target, ctx) {
                set_tapped(game, obj, true);
            }
        }
        Effect::Untap { target } => {
            for obj in objects_of(game, target, ctx) {
                set_tapped(game, obj, false);
            }
        }
        Effect::Freeze { target } => {
            for obj in objects_of(game, target, ctx) {
                if let Some(o) = game.state.object_mut(obj) {
                    o.frozen = true;
                }
            }
        }
        Effect::GainControl { target, player, duration } => {
            let Some(new_controller) = players_of(game, player, ctx).first().copied() else {
                game.state
                    .push_log("troca de controle sem jogador definido", Some(ctx.controller));
                return;
            };
            let objs = objects_of(game, target, ctx);
            gain_control(game, &objs, new_controller, *duration, ctx);
        }
        Effect::AttachTo { equipment, target } => {
            let equips = objects_of(game, equipment, ctx);
            let hosts = objects_of(game, target, ctx);
            if let Some(host) = hosts.first().copied() {
                for eq in equips {
                    attach(game, eq, host);
                }
            }
        }
        Effect::Unattach { equipment } => {
            for eq in objects_of(game, equipment, ctx) {
                unattach(game, eq);
            }
        }
        Effect::Transform { target } => {
            for obj in objects_of(game, target, ctx) {
                if let Some(o) = game.state.object_mut(obj) {
                    o.flipped = !o.flipped;
                }
                game.state.emit(GameEvent::Transformed { object: obj });
            }
        }
        Effect::CreateToken { spec, count, controller } => {
            let n = value_of(game, count, ctx).clamp(0, MAX_TOKENS);
            for p in players_of(game, controller, ctx) {
                for _ in 0..n {
                    if let Some(id) = create_token(game, spec, p) {
                        ctx.remembered.push(id);
                    }
                }
            }
        }

        // --- modificações contínuas --------------------------------------
        Effect::ModifyPT { target, power, toughness, duration } => {
            let p = value_of(game, power, ctx);
            let t = value_of(game, toughness, ctx);
            let objs = objects_of(game, target, ctx);
            push_continuous(game, &objs, StaticModRuntime::ModifyPT(p, t), *duration, ctx);
        }
        Effect::SetPT { target, power, toughness, duration } => {
            let p = value_of(game, power, ctx);
            let t = value_of(game, toughness, ctx);
            let objs = objects_of(game, target, ctx);
            push_continuous(game, &objs, StaticModRuntime::SetPT(p, t), *duration, ctx);
        }
        Effect::GrantKeywords { target, keywords, duration } => {
            let objs = objects_of(game, target, ctx);
            let modification = StaticModRuntime::GrantKeywords(keywords.clone());
            push_continuous(game, &objs, modification, *duration, ctx);
        }
        Effect::LoseKeywords { target, keywords, duration } => {
            let objs = objects_of(game, target, ctx);
            let modification = StaticModRuntime::LoseKeywords(keywords.clone());
            push_continuous(game, &objs, modification, *duration, ctx);
        }
        Effect::AddCounters { target, kind, count } => {
            let n = value_of(game, count, ctx);
            for obj in objects_of(game, target, ctx) {
                change_counters(game, obj, kind, n);
            }
        }
        Effect::RemoveCounters { target, kind, count } => {
            let n = value_of(game, count, ctx);
            for obj in objects_of(game, target, ctx) {
                change_counters(game, obj, kind, -n);
            }
        }
        Effect::CantBeBlocked { target, duration } => {
            let objs = objects_of(game, target, ctx);
            push_continuous(game, &objs, StaticModRuntime::CantBeBlocked, *duration, ctx);
        }
        Effect::CantAttackOrBlock { target, duration } => {
            let objs = objects_of(game, target, ctx);
            push_continuous(game, &objs, StaticModRuntime::CantAttackOrBlock, *duration, ctx);
        }

        // --- pilha --------------------------------------------------------
        Effect::CounterSpell { target, unless_pays } => {
            for obj in objects_of(game, target, ctx) {
                counter_spell(game, obj, unless_pays.as_ref(), ctx);
            }
        }
        Effect::CopySpell { target, count, may_choose_new_targets } => {
            let n = value_of(game, count, ctx).clamp(0, MAX_COPIES);
            for obj in objects_of(game, target, ctx) {
                for _ in 0..n {
                    copy_spell(game, obj, *may_choose_new_targets, ctx);
                }
            }
        }

        // --- mana ----------------------------------------------------------
        Effect::AddMana { symbols, player } => {
            for p in players_of(game, player, ctx) {
                for symbol in symbols {
                    add_mana_symbol(game, p, *symbol);
                }
            }
        }
        Effect::AddManaAnyColor { count, player } => {
            let n = value_of(game, count, ctx).clamp(0, MAX_ANY_COLOR_MANA);
            for p in players_of(game, player, ctx) {
                for _ in 0..n {
                    let color = ask_color(game, p, "escolha a cor do mana");
                    if let Some(state) = game.state.players.get_mut(p.index()) {
                        state.mana_pool.add(Some(color), 1);
                    }
                }
            }
        }

        // --- combate --------------------------------------------------------
        Effect::Fight { a, b } => {
            let first = objects_of(game, a, ctx).first().copied();
            let second = objects_of(game, b, ctx).first().copied();
            if let (Some(x), Some(y)) = (first, second) {
                fight(game, x, y);
            }
        }
        Effect::PutOntoBattlefieldAttacking { spec, controller } => {
            for p in players_of(game, controller, ctx) {
                put_token_attacking(game, spec, p);
            }
        }
        Effect::ExtraCombatPhase { player } => {
            for p in players_of(game, player, ctx) {
                // Só o jogador ativo pode ganhar fase extra no turno corrente
                // (CR 500.8) — para os demais o efeito não faz nada.
                if p == game.state.active_player {
                    game.state.extra_combats = game.state.extra_combats.saturating_add(1);
                    game.state.push_log("ganha uma fase de combate extra", Some(p));
                } else {
                    game.state
                        .push_log("fase de combate extra ignorada: não é o jogador ativo", Some(p));
                }
            }
        }
        Effect::ExtraTurn { player } => {
            for p in players_of(game, player, ctx) {
                game.state.extra_turns.push(p);
                game.state.push_log("ganha um turno extra", Some(p));
            }
        }

        // --- controle de fluxo ---------------------------------------------
        Effect::Conditional { cond, then_do, else_do } => {
            if query::eval_condition(game, cond, ctx) {
                resolve_effect(game, then_do, ctx);
            } else if let Some(other) = else_do {
                resolve_effect(game, other, ctx);
            }
        }
        Effect::ForEach { over, do_ } => {
            let ids = query::select(game, over, ctx);
            for_each_ids(game, &ids, do_, ctx);
        }
        Effect::Repeat { times, do_ } => {
            let n = value_of(game, times, ctx).clamp(0, REPEAT_GUARD);
            for _ in 0..n {
                if game.state.is_over() {
                    return;
                }
                resolve_effect(game, do_, ctx);
            }
        }
        Effect::May { do_, prompt } => {
            if ask_confirm(game, ctx.controller, prompt) {
                game.state
                    .push_log(format!("aceitou: {prompt}"), Some(ctx.controller));
                resolve_effect(game, do_, ctx);
            } else {
                game.state
                    .push_log(format!("recusou: {prompt}"), Some(ctx.controller));
            }
        }
        Effect::Modal { choose, options } => resolve_modal(game, *choose, options, ctx),

        // --- fim de jogo ------------------------------------------------------
        Effect::WinGame { player } => {
            for p in players_of(game, player, ctx) {
                // Vencer por efeito = todos os outros perdem (CR 104.2a).
                let others: Vec<PlayerId> = game
                    .state
                    .players
                    .iter()
                    .filter(|x| x.id != p && !x.has_lost)
                    .map(|x| x.id)
                    .collect();
                for other in others {
                    turn::lose_game(game, other, LossReason::Effect);
                }
            }
        }
        Effect::LoseGame { player } => {
            for p in players_of(game, player, ctx) {
                turn::lose_game(game, p, LossReason::Effect);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ponte com `query`
// ---------------------------------------------------------------------------

/// Objetos referenciados. Toda a inteligência de referência mora em `query`;
/// aqui só se consome o resultado.
fn objects_of(game: &Game, r: &ObjRef, ctx: &EvalCtx) -> Vec<ObjectId> {
    query::resolve_objects(game, r, ctx)
}

fn players_of(game: &Game, r: &PlayerRef, ctx: &EvalCtx) -> Vec<PlayerId> {
    query::resolve_players(game, r, ctx)
}

fn value_of(game: &Game, v: &Value, ctx: &EvalCtx) -> i32 {
    query::eval_value(game, v, ctx)
}

/// Valor usado como quantidade de repetições: nunca negativo, sempre limitado.
fn count_of(game: &Game, v: &Value, ctx: &EvalCtx) -> usize {
    value_of(game, v, ctx).clamp(0, REPEAT_GUARD) as usize
}

fn owner_of(game: &Game, obj: ObjectId) -> Option<PlayerId> {
    game.state.object(obj).map(|o| o.owner)
}

fn zone_key(zone: ZoneId) -> (ZoneKind, u8) {
    (zone.kind, zone.owner.map_or(SHARED_ZONE_KEY, |p| p.0))
}

// ---------------------------------------------------------------------------
// Perguntas ao agente
// ---------------------------------------------------------------------------

fn ask_confirm(game: &mut Game, player: PlayerId, prompt: &str) -> bool {
    match game.ask(Request::ConfirmOptional { player, prompt: prompt.to_string() }) {
        Action::Confirm { yes } => yes,
        // Sem resposta válida, o opcional não acontece: é a escolha que nunca
        // muda o estado, então é a única segura.
        _ => false,
    }
}

fn ask_color(game: &mut Game, player: PlayerId, prompt: &str) -> Color {
    match game.ask(Request::ChooseColor { player, prompt: prompt.to_string() }) {
        Action::ChooseColor { color } => color,
        _ => Color::White,
    }
}

/// Seleção de objetos com validação: o agente não consegue devolver algo fora
/// da lista de candidatos nem fora do intervalo pedido.
fn ask_select(
    game: &mut Game,
    player: PlayerId,
    prompt: &str,
    candidates: &[ObjectId],
    min: u8,
    max: u8,
) -> Vec<ObjectId> {
    if candidates.is_empty() || max == 0 {
        return Vec::new();
    }
    let fallback: Vec<ObjectId> = candidates
        .iter()
        .copied()
        .take((min as usize).min(candidates.len()))
        .collect();
    let action = game.ask(Request::SelectObjects {
        player,
        prompt: prompt.to_string(),
        candidates: candidates.to_vec(),
        min,
        max,
    });
    let Action::SelectObjects { objects } = action else {
        return fallback;
    };
    let valid = objects.len() >= min as usize
        && objects.len() <= max as usize
        && objects.iter().all(|o| candidates.contains(o))
        && !has_duplicates(&objects);
    if valid {
        objects
    } else {
        fallback
    }
}

fn has_duplicates(ids: &[ObjectId]) -> bool {
    ids.iter().enumerate().any(|(i, id)| ids[i + 1..].contains(id))
}

/// Scry/surveil: separa em topo e destino alternativo, validando a resposta.
fn ask_arrange(
    game: &mut Game,
    player: PlayerId,
    prompt: &str,
    cards: &[ObjectId],
    alt_label: &str,
) -> (Vec<ObjectId>, Vec<ObjectId>) {
    let action = game.ask(Request::ArrangeCards {
        player,
        prompt: prompt.to_string(),
        cards: cards.to_vec(),
        alt_label: alt_label.to_string(),
    });
    let Action::ArrangeCards { top, alt } = action else {
        return (cards.to_vec(), Vec::new());
    };
    let total = top.len() + alt.len();
    let covers_all = total == cards.len()
        && top.iter().chain(alt.iter()).all(|c| cards.contains(c))
        && !has_duplicates(&top)
        && !has_duplicates(&alt)
        && top.iter().all(|c| !alt.contains(c));
    if covers_all {
        (top, alt)
    } else {
        (cards.to_vec(), Vec::new())
    }
}

// ---------------------------------------------------------------------------
// Dano e vida
// ---------------------------------------------------------------------------

/// Toque mortal e vínculo com a vida da fonte do dano. Fonte inexistente não
/// tem palavra-chave nenhuma.
fn source_damage_flags(game: &Game, source: Option<ObjectId>) -> (bool, bool) {
    let Some(src) = source else { return (false, false) };
    match layers::characteristics(game, src) {
        Some(c) => (
            c.has_keyword(&Keyword::Deathtouch),
            c.has_keyword(&Keyword::Lifelink),
        ),
        None => (false, false),
    }
}

fn deal_damage_to_object(
    game: &mut Game,
    source: Option<ObjectId>,
    target: ObjectId,
    amount: i32,
    kind: DamageKind,
) {
    let (deathtouch, lifelink) = source_damage_flags(game, source);
    damage_object(
        game,
        source.unwrap_or(ObjectId::NONE),
        target,
        amount,
        kind,
        deathtouch,
        lifelink,
    );
}

/// Marca dano num permanente. CR 120.3: dano marcado fica até a limpeza; quem
/// mata é a SBA (CR 704.5g/h), não este código.
fn damage_object(
    game: &mut Game,
    source: ObjectId,
    target: ObjectId,
    amount: i32,
    kind: DamageKind,
    deathtouch: bool,
    lifelink: bool,
) {
    if amount <= 0 {
        return;
    }
    let chars = layers::characteristics(game, target);
    // CR 615.1 — prevenção acontece antes do dano ser marcado.
    if chars.as_ref().is_some_and(|c| c.prevent_all_damage) {
        game.state.push_log(
            format!("dano em {} prevenido", game.card_name(target)),
            None,
        );
        return;
    }
    let is_planeswalker = chars
        .as_ref()
        .is_some_and(|c| c.type_line.has_type(CardType::Planeswalker));

    if is_planeswalker {
        // CR 120.3c — dano em planeswalker remove marcadores de lealdade.
        change_counters(game, target, &CounterKind::Loyalty, -amount);
    } else if let Some(obj) = game.state.object_mut(target) {
        obj.damage += amount;
        if deathtouch {
            // CR 702.2b — qualquer dano de fonte com toque mortal é letal.
            obj.deathtouch_damage = true;
        }
    } else {
        game.state
            .push_log(format!("dano em {target} ignorado: objeto inexistente"), None);
        return;
    }

    let marked = game.state.object(target).map(|o| o.damage).unwrap_or(0);
    let lethal = deathtouch
        || chars
            .as_ref()
            .is_some_and(|c| c.is_creature() && c.toughness > 0 && marked >= c.toughness);

    game.state.emit(GameEvent::DamageDealt {
        source,
        target,
        amount,
        kind,
        deathtouch,
    });
    game.push_event(MatchEvent::DamageDealt { source, target, amount, lethal });

    if lifelink {
        // CR 702.15a — vínculo com a vida dá vida ao controlador da fonte.
        if let Some(controller) = game.state.object(source).map(|o| o.controller) {
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
    let (_, lifelink) = source_damage_flags(game, source);
    let src = source.unwrap_or(ObjectId::NONE);
    let Some(state) = game.state.players.get_mut(player.index()) else { return };
    state.life -= amount;
    state.life_lost_this_turn += amount;
    state.damage_taken_this_turn += amount;
    let total = state.life;

    game.state.emit(GameEvent::DamageDealtToPlayer {
        source: src,
        player,
        amount,
        kind,
    });
    game.state.emit(GameEvent::LifeLost { player, amount });
    game.push_event(MatchEvent::DamageToPlayer { source: src, player, amount });
    game.push_event(MatchEvent::LifeChanged { player, delta: -amount, total });

    if lifelink {
        if let Some(controller) = game.state.object(src).map(|o| o.controller) {
            gain_life(game, controller, amount);
        }
    }
}

fn gain_life(game: &mut Game, player: PlayerId, amount: i32) {
    if amount <= 0 {
        return;
    }
    let Some(state) = game.state.players.get_mut(player.index()) else { return };
    state.life += amount;
    state.life_gained_this_turn += amount;
    let total = state.life;
    game.state.emit(GameEvent::LifeGained { player, amount });
    game.push_event(MatchEvent::LifeChanged { player, delta: amount, total });
}

fn lose_life(game: &mut Game, player: PlayerId, amount: i32) {
    if amount <= 0 {
        return;
    }
    let Some(state) = game.state.players.get_mut(player.index()) else { return };
    state.life -= amount;
    state.life_lost_this_turn += amount;
    let total = state.life;
    game.state.emit(GameEvent::LifeLost { player, amount });
    game.push_event(MatchEvent::LifeChanged { player, delta: -amount, total });
}

fn set_life(game: &mut Game, player: PlayerId, amount: i32) {
    let current = match game.state.players.get(player.index()) {
        Some(p) => p.life,
        None => return,
    };
    // CR 119.3f — definir vida é ganhar ou perder a diferença.
    match amount - current {
        d if d > 0 => gain_life(game, player, d),
        d if d < 0 => lose_life(game, player, -d),
        _ => {}
    }
}

/// Divide um total de dano entre os alvos escolhidos. A divisão é feita o mais
/// uniforme possível e o resto vai aos primeiros alvos — determinístico.
fn divide_damage(game: &mut Game, total: &Value, targets: &[u8], ctx: &mut EvalCtx) {
    let amount = value_of(game, total, ctx);
    if amount <= 0 || targets.is_empty() {
        return;
    }
    let chosen: Vec<TargetChoice> = targets
        .iter()
        .filter_map(|i| ctx.targets.get(*i as usize).copied())
        .collect();
    if chosen.is_empty() {
        game.state
            .push_log("divisão de dano sem alvo válido", Some(ctx.controller));
        return;
    }
    let n = chosen.len() as i32;
    let base = amount / n;
    let rest = amount % n;
    for (i, target) in chosen.iter().enumerate() {
        let share = base + if (i as i32) < rest { 1 } else { 0 };
        if share <= 0 {
            continue;
        }
        match target {
            TargetChoice::Object(o) => {
                deal_damage_to_object(game, ctx.source, *o, share, DamageKind::Noncombat)
            }
            TargetChoice::Player(p) => {
                deal_damage_to_player(game, ctx.source, *p, share, DamageKind::Noncombat)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cartas e zonas
// ---------------------------------------------------------------------------

fn discard(
    game: &mut Game,
    player: PlayerId,
    count: usize,
    filter: &Filter,
    random: bool,
    ctx: &EvalCtx,
) {
    if count == 0 {
        return;
    }
    let hand = turn::zone_objects(game, ZoneId::hand(player));
    let candidates: Vec<ObjectId> = hand
        .into_iter()
        .filter(|o| query::matches_filter(game, *o, filter, ctx))
        .collect();
    if candidates.is_empty() {
        return;
    }
    let n = count.min(candidates.len());
    let picked = if random {
        // CR 701.9 — descarte aleatório usa o gerador do jogo, nunca o do SO.
        let mut pool = candidates;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            if pool.is_empty() {
                break;
            }
            let idx = game.rng.gen_range(0..pool.len());
            out.push(pool.remove(idx));
        }
        out
    } else {
        let cap = n.min(u8::MAX as usize) as u8;
        ask_select(game, player, "descarte cartas", &candidates, cap, cap)
    };
    for card in picked {
        turn::discard_card(game, player, card);
    }
}

fn mill(game: &mut Game, player: PlayerId, count: usize) {
    let library = turn::zone_objects(game, ZoneId::library(player));
    for card in library.into_iter().take(count) {
        turn::move_object(game, card, ZoneId::graveyard(player));
        game.state
            .emit(GameEvent::CardMilled { player, object: card });
    }
}

/// Scry (CR 701.18) e surveil (CR 701.42) só diferem no destino alternativo.
fn look_at_top(game: &mut Game, player: PlayerId, count: usize, to_graveyard: bool) {
    if count == 0 {
        return;
    }
    let cards: Vec<ObjectId> = turn::zone_objects(game, ZoneId::library(player))
        .into_iter()
        .take(count)
        .collect();
    if cards.is_empty() {
        return;
    }
    let (label, prompt) = if to_graveyard {
        ("cemitério", "surveil: escolha o que vai ao cemitério")
    } else {
        ("fundo", "scry: escolha o que vai ao fundo")
    };
    let (top, alt) = ask_arrange(game, player, prompt, &cards, label);

    if to_graveyard {
        for card in &alt {
            turn::move_object(game, *card, ZoneId::graveyard(player));
        }
        reorder_library_top(game, player, &top, &[]);
    } else {
        reorder_library_top(game, player, &top, &alt);
    }
    game.state.push_log(
        format!("olhou {} carta(s) do topo ({label}: {})", cards.len(), alt.len()),
        Some(player),
    );
}

/// Reposiciona cartas dentro da própria biblioteca. Não passa por `move_object`
/// de propósito: não há mudança de zona, então CR 400.7 não se aplica.
fn reorder_library_top(game: &mut Game, player: PlayerId, top: &[ObjectId], bottom: &[ObjectId]) {
    let key = zone_key(ZoneId::library(player));
    let Some(zone) = game.state.zones.get_mut(&key) else { return };
    for card in top.iter().chain(bottom.iter()) {
        zone.remove(*card);
    }
    for card in top.iter().rev() {
        zone.push_top(*card);
    }
    for card in bottom {
        zone.push_bottom(*card);
    }
}

fn search_library(
    game: &mut Game,
    player: PlayerId,
    count: usize,
    filter: &Filter,
    to_hand: bool,
    ctx: &EvalCtx,
) {
    let library = turn::zone_objects(game, ZoneId::library(player));
    let candidates: Vec<ObjectId> = library
        .into_iter()
        .filter(|o| query::matches_filter(game, *o, filter, ctx))
        .collect();
    let cap = count.min(candidates.len()).min(u8::MAX as usize) as u8;
    // Procurar é sempre opcional em quantidade (CR 701.19b): mínimo zero.
    let picked = ask_select(game, player, "procure na biblioteca", &candidates, 0, cap);
    for card in picked {
        let dest = if to_hand {
            ZoneId::hand(player)
        } else {
            ZoneId::BATTLEFIELD
        };
        turn::move_object(game, card, dest);
        if !to_hand {
            if let Some(o) = game.state.object_mut(card) {
                o.controller = player;
            }
        }
    }
    // CR 701.19c — quem procura embaralha depois.
    turn::shuffle_library(game, player);
}

// ---------------------------------------------------------------------------
// Permanentes
// ---------------------------------------------------------------------------

/// CR 701.7 — destruir. Indestrutível ignora; escudo de regeneração substitui.
fn destroy(game: &mut Game, obj: ObjectId, no_regeneration: bool) {
    let on_battlefield = game
        .state
        .object(obj)
        .is_some_and(|o| o.zone.kind == ZoneKind::Battlefield);
    if !on_battlefield {
        return;
    }
    if layers::characteristics(game, obj)
        .is_some_and(|c| c.has_keyword(&Keyword::Indestructible))
    {
        // CR 700.4 — permanente indestrutível não é destruído.
        game.state.push_log(
            format!("{} é indestrutível: destruição ignorada", game.card_name(obj)),
            None,
        );
        return;
    }
    let shields = game
        .state
        .object(obj)
        .map(|o| o.regeneration_shields)
        .unwrap_or(0);
    if !no_regeneration && shields > 0 {
        // CR 701.15a — regenerar substitui a destruição: vira, tira do combate
        // e remove todo o dano marcado.
        if let Some(o) = game.state.object_mut(obj) {
            o.regeneration_shields = shields.saturating_sub(1);
            o.damage = 0;
            o.deathtouch_damage = false;
            o.combat.removed_from_combat = true;
        }
        set_tapped(game, obj, true);
        game.state
            .push_log(format!("{} regenerou", game.card_name(obj)), None);
        return;
    }
    let owner = match owner_of(game, obj) {
        Some(o) => o,
        None => return,
    };
    game.push_event(MatchEvent::Destroyed { card: obj });
    turn::move_object(game, obj, ZoneId::graveyard(owner));
}

fn sacrifice(game: &mut Game, player: PlayerId, count: usize, filter: &Filter, ctx: &EvalCtx) {
    if count == 0 {
        return;
    }
    let candidates: Vec<ObjectId> = game
        .state
        .battlefield()
        .objects
        .clone()
        .into_iter()
        .filter(|o| game.state.object(*o).is_some_and(|s| s.controller == player))
        .filter(|o| query::matches_filter(game, *o, filter, ctx))
        .collect();
    if candidates.is_empty() {
        return;
    }
    let n = count.min(candidates.len()).min(u8::MAX as usize) as u8;
    let picked = ask_select(game, player, "sacrifique", &candidates, n, n);
    for obj in picked {
        let owner = owner_of(game, obj).unwrap_or(player);
        game.state
            .emit(GameEvent::Sacrificed { object: obj, controller: player });
        turn::move_object(game, obj, ZoneId::graveyard(owner));
    }
}

fn set_tapped(game: &mut Game, obj: ObjectId, tapped: bool) {
    let current = match game.state.object(obj) {
        Some(o) => o.tapped,
        None => return,
    };
    if current == tapped {
        return;
    }
    if let Some(o) = game.state.object_mut(obj) {
        o.tapped = tapped;
    }
    if tapped {
        game.state.emit(GameEvent::Tapped { object: obj });
        game.push_event(MatchEvent::Tapped { card: obj });
    } else {
        game.state.emit(GameEvent::Untapped { object: obj });
        game.push_event(MatchEvent::Untapped { card: obj });
    }
}

fn change_counters(game: &mut Game, obj: ObjectId, kind: &CounterKind, delta: i32) {
    if delta == 0 {
        return;
    }
    let Some(state) = game.state.object_mut(obj) else { return };
    let current = state.counter(kind);
    // CR 122.1c — não existe quantidade negativa de marcador: remover mais do
    // que há em cima do objeto só remove o que há.
    let applied = if delta < 0 { -current.min(-delta) } else { delta };
    if applied == 0 {
        return;
    }
    state.add_counter(kind.clone(), applied);
    let event = if applied > 0 {
        GameEvent::CountersAdded { object: obj, kind: kind.clone(), amount: applied }
    } else {
        GameEvent::CountersRemoved { object: obj, kind: kind.clone(), amount: -applied }
    };
    game.state.emit(event);
    game.push_event(MatchEvent::CountersChanged {
        card: obj,
        kind: counter_label(kind),
        delta: applied,
    });
}

fn counter_label(kind: &CounterKind) -> String {
    match kind {
        CounterKind::Named(name) => name.clone(),
        other => format!("{other:?}"),
    }
}

fn gain_control(
    game: &mut Game,
    objects: &[ObjectId],
    new_controller: PlayerId,
    duration: Duration,
    ctx: &EvalCtx,
) {
    if duration == Duration::Instant {
        for obj in objects {
            let from = match game.state.object(*obj) {
                Some(o) => o.controller,
                None => continue,
            };
            if from == new_controller {
                continue;
            }
            if let Some(o) = game.state.object_mut(*obj) {
                o.controller = new_controller;
                // CR 302.6 — quem acabou de ganhar o controle não controlava a
                // criatura desde o começo do turno.
                o.summoning_sick = true;
            }
            game.state.emit(GameEvent::ControlChanged {
                object: *obj,
                from,
                to: new_controller,
            });
        }
        return;
    }
    for obj in objects {
        if let Some(o) = game.state.object_mut(*obj) {
            o.summoning_sick = true;
        }
    }
    push_continuous(
        game,
        objects,
        StaticModRuntime::GainControl(new_controller),
        duration,
        ctx,
    );
}

fn attach(game: &mut Game, equipment: ObjectId, host: ObjectId) {
    if equipment == host {
        return;
    }
    let both_present = game.state.object(equipment).is_some()
        && game
            .state
            .object(host)
            .is_some_and(|o| o.zone.kind == ZoneKind::Battlefield);
    if !both_present {
        return;
    }
    unattach(game, equipment);
    if let Some(eq) = game.state.object_mut(equipment) {
        eq.attached_to = Some(host);
    }
    if let Some(h) = game.state.object_mut(host) {
        if !h.attachments.contains(&equipment) {
            h.attachments.push(equipment);
        }
    }
    game.state
        .emit(GameEvent::Attached { equipment, to: host });
}

fn unattach(game: &mut Game, equipment: ObjectId) {
    let host = match game.state.object(equipment) {
        Some(o) => o.attached_to,
        None => return,
    };
    let Some(host) = host else { return };
    if let Some(eq) = game.state.object_mut(equipment) {
        eq.attached_to = None;
    }
    if let Some(h) = game.state.object_mut(host) {
        h.attachments.retain(|x| *x != equipment);
    }
    game.state
        .emit(GameEvent::Unattached { equipment, from: host });
}

// ---------------------------------------------------------------------------
// Efeitos contínuos (CR 611)
// ---------------------------------------------------------------------------

/// Registra um efeito contínuo já com os números calculados. CR 611.2c: o valor
/// é travado na resolução, não recalculado depois — por isso o parâmetro é
/// `StaticModRuntime` e não `StaticMod`.
fn push_continuous(
    game: &mut Game,
    affected: &[ObjectId],
    modification: StaticModRuntime,
    duration: Duration,
    ctx: &EvalCtx,
) {
    if affected.is_empty() {
        return;
    }
    if duration == Duration::Instant {
        // Modificação sem duração não tem como existir no sistema de camadas.
        game.state.push_log(
            "modificação contínua com duração instantânea ignorada",
            Some(ctx.controller),
        );
        return;
    }
    let id = game.state.next_effect_id;
    game.state.next_effect_id = game.state.next_effect_id.saturating_add(1);
    let timestamp = game.state.next_timestamp();
    let created_turn = game.state.turn;
    game.state.continuous.push(ContinuousEffect {
        id,
        source: ctx.source.unwrap_or(ObjectId::NONE),
        affected: affected.to_vec(),
        modification,
        duration,
        timestamp,
        created_turn,
        controller: ctx.controller,
    });
}

// ---------------------------------------------------------------------------
// Fichas (CR 111)
// ---------------------------------------------------------------------------

/// Reserva um id novo mantendo `objects` indexado por `id.0` — `GameState::object`
/// depende dessa coincidência para ser O(1) sem mapa.
fn alloc_object_id(game: &mut Game) -> ObjectId {
    let id = ObjectId(game.state.objects.len() as u32);
    // Mantém o gerador global à frente: outro subsistema pode consumir ids sem
    // criar `ObjectState` (item de pilha, por exemplo).
    loop {
        if game.state.id_gen.next_object().0 >= id.0 {
            break;
        }
    }
    id
}

/// Cria uma ficha no campo de batalha. A ficha nasce fora de qualquer zona e
/// entra pelo `turn::move_object`, que é quem emite `EnteredBattlefield`.
fn create_token(game: &mut Game, spec: &TokenSpec, controller: PlayerId) -> Option<ObjectId> {
    if game.state.players.get(controller.index()).is_none() {
        return None;
    }
    let id = alloc_object_id(game);
    let timestamp = game.state.next_timestamp();
    let turn_number = game.state.turn;
    // Se o catálogo tiver uma carta com o nome da ficha, ela serve de base para
    // as camadas; senão a ficha vive só do `token_spec`.
    let card = game
        .db
        .id_by_name(&spec.name)
        .unwrap_or(CardDefId(u32::MAX));

    // Nasce no exílio *sem* entrar na lista da zona: é só um ponto de partida
    // neutro para a mudança de zona não emitir "saiu do campo".
    let mut state = ObjectState::new(id, card, controller, ZoneId::EXILE, timestamp);
    state.controller = controller;
    state.is_token = true;
    state.token_spec = Some(Box::new(spec.clone()));
    state.entered_turn = turn_number;
    if id.0 as usize != game.state.objects.len() {
        game.state
            .push_log(format!("id {id} fora de sequência: ficha não criada"), None);
        return None;
    }
    game.state.objects.push(state);

    turn::move_object(game, id, ZoneId::BATTLEFIELD);
    if let Some(o) = game.state.object_mut(id) {
        o.controller = controller;
    }
    game.push_event(MatchEvent::TokenCreated { card: id, controller });
    game.state
        .push_log(format!("criou a ficha {}", spec.name), Some(controller));
    Some(id)
}

fn put_token_attacking(game: &mut Game, spec: &TokenSpec, controller: PlayerId) {
    let Some(id) = create_token(game, spec, controller) else { return };
    let defender = game
        .state
        .opponents(controller)
        .into_iter()
        .find(|p| game.state.players.get(p.index()).is_some_and(|s| !s.has_lost));
    let Some(defender) = defender else { return };
    let target = Defender::Player(defender);
    if let Some(o) = game.state.object_mut(id) {
        o.combat.attacking = Some(target);
    }
    // CR 508.3a — colocada atacando, a ficha nunca foi declarada atacante, então
    // não dispara gatilhos de "ataca".
    game.state
        .emit(GameEvent::Attacked { object: id, defender: target });
}

// ---------------------------------------------------------------------------
// Pilha
// ---------------------------------------------------------------------------

fn counter_spell(game: &mut Game, stack_id: ObjectId, unless_pays: Option<&Cost>, ctx: &EvalCtx) {
    let Some(index) = game.state.stack.iter().position(|it| it.id == stack_id) else {
        game.state.push_log(
            format!("{stack_id} não está mais na pilha: contramágica sem efeito"),
            Some(ctx.controller),
        );
        return;
    };
    if let Some(cost) = unless_pays {
        let owner = game.state.stack[index].controller;
        // CR 118.5 — "a menos que pague": o controlador da mágica escolhe.
        if cast::can_pay(game, owner, cost) && ask_confirm(game, owner, "pagar para não anular") {
            match cast::pay_cost(game, owner, cost, &[]) {
                Ok(()) => {
                    game.state
                        .push_log("pagou o custo e a mágica não foi anulada", Some(owner));
                    return;
                }
                Err(err) => {
                    game.state
                        .push_log(format!("pagamento falhou: {err}"), Some(owner));
                }
            }
        }
    }
    stack::counter_item(game, stack_id);
}

/// Copia uma mágica na pilha (CR 707.10). A cópia não é um objeto-carta: ela
/// existe só como item de pilha.
fn copy_spell(game: &mut Game, stack_id: ObjectId, new_targets: bool, ctx: &EvalCtx) {
    let Some(index) = game.state.stack.iter().position(|it| it.id == stack_id) else {
        return;
    };
    let original = game.state.stack[index].clone();
    let id = alloc_object_id(game);
    let copy = StackItem {
        id,
        kind: StackItemKind::CopiedSpell {
            original: original.kind.source(),
            source_card: original.card,
        },
        controller: ctx.controller,
        card: original.card,
        targets: original.targets.clone(),
        x_value: original.x_value,
        modes: original.modes.clone(),
        trigger_ctx: original.trigger_ctx.clone(),
        remembered: Vec::new(),
        optional_confirmed: original.optional_confirmed,
    };
    // A cópia entra acima do original. A ponta "de cima" de `state.stack` é
    // descoberta em vez de assumida: `stack::peek` é a autoridade.
    let top_is_first = game.state.stack.len() >= 2
        && match stack::peek(game) {
            Some(top) => game.state.stack.first().is_some_and(|f| f.id == top.id),
            None => false,
        };
    if top_is_first {
        game.state.stack.insert(index, copy);
    } else {
        game.state.stack.insert(index + 1, copy);
    }
    if new_targets {
        // Escolher alvos novos exige reenumerar alvos legais da mágica copiada,
        // o que depende da definição da carta: a cópia mantém os alvos.
        game.state.push_log(
            "cópia mantém os alvos originais: escolha de novos alvos não suportada",
            Some(ctx.controller),
        );
    }
    game.state
        .push_log("copiou uma mágica na pilha", Some(ctx.controller));
}

// ---------------------------------------------------------------------------
// Mana
// ---------------------------------------------------------------------------

fn add_mana_symbol(game: &mut Game, player: PlayerId, symbol: ManaSymbol) {
    let Some(state) = game.state.players.get_mut(player.index()) else { return };
    match symbol {
        ManaSymbol::Colored(c) => state.mana_pool.add(Some(c), 1),
        ManaSymbol::Colorless | ManaSymbol::Snow => state.mana_pool.add(None, 1),
        ManaSymbol::Generic(n) => state.mana_pool.add(None, n as u16),
        // Híbrido produzido por efeito: a primeira cor é a escolha determinística.
        ManaSymbol::Hybrid(a, _) => state.mana_pool.add(Some(a), 1),
        ManaSymbol::MonoHybrid(c) | ManaSymbol::Phyrexian(c) => state.mana_pool.add(Some(c), 1),
        ManaSymbol::X => {}
    }
}

// ---------------------------------------------------------------------------
// Combate
// ---------------------------------------------------------------------------

/// CR 701.13 — as duas criaturas causam dano igual ao próprio poder ao mesmo
/// tempo, então os dois poderes são lidos antes de qualquer dano ser marcado.
fn fight(game: &mut Game, a: ObjectId, b: ObjectId) {
    let power_a = layers::characteristics(game, a).map(|c| c.power);
    let power_b = layers::characteristics(game, b).map(|c| c.power);
    let (Some(power_a), Some(power_b)) = (power_a, power_b) else {
        game.state
            .push_log("luta cancelada: criatura sem características", None);
        return;
    };
    if power_a > 0 {
        deal_damage_to_object(game, Some(a), b, power_a, DamageKind::Noncombat);
    }
    if power_b > 0 {
        deal_damage_to_object(game, Some(b), a, power_b, DamageKind::Noncombat);
    }
}

// ---------------------------------------------------------------------------
// Fluxo
// ---------------------------------------------------------------------------

/// Executa o corpo do `ForEach` uma vez por objeto, com `ctx.selected` apontando
/// para o item da volta. O valor anterior é restaurado no fim para que um
/// `ForEach` aninhado não corrompa o de fora.
fn for_each_ids(game: &mut Game, ids: &[ObjectId], body: &Effect, ctx: &mut EvalCtx) {
    let previous = ctx.selected;
    for id in ids {
        if game.state.is_over() {
            break;
        }
        ctx.selected = Some(*id);
        resolve_effect(game, body, ctx);
    }
    ctx.selected = previous;
}

fn resolve_modal(game: &mut Game, choose: u8, options: &[(String, Effect)], ctx: &mut EvalCtx) {
    if options.is_empty() || choose == 0 {
        return;
    }
    let labels: Vec<String> = options.iter().map(|(label, _)| label.clone()).collect();
    let wanted = (choose as usize).min(options.len());
    let action = game.ask(Request::ChooseModes {
        player: ctx.controller,
        prompt: "escolha os modos".to_string(),
        options: labels,
        choose: wanted as u8,
    });
    let mut modes: Vec<u8> = match action {
        Action::ChooseModes { modes } => modes,
        _ => Vec::new(),
    };
    // CR 700.2d — o mesmo modo não pode ser escolhido duas vezes.
    modes.retain(|m| (*m as usize) < options.len());
    modes.dedup();
    if modes.len() != wanted {
        modes = (0..wanted as u8).collect();
    }
    // Modos resolvem na ordem impressa, não na ordem escolhida (CR 601.2b).
    let mut ordered = modes;
    ordered.sort_unstable();
    for index in ordered {
        if game.state.is_over() {
            return;
        }
        if let Some((_, effect)) = options.get(index as usize) {
            resolve_effect(game, effect, ctx);
        }
    }
}

// ---------------------------------------------------------------------------
// Enumeração de opções (consumida por `Game::legal_actions_for`)
// ---------------------------------------------------------------------------

/// Combinações de `choose` modos entre `count` disponíveis, em ordem crescente
/// e determinística.
pub fn mode_options(count: usize, choose: u8) -> Vec<Action> {
    if count == 0 || choose == 0 {
        return vec![Action::ChooseModes { modes: Vec::new() }];
    }
    let k = (choose as usize).min(count);
    let pool: Vec<u8> = (0..count.min(u8::MAX as usize) as u8).collect();
    let mut out: Vec<Vec<u8>> = Vec::new();
    combinations(&pool, k, &mut Vec::new(), 0, &mut out, MAX_MODE_OPTIONS);
    if out.is_empty() {
        out.push(pool.into_iter().take(k).collect());
    }
    out.into_iter()
        .map(|modes| Action::ChooseModes { modes })
        .collect()
}

/// Todos os subconjuntos de tamanho entre `min` e `max`, com teto de opções.
/// A ordem é sempre a mesma para a mesma entrada — determinismo é requisito.
pub fn selection_options(candidates: &[ObjectId], min: u8, max: u8) -> Vec<Action> {
    let len = candidates.len();
    let lo = (min as usize).min(len);
    let hi = (max as usize).min(len);
    if hi == 0 || lo > hi {
        // Impossível atender ao mínimo: a única resposta é a lista inteira, o
        // que mantém o motor com ao menos uma ação legal.
        let objects = candidates.iter().copied().take(hi).collect();
        return vec![Action::SelectObjects { objects }];
    }
    let mut out: Vec<Vec<ObjectId>> = Vec::new();
    for k in lo..=hi {
        if out.len() >= MAX_SELECTION_OPTIONS {
            break;
        }
        combinations(candidates, k, &mut Vec::new(), 0, &mut out, MAX_SELECTION_OPTIONS);
    }
    if out.is_empty() {
        out.push(candidates.iter().copied().take(lo).collect());
    }
    out.into_iter()
        .map(|objects| Action::SelectObjects { objects })
        .collect()
}

/// Particionamentos ordenados de um scry/surveil: cada carta vai para o topo ou
/// para o destino alternativo, preservando a ordem relativa; para topos com duas
/// cartas ou mais a ordem invertida também é oferecida.
pub fn arrange_options(cards: &[ObjectId]) -> Vec<Action> {
    if cards.is_empty() {
        return vec![Action::ArrangeCards { top: Vec::new(), alt: Vec::new() }];
    }
    let n = cards.len().min(16);
    let mut splits: Vec<(Vec<ObjectId>, Vec<ObjectId>)> = Vec::new();
    let total = 1usize << n;
    for mask in 0..total {
        if splits.len() >= MAX_ARRANGE_OPTIONS {
            break;
        }
        let mut top = Vec::new();
        let mut alt = Vec::new();
        for (i, card) in cards.iter().take(n).enumerate() {
            if mask & (1 << i) == 0 {
                top.push(*card);
            } else {
                alt.push(*card);
            }
        }
        splits.push((top, alt));
    }
    let mut out: Vec<Action> = splits
        .iter()
        .map(|(top, alt)| Action::ArrangeCards { top: top.clone(), alt: alt.clone() })
        .collect();
    for (top, alt) in &splits {
        if out.len() >= MAX_ARRANGE_OPTIONS {
            break;
        }
        if top.len() >= 2 {
            let mut reversed = top.clone();
            reversed.reverse();
            out.push(Action::ArrangeCards { top: reversed, alt: alt.clone() });
        }
    }
    out.truncate(MAX_ARRANGE_OPTIONS);
    out
}

/// Combinações de tamanho `k`, em ordem lexicográfica de índice, com teto.
fn combinations<T: Copy>(
    pool: &[T],
    k: usize,
    current: &mut Vec<T>,
    start: usize,
    out: &mut Vec<Vec<T>>,
    cap: usize,
) {
    if out.len() >= cap {
        return;
    }
    if current.len() == k {
        out.push(current.clone());
        return;
    }
    for i in start..pool.len() {
        if pool.len() - i < k - current.len() {
            break;
        }
        current.push(pool[i]);
        combinations(pool, k, current, i + 1, out, cap);
        current.pop();
        if out.len() >= cap {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardDatabase, CardDef};
    use crate::engine::{Agent, GameConfig, PlayerConfig};
    use crate::ids::CardDefId;
    use crate::mana::ManaCost;
    use crate::state::GameOutcome;
    use crate::types::{CardType, Rarity, TypeLine};
    use std::sync::Arc;

    /// Agente que responde de uma fila roteirizada e passa quando ela acaba.
    struct Scripted {
        answers: std::collections::VecDeque<Action>,
    }

    impl Scripted {
        fn new(answers: Vec<Action>) -> Box<dyn Agent> {
            Box::new(Scripted { answers: answers.into() })
        }
    }

    impl Agent for Scripted {
        fn name(&self) -> &str {
            "scripted"
        }
        fn decide(&mut self, _game: &Game, _request: &Request, legal: &[Action]) -> Action {
            self.answers
                .pop_front()
                .unwrap_or_else(|| legal.first().cloned().unwrap_or(Action::PassPriority))
        }
    }

    fn bear() -> CardDef {
        CardDef {
            id: CardDefId(0),
            name: "Grizzly Bears".to_string(),
            mana_cost: ManaCost::FREE,
            type_line: TypeLine {
                supertypes: Vec::new(),
                types: vec![CardType::Creature],
                subtypes: vec!["Bear".to_string()],
            },
            color_override: None,
            power: Some(2),
            toughness: Some(2),
            loyalty: None,
            abilities: Vec::new(),
            spell_effect: None,
            spell_targets: Vec::new(),
            oracle_text: String::new(),
            flavor_text: None,
            rarity: Rarity::Common,
            set_code: "TST".to_string(),
            collector_number: "1".to_string(),
            artist: None,
            art_key: None,
        }
    }

    fn game_with(answers: Vec<Action>) -> Game {
        let mut db = CardDatabase { cards: vec![bear()] };
        db.reindex();
        let deck: Vec<CardDefId> = vec![CardDefId(0); 10];
        let players = vec![
            PlayerConfig { name: "A".to_string(), deck: deck.clone() },
            PlayerConfig { name: "B".to_string(), deck },
        ];
        let agents: Vec<Box<dyn Agent>> = vec![Scripted::new(answers), Scripted::new(Vec::new())];
        let config = GameConfig { allow_mulligan: false, ..GameConfig::default() };
        match Game::new(Arc::new(db), players, agents, config, 42) {
            Ok(g) => g,
            Err(err) => panic!("montagem de teste falhou: {err}"),
        }
    }

    fn ctx_of(game: &Game) -> EvalCtx {
        let source = game.state.objects.first().map(|o| o.id);
        EvalCtx {
            source,
            controller: PlayerId::P0,
            ..EvalCtx::default()
        }
    }

    fn log_count(game: &Game, needle: &str) -> usize {
        game.state
            .log
            .iter()
            .filter(|entry| entry.text.contains(needle))
            .count()
    }

    #[test]
    fn sequencia_com_for_each_roda_o_corpo_uma_vez_por_objeto() {
        let mut game = game_with(vec![
            Action::Confirm { yes: true },
            Action::Confirm { yes: true },
            Action::Confirm { yes: true },
            Action::Confirm { yes: true },
        ]);
        let ids: Vec<ObjectId> = game.state.objects.iter().take(2).map(|o| o.id).collect();
        let mut ctx = ctx_of(&game);
        ctx.selected = None;

        // Sequência de dois `May` por item: quatro confirmações no total.
        let body = Effect::Sequence(vec![
            Effect::May { do_: Box::new(Effect::Nothing), prompt: "um".to_string() },
            Effect::May { do_: Box::new(Effect::Nothing), prompt: "dois".to_string() },
        ]);
        for_each_ids(&mut game, &ids, &body, &mut ctx);

        assert_eq!(log_count(&game, "aceitou: um"), 2);
        assert_eq!(log_count(&game, "aceitou: dois"), 2);
        // `ctx.selected` volta ao valor anterior para não vazar para fora do laço.
        assert_eq!(ctx.selected, None);
    }

    #[test]
    fn modify_pt_cria_efeito_continuo_com_valores_travados() {
        let mut game = game_with(Vec::new());
        let id = match game.state.objects.first() {
            Some(o) => o.id,
            None => panic!("estado de teste sem objetos"),
        };
        let ctx = ctx_of(&game);

        push_continuous(
            &mut game,
            &[id],
            StaticModRuntime::ModifyPT(2, 3),
            Duration::EndOfTurn,
            &ctx,
        );

        assert_eq!(game.state.continuous.len(), 1);
        let effect = match game.state.continuous.first() {
            Some(e) => e,
            None => panic!("efeito contínuo não registrado"),
        };
        // O número está travado no efeito (CR 611.2c), não é um `Value` a reavaliar.
        assert_eq!(effect.modification, StaticModRuntime::ModifyPT(2, 3));
        assert_eq!(effect.affected, vec![id]);
        assert_eq!(effect.duration, Duration::EndOfTurn);
        assert_eq!(effect.created_turn, game.state.turn);
        assert!(effect.timestamp > 0, "efeito contínuo precisa de timestamp");
    }

    #[test]
    fn duracao_instantanea_nao_cria_efeito_continuo() {
        let mut game = game_with(Vec::new());
        let id = match game.state.objects.first() {
            Some(o) => o.id,
            None => panic!("estado de teste sem objetos"),
        };
        let ctx = ctx_of(&game);
        push_continuous(
            &mut game,
            &[id],
            StaticModRuntime::ModifyPT(1, 1),
            Duration::Instant,
            &ctx,
        );
        assert!(game.state.continuous.is_empty());
    }

    #[test]
    fn may_recusado_nao_faz_nada() {
        let mut game = game_with(vec![Action::Confirm { yes: false }]);
        let objects_before = game.state.objects.len();
        let continuous_before = game.state.continuous.len();
        let mut ctx = ctx_of(&game);

        let effect = Effect::May {
            do_: Box::new(Effect::CreateToken {
                spec: TokenSpec {
                    name: "Soldier".to_string(),
                    type_line: TypeLine::default(),
                    colors: Vec::new(),
                    power: 1,
                    toughness: 1,
                    keywords: Vec::new(),
                    art_key: None,
                },
                count: Value::Const(1),
                controller: PlayerRef::You,
            }),
            prompt: "criar ficha".to_string(),
        };
        resolve_effect(&mut game, &effect, &mut ctx);

        assert_eq!(log_count(&game, "recusou: criar ficha"), 1);
        assert_eq!(log_count(&game, "aceitou: criar ficha"), 0);
        assert_eq!(game.state.objects.len(), objects_before);
        assert_eq!(game.state.continuous.len(), continuous_before);
        assert!(game.state.battlefield().is_empty());
    }

    #[test]
    fn dano_com_toque_mortal_marca_a_flag() {
        let mut game = game_with(Vec::new());
        let ids: Vec<ObjectId> = game.state.objects.iter().take(2).map(|o| o.id).collect();
        let (source, target) = match ids.as_slice() {
            [a, b, ..] => (*a, *b),
            _ => panic!("estado de teste com objetos insuficientes"),
        };

        damage_object(
            &mut game,
            source,
            target,
            3,
            DamageKind::Noncombat,
            true,
            false,
        );

        let obj = match game.state.object(target) {
            Some(o) => o,
            None => panic!("alvo sumiu"),
        };
        assert_eq!(obj.damage, 3);
        assert!(obj.deathtouch_damage, "dano de toque mortal precisa marcar a flag");
        assert!(game.state.event_queue.iter().any(|e| matches!(
            e,
            GameEvent::DamageDealt { deathtouch: true, amount: 3, .. }
        )));
        assert!(game
            .match_events
            .iter()
            .any(|e| matches!(e, MatchEvent::DamageDealt { lethal: true, .. })));
    }

    #[test]
    fn dano_sem_toque_mortal_nao_marca_a_flag() {
        let mut game = game_with(Vec::new());
        let ids: Vec<ObjectId> = game.state.objects.iter().take(2).map(|o| o.id).collect();
        let (source, target) = match ids.as_slice() {
            [a, b, ..] => (*a, *b),
            _ => panic!("estado de teste com objetos insuficientes"),
        };
        damage_object(&mut game, source, target, 1, DamageKind::Combat, false, false);
        damage_object(&mut game, source, target, 1, DamageKind::Combat, false, false);
        let obj = match game.state.object(target) {
            Some(o) => o,
            None => panic!("alvo sumiu"),
        };
        // Dano acumula na mesma criatura até a limpeza (CR 120.3).
        assert_eq!(obj.damage, 2);
        assert!(!obj.deathtouch_damage);
    }

    #[test]
    fn vinculo_com_a_vida_da_vida_ao_controlador_da_fonte() {
        let mut game = game_with(Vec::new());
        let ids: Vec<ObjectId> = game.state.objects.iter().take(2).map(|o| o.id).collect();
        let (source, target) = match ids.as_slice() {
            [a, b, ..] => (*a, *b),
            _ => panic!("estado de teste com objetos insuficientes"),
        };
        let before = game.state.player(PlayerId::P0).life;
        damage_object(&mut game, source, target, 4, DamageKind::Noncombat, false, true);
        assert_eq!(game.state.player(PlayerId::P0).life, before + 4);
    }

    #[test]
    fn ficha_entra_no_campo_e_emite_evento() {
        let mut game = game_with(Vec::new());
        let spec = TokenSpec {
            name: "Soldier".to_string(),
            type_line: TypeLine {
                supertypes: Vec::new(),
                types: vec![CardType::Creature],
                subtypes: vec!["Soldier".to_string()],
            },
            colors: Vec::new(),
            power: 1,
            toughness: 1,
            keywords: Vec::new(),
            art_key: None,
        };
        let id = match create_token(&mut game, &spec, PlayerId::P0) {
            Some(id) => id,
            None => panic!("ficha não foi criada"),
        };
        let obj = match game.state.object(id) {
            Some(o) => o,
            None => panic!("ficha sem estado"),
        };
        assert!(obj.is_token);
        assert_eq!(obj.controller, PlayerId::P0);
        assert!(obj.on_battlefield());
        // CR 302.6 — ficha entra com enjoo de invocação como qualquer criatura.
        assert!(obj.summoning_sick);
        assert!(game.state.battlefield().contains(id));
        assert!(game
            .match_events
            .iter()
            .any(|e| matches!(e, MatchEvent::TokenCreated { .. })));
        assert!(game
            .state
            .event_queue
            .iter()
            .any(|e| matches!(e, GameEvent::EnteredBattlefield { .. })));
    }

    #[test]
    fn perder_o_jogo_por_efeito_encerra_a_partida() {
        let mut game = game_with(Vec::new());
        let mut ctx = ctx_of(&game);
        ctx.controller = PlayerId::P0;
        turn::lose_game(&mut game, PlayerId::P1, LossReason::Effect);
        assert_eq!(game.state.outcome, GameOutcome::Winner(PlayerId::P0));
        // Partida encerrada não resolve mais efeito nenhum.
        let before = game.state.log.len();
        resolve_effect(
            &mut game,
            &Effect::May { do_: Box::new(Effect::Nothing), prompt: "tarde".to_string() },
            &mut ctx,
        );
        assert_eq!(game.state.log.len(), before);
    }

    #[test]
    fn contadores_nao_ficam_negativos() {
        let mut game = game_with(Vec::new());
        let id = match game.state.objects.first() {
            Some(o) => o.id,
            None => panic!("estado de teste sem objetos"),
        };
        change_counters(&mut game, id, &CounterKind::PlusOnePlusOne, 2);
        change_counters(&mut game, id, &CounterKind::PlusOnePlusOne, -5);
        let obj = match game.state.object(id) {
            Some(o) => o,
            None => panic!("objeto sumiu"),
        };
        assert_eq!(obj.counter(&CounterKind::PlusOnePlusOne), 0);
    }

    #[test]
    fn mode_options_enumera_combinacoes() {
        let opts = mode_options(3, 1);
        assert_eq!(opts.len(), 3);
        let opts = mode_options(3, 2);
        assert_eq!(opts.len(), 3);
        assert!(opts.contains(&Action::ChooseModes { modes: vec![0, 1] }));
        assert!(opts.contains(&Action::ChooseModes { modes: vec![1, 2] }));
        // Escolher mais modos que o disponível não pode gerar lista vazia.
        assert_eq!(mode_options(2, 5).len(), 1);
        assert_eq!(mode_options(0, 1).len(), 1);
    }

    #[test]
    fn selection_options_respeita_min_e_max() {
        let ids = [ObjectId(1), ObjectId(2), ObjectId(3)];
        let opts = selection_options(&ids, 1, 2);
        // 3 de tamanho 1 + 3 de tamanho 2.
        assert_eq!(opts.len(), 6);
        assert!(opts.contains(&Action::SelectObjects { objects: vec![ObjectId(1)] }));
        assert!(opts.contains(&Action::SelectObjects {
            objects: vec![ObjectId(2), ObjectId(3)]
        }));
        // Determinismo: mesma entrada, mesma saída na mesma ordem.
        assert_eq!(opts, selection_options(&ids, 1, 2));
        assert!(!selection_options(&[], 0, 3).is_empty());
        assert!(selection_options(&ids, 0, 3).len() <= MAX_SELECTION_OPTIONS);
    }

    #[test]
    fn arrange_options_cobre_topo_e_alternativo() {
        let ids = [ObjectId(7), ObjectId(8)];
        let opts = arrange_options(&ids);
        assert!(!opts.is_empty());
        assert!(opts.len() <= MAX_ARRANGE_OPTIONS);
        assert!(opts.contains(&Action::ArrangeCards {
            top: vec![ObjectId(7), ObjectId(8)],
            alt: Vec::new()
        }));
        assert!(opts.contains(&Action::ArrangeCards {
            top: Vec::new(),
            alt: vec![ObjectId(7), ObjectId(8)]
        }));
        // A ordem invertida do topo precisa estar disponível — é o ponto do scry.
        assert!(opts.contains(&Action::ArrangeCards {
            top: vec![ObjectId(8), ObjectId(7)],
            alt: Vec::new()
        }));
        assert_eq!(opts, arrange_options(&ids));
    }
}
