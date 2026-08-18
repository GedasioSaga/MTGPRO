//! Estrutura de turno, prioridade e mudança de zona (CR 103, 117, 400.7, 500–514).
//!
//! Este módulo é o laço principal: monta o estado inicial, distribui mãos com
//! mulligan de Londres, percorre os passos do turno e roda o laço de prioridade.
//! Toda mudança de zona do motor passa por `move_object` — é o único lugar que
//! sabe emitir o par correto de eventos e aplicar CR 400.7.
use std::collections::BTreeMap;

use rand::Rng;

use super::{cast, combat, layers, sba, stack, triggers, Game, GameConfig, PlayerConfig};
use crate::action::{Action, ActionError, Request};
use crate::card::CardDatabase;
use crate::event::{GameEvent, LossReason, Step};
use crate::ids::{IdGen, ObjectId, PlayerId};
use crate::state::{GameOutcome, GameState, ObjectState, PlayerState};
use crate::types::CounterKind;
use crate::view::MatchEvent;
use crate::zone::{Zone, ZoneId, ZoneKind};

/// Chave das zonas compartilhadas em `state.zones` (CR 400.1 — não têm dono).
const SHARED_ZONE_KEY: u8 = u8::MAX;
/// Teto de mulligans: com 7 a mão manida é vazia, então não há por que seguir.
const MAX_MULLIGANS: u8 = 7;
/// Trava contra laço de prioridade que não converge (gatilho que se re-cria).
const PRIORITY_GUARD: u32 = 20_000;
/// CR 514.3a permite limpezas repetidas; um deck patológico poderia não parar.
const CLEANUP_GUARD: u8 = 20;
/// Teto de combinações enumeradas para devolver cartas ao fundo.
const MAX_BOTTOM_OPTIONS: usize = 40;
/// Teto de iterações do descarte por limite de mão.
const DISCARD_GUARD: u8 = 30;

/// Passos que abrem uma fase — só neles vale emitir `PhaseBegan`.
const PHASE_STARTS: [Step; 5] = [
    Step::Untap,
    Step::PrecombatMain,
    Step::BeginCombat,
    Step::PostcombatMain,
    Step::End,
];

// ---------------------------------------------------------------------------
// Montagem
// ---------------------------------------------------------------------------

/// Estado inicial: zonas vazias, bibliotecas populadas, turno 1 no passo de
/// desvirar. Não embaralha nem compra — isso é responsabilidade de `Game::new`.
pub fn initial_state(
    db: &CardDatabase,
    players: &[PlayerConfig],
    config: &GameConfig,
) -> Result<GameState, ActionError> {
    if players.len() < 2 {
        return Err(ActionError::Illegal(
            "uma partida exige ao menos dois jogadores".to_string(),
        ));
    }
    if players.len() >= SHARED_ZONE_KEY as usize {
        return Err(ActionError::Illegal(
            "número de jogadores acima do suportado".to_string(),
        ));
    }

    let mut zones: BTreeMap<(ZoneKind, u8), Zone> = BTreeMap::new();
    for kind in [ZoneKind::Battlefield, ZoneKind::Stack, ZoneKind::Exile] {
        zones.insert((kind, SHARED_ZONE_KEY), Zone::new(kind));
    }

    let mut player_states: Vec<PlayerState> = Vec::with_capacity(players.len());
    let mut objects: Vec<ObjectState> = Vec::new();
    let mut id_gen = IdGen::default();

    for (index, cfg) in players.iter().enumerate() {
        let pid = PlayerId(index as u8);
        for kind in [
            ZoneKind::Library,
            ZoneKind::Hand,
            ZoneKind::Graveyard,
            ZoneKind::Command,
        ] {
            zones.insert((kind, pid.0), Zone::new(kind));
        }
        player_states.push(PlayerState::new(pid, cfg.name.clone(), config.starting_life));

        if cfg.deck.is_empty() {
            return Err(ActionError::Illegal(format!(
                "deck de {} está vazio",
                cfg.name
            )));
        }
        for card in &cfg.deck {
            if db.get(*card).is_none() {
                return Err(ActionError::Illegal(format!(
                    "{card} não existe no catálogo carregado"
                )));
            }
            let id = id_gen.next_object();
            // Índice em `objects` coincide com `ObjectId.0` porque o gerador é
            // monotônico e começa em zero — `GameState::object` conta com isso.
            objects.push(ObjectState::new(id, *card, pid, ZoneId::library(pid), 0));
            if let Some(library) = zones.get_mut(&(ZoneKind::Library, pid.0)) {
                library.push_bottom(id);
            }
        }
    }

    let active = PlayerId(0);
    Ok(GameState {
        players: player_states,
        objects,
        zones,
        stack: Vec::new(),
        continuous: Vec::new(),
        turn: 1,
        active_player: active,
        priority_player: active,
        step: Step::Untap,
        consecutive_passes: 0,
        extra_turns: Vec::new(),
        extra_combats: 0,
        first_strike_done: false,
        outcome: GameOutcome::Ongoing,
        pending: Request::Priority { player: active },
        id_gen,
        timestamp: 0,
        next_effect_id: 0,
        event_queue: Vec::new(),
        pending_triggers: Vec::new(),
        log: Vec::new(),
    })
}

/// Embaralha todas as bibliotecas. Determinístico: mesma semente, mesma ordem.
pub fn shuffle_all_libraries(game: &mut Game) {
    let ids: Vec<PlayerId> = game.state.players.iter().map(|p| p.id).collect();
    for player in ids {
        shuffle_library(game, player);
    }
}

/// Fisher-Yates sobre `game.rng`. Único ponto de aleatoriedade de biblioteca.
pub fn shuffle_library(game: &mut Game, player: PlayerId) {
    let key = (ZoneKind::Library, player.0);
    let mut cards = match game.state.zones.get(&key) {
        Some(zone) => zone.objects.clone(),
        None => return,
    };
    for i in (1..cards.len()).rev() {
        let j = game.rng.gen_range(0..=i);
        cards.swap(i, j);
    }
    if let Some(zone) = game.state.zones.get_mut(&key) {
        zone.objects = cards;
    }
}

// ---------------------------------------------------------------------------
// Mãos iniciais — mulligan de Londres (CR 103.5)
// ---------------------------------------------------------------------------

/// Distribui a mão inicial de cada jogador. Londres: sempre compra a mão cheia
/// e, ao manter com N mulligans, devolve N cartas ao fundo da biblioteca.
pub fn opening_hands(game: &mut Game) {
    let hand_size = game.config.starting_hand_size;
    let players: Vec<PlayerId> = game.state.players.iter().map(|p| p.id).collect();

    for player in players {
        let mut mulligans = 0u8;
        loop {
            deal_hand(game, player, hand_size);
            if !game.config.allow_mulligan || mulligans >= MAX_MULLIGANS {
                break;
            }
            let action = game.ask(Request::Mulligan {
                player,
                mulligans_taken: mulligans,
            });
            if !matches!(action, Action::Mulligan) {
                break;
            }
            return_hand_to_library(game, player);
            shuffle_library(game, player);
            mulligans = mulligans.saturating_add(1);
            game.state.player_mut(player).mulligans_taken = mulligans;
            let name = game.state.player(player).name.clone();
            game.state
                .push_log(format!("{name} toma mulligan #{mulligans}"), Some(player));
        }
        if mulligans > 0 {
            bottom_after_mulligan(game, player, mulligans);
        }
    }
}

/// Compra a mão inicial sem emitir evento de compra: no início do jogo não há
/// permanente em campo, então nenhum gatilho poderia observá-la.
fn deal_hand(game: &mut Game, player: PlayerId, count: usize) {
    for _ in 0..count {
        let top = zone_objects(game, ZoneId::library(player))
            .first()
            .copied();
        let Some(card) = top else {
            game.state
                .push_log("biblioteca acabou ao distribuir a mão inicial", Some(player));
            return;
        };
        move_object(game, card, ZoneId::hand(player));
    }
}

fn return_hand_to_library(game: &mut Game, player: PlayerId) {
    for card in zone_objects(game, ZoneId::hand(player)) {
        move_object(game, card, ZoneId::library(player));
    }
}

fn bottom_after_mulligan(game: &mut Game, player: PlayerId, mulligans: u8) {
    let hand_len = zone_objects(game, ZoneId::hand(player)).len();
    let count = (mulligans as usize).min(hand_len);
    if count == 0 {
        return;
    }
    let action = game.ask(Request::BottomCards {
        player,
        count: count as u8,
    });
    let mut chosen = match action {
        Action::PutOnBottom { objects } => objects,
        _ => Vec::new(),
    };
    if chosen.is_empty() {
        chosen = zone_objects(game, ZoneId::hand(player))
            .into_iter()
            .take(count)
            .collect();
    }
    for card in chosen.into_iter().take(count) {
        if zone_objects(game, ZoneId::hand(player)).contains(&card) {
            move_object(game, card, ZoneId::library(player));
        }
    }
    let name = game.state.player(player).name.clone();
    game.state.push_log(
        format!("{name} devolve {count} carta(s) ao fundo da biblioteca"),
        Some(player),
    );
}

/// Combinações de `count` cartas da mão. Enumerar todas explode em mão grande,
/// então há teto — o agente ainda recebe variedade suficiente para decidir.
pub fn bottom_card_options(game: &Game, player: PlayerId, count: u8) -> Vec<Action> {
    let hand = zone_objects(game, ZoneId::hand(player));
    let k = (count as usize).min(hand.len());
    if k == 0 {
        return vec![Action::PutOnBottom { objects: Vec::new() }];
    }
    let mut out: Vec<Vec<ObjectId>> = Vec::new();
    combinations(&hand, k, &mut Vec::new(), 0, &mut out, MAX_BOTTOM_OPTIONS);
    if out.is_empty() {
        out.push(hand.into_iter().take(k).collect());
    }
    out.into_iter()
        .map(|objects| Action::PutOnBottom { objects })
        .collect()
}

fn combinations(
    pool: &[ObjectId],
    k: usize,
    current: &mut Vec<ObjectId>,
    start: usize,
    out: &mut Vec<Vec<ObjectId>>,
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
// Laço principal
// ---------------------------------------------------------------------------

/// Joga a partida até haver resultado, empate por limite, ou trava de segurança.
pub fn run_game(game: &mut Game) {
    game.state.push_log("partida iniciada", None);
    while !game.state.is_over() {
        // Trava de simulação: empate em vez de laço infinito.
        if game.state.turn > game.config.max_turns {
            force_draw(game, "limite de turnos atingido");
            break;
        }
        run_turn(game);
        if game.state.is_over() {
            break;
        }
        advance_turn(game);
    }
    finish_game(game);
}

fn run_turn(game: &mut Game) {
    begin_turn(game);
    for step in Step::SEQUENCE {
        if game.state.is_over() {
            return;
        }
        match step {
            // A fase de combate roda em bloco próprio porque pode repetir.
            Step::BeginCombat => run_combat_phase(game),
            s if s.is_combat() => {}
            Step::Cleanup => cleanup_step(game),
            s => run_step(game, s),
        }
    }
}

fn begin_turn(game: &mut Game) {
    let active = game.state.active_player;
    let turn = game.state.turn;
    for player in game.state.players.iter_mut() {
        player.reset_turn_counters();
    }
    game.state.extra_combats = 0;
    game.state.first_strike_done = false;
    game.state.emit(GameEvent::TurnBegan {
        player: active,
        turn,
    });
    game.push_event(MatchEvent::TurnStart {
        turn,
        player: active,
    });
    let name = game.state.player(active).name.clone();
    game.state
        .push_log(format!("turno {turn} — {name}"), Some(active));
}

fn advance_turn(game: &mut Game) {
    let ended = game.state.active_player;
    game.state.emit(GameEvent::TurnEnded { player: ended });
    // CR 500.7 — turno extra criado por último é o tomado primeiro.
    let candidate = match game.state.extra_turns.pop() {
        Some(player) => player,
        None => game.state.next_player(ended),
    };
    let next = next_alive(&game.state, candidate).unwrap_or(candidate);
    game.state.active_player = next;
    game.state.turn = game.state.turn.saturating_add(1);
}

fn run_step(game: &mut Game, step: Step) {
    enter_step(game, step);
    step_turn_based_actions(game, step);
    // Gatilhos de começo de passo esperam a próxima prioridade para ir à pilha.
    triggers::fire_step_triggers(game);
    if !step.skips_priority() {
        give_priority(game);
    }
    leave_step(game, step);
}

fn enter_step(game: &mut Game, step: Step) {
    game.state.step = step;
    game.state.consecutive_passes = 0;
    if PHASE_STARTS.contains(&step) {
        game.state.emit(GameEvent::PhaseBegan { phase: step.phase() });
    }
    game.state.emit(GameEvent::StepBegan { step });
    game.push_event(MatchEvent::StepChange {
        step,
        label: step.label().to_string(),
    });
}

fn leave_step(game: &mut Game, step: Step) {
    // CR 511.3 — o combate termina junto com o passo de fim de combate.
    if step == Step::EndCombat {
        combat::end_combat(game);
    }
    empty_mana_pools(game);
    game.state.emit(GameEvent::StepEnded { step });
}

/// Ações baseadas em turno (CR 117.3a): acontecem automaticamente ao começar o
/// passo, antes de qualquer jogador receber prioridade, e não usam a pilha.
fn step_turn_based_actions(game: &mut Game, step: Step) {
    match step {
        Step::Untap => untap_step(game),
        Step::Draw => draw_step(game),
        Step::DeclareAttackers => declare_attackers_step(game),
        Step::DeclareBlockers => declare_blockers_step(game),
        Step::FirstStrikeDamage => combat::combat_damage_step(game, true),
        Step::CombatDamage => combat::combat_damage_step(game, false),
        _ => {}
    }
}

fn run_combat_phase(game: &mut Game) {
    loop {
        game.state.first_strike_done = false;
        run_step(game, Step::BeginCombat);
        if game.state.is_over() {
            return;
        }
        run_step(game, Step::DeclareAttackers);
        if game.state.is_over() {
            return;
        }
        // CR 508.1d — sem atacantes declarados, bloqueio e dano são pulados.
        if has_attackers(game) {
            run_step(game, Step::DeclareBlockers);
            if game.state.is_over() {
                return;
            }
            if combat::has_first_strike_creatures(game) {
                run_step(game, Step::FirstStrikeDamage);
                game.state.first_strike_done = true;
                if game.state.is_over() {
                    return;
                }
            }
            run_step(game, Step::CombatDamage);
            if game.state.is_over() {
                return;
            }
        }
        run_step(game, Step::EndCombat);
        // CR 506.5 — fase de combate adicional repete o bloco inteiro.
        if game.state.is_over() || game.state.extra_combats == 0 {
            return;
        }
        game.state.extra_combats -= 1;
        let active = game.state.active_player;
        game.state.push_log("fase de combate adicional", Some(active));
    }
}

fn has_attackers(game: &Game) -> bool {
    game.state.permanents().any(|o| o.combat.is_attacking())
}

// ---------------------------------------------------------------------------
// Passos com ação própria
// ---------------------------------------------------------------------------

/// CR 502 — desvirar. Sem prioridade, sem pilha.
fn untap_step(game: &mut Game) {
    let active = game.state.active_player;
    let controlled: Vec<ObjectId> = game
        .state
        .permanents_controlled_by(active)
        .map(|o| o.id)
        .collect();

    // CR 302.6 — o enjoo de invocação some no começo do turno do controlador.
    for id in &controlled {
        if let Some(obj) = game.state.object_mut(*id) {
            obj.summoning_sick = false;
        }
    }
    for id in controlled {
        untap_permanent(game, id);
    }
}

fn untap_permanent(game: &mut Game, id: ObjectId) {
    let does_not_untap = layers::characteristics(game, id).is_some_and(|c| c.does_not_untap);
    let mut untapped = false;
    let mut stun_removed = false;

    if let Some(obj) = game.state.object_mut(id) {
        if !obj.tapped {
            return;
        }
        if obj.counter(&CounterKind::Stun) > 0 {
            // CR 701.42a — marcador de atordoamento é gasto no lugar do desvirar.
            obj.add_counter(CounterKind::Stun, -1);
            stun_removed = true;
        } else if obj.frozen {
            // Efeito de congelamento se esgota ao pular um passo de desvirar.
            obj.frozen = false;
        } else if !does_not_untap {
            obj.tapped = false;
            untapped = true;
        }
    }

    if stun_removed {
        game.state.emit(GameEvent::CountersRemoved {
            object: id,
            kind: CounterKind::Stun,
            amount: 1,
        });
        game.push_event(MatchEvent::CountersChanged {
            card: id,
            kind: "Stun".to_string(),
            delta: -1,
        });
    }
    if untapped {
        game.state.emit(GameEvent::Untapped { object: id });
        game.push_event(MatchEvent::Untapped { card: id });
    }
}

/// CR 504.1 — o jogador ativo compra. CR 103.7a: quem começa pula a primeira.
fn draw_step(game: &mut Game) {
    let active = game.state.active_player;
    if game.state.turn == 1 {
        game.state
            .push_log("jogador inicial não compra no primeiro turno", Some(active));
        return;
    }
    draw_card(game, active);
}

/// CR 508 — declaração de atacantes é ação baseada em turno do jogador ativo.
fn declare_attackers_step(game: &mut Game) {
    let active = game.state.active_player;
    let eligible = combat::eligible_attackers(game, active);
    if eligible.is_empty() {
        return;
    }
    let action = game.ask(Request::DeclareAttackers {
        player: active,
        eligible,
    });
    if let Action::Attack { assignments } = action {
        if !assignments.is_empty() {
            combat::declare_attackers(game, &assignments);
        }
    }
}

/// CR 509 — cada jogador defensor declara bloqueadores; depois o atacante
/// ordena os bloqueadores de cada criatura bloqueada por duas ou mais.
fn declare_blockers_step(game: &mut Game) {
    let attackers: Vec<ObjectId> = game
        .state
        .permanents()
        .filter(|o| o.combat.is_attacking())
        .map(|o| o.id)
        .collect();
    if attackers.is_empty() {
        return;
    }
    let active = game.state.active_player;
    let defenders: Vec<PlayerId> = game
        .state
        .players
        .iter()
        .filter(|p| p.id != active && !p.has_lost)
        .map(|p| p.id)
        .collect();

    for defender in defenders {
        let eligible = combat::eligible_blockers(game, defender);
        if eligible.is_empty() {
            continue;
        }
        let action = game.ask(Request::DeclareBlockers {
            player: defender,
            eligible,
            attackers: attackers.clone(),
        });
        if let Action::Block { assignments } = action {
            if !assignments.is_empty() {
                combat::declare_blockers(game, &assignments);
            }
        }
    }
    order_multiple_blockers(game, &attackers);
}

/// CR 509.2 — a ordem de dano entre bloqueadores é escolha do atacante.
fn order_multiple_blockers(game: &mut Game, attackers: &[ObjectId]) {
    for attacker in attackers {
        let (blockers, controller) = match game.state.object(*attacker) {
            Some(o) if o.combat.blocked_by.len() > 1 => (o.combat.blocked_by.clone(), o.controller),
            _ => continue,
        };
        let action = game.ask(Request::OrderBlockers {
            player: controller,
            attacker: *attacker,
            blockers: blockers.clone(),
        });
        let Action::OrderBlockers { attacker: which, order } = action else {
            continue;
        };
        let same_set = which == *attacker
            && order.len() == blockers.len()
            && order.iter().all(|b| blockers.contains(b));
        if !same_set {
            continue;
        }
        if let Some(obj) = game.state.object_mut(*attacker) {
            obj.combat.blocked_by = order;
        }
    }
}

/// CR 514 — limpeza. Descarte até o limite de mão, dano some, efeitos de fim de
/// turno expiram. Se algo dispara ou uma SBA se aplica, os jogadores recebem
/// prioridade e outra limpeza acontece (CR 514.3a).
fn cleanup_step(game: &mut Game) {
    let mut rounds = 0u8;
    loop {
        rounds = rounds.saturating_add(1);
        if rounds > CLEANUP_GUARD {
            force_draw(game, "passo de limpeza não estabilizou");
            return;
        }
        enter_step(game, Step::Cleanup);
        discard_to_hand_size(game);
        clear_damage_and_expiring_effects(game);
        triggers::fire_step_triggers(game);

        let sba_applied = sba::check(game);
        triggers::collect(game);
        let needs_priority = sba_applied || !game.state.pending_triggers.is_empty();

        if !needs_priority || game.state.is_over() {
            leave_step(game, Step::Cleanup);
            return;
        }
        give_priority(game);
        leave_step(game, Step::Cleanup);
        if game.state.is_over() {
            return;
        }
    }
}

fn discard_to_hand_size(game: &mut Game) {
    let active = game.state.active_player;
    let max = game.state.player(active).max_hand_size;
    let mut rounds = 0u8;
    loop {
        rounds = rounds.saturating_add(1);
        if rounds > DISCARD_GUARD {
            return;
        }
        let hand = zone_objects(game, ZoneId::hand(active));
        if hand.len() <= max {
            return;
        }
        let excess = (hand.len() - max).min(u8::MAX as usize);
        let action = game.ask(Request::SelectObjects {
            player: active,
            prompt: format!("Descarte {excess} carta(s): limite de mão"),
            candidates: hand.clone(),
            min: excess as u8,
            max: excess as u8,
        });
        let mut chosen = match action {
            Action::SelectObjects { objects } => objects,
            _ => Vec::new(),
        };
        if chosen.is_empty() {
            chosen = hand.into_iter().take(excess).collect();
        }
        for card in chosen.into_iter().take(excess) {
            discard_card(game, active, card);
        }
    }
}

/// CR 514.2 — dano marcado some e efeitos "até o fim do turno" terminam,
/// tudo simultaneamente e sem usar a pilha.
fn clear_damage_and_expiring_effects(game: &mut Game) {
    for obj in game.state.objects.iter_mut() {
        obj.damage = 0;
        obj.deathtouch_damage = false;
        obj.regeneration_shields = 0;
    }
    layers::expire_continuous_effects(game);
}

// ---------------------------------------------------------------------------
// Prioridade (CR 117)
// ---------------------------------------------------------------------------

/// Laço de prioridade do passo corrente. Sai quando todos passam em sequência
/// com a pilha vazia; com pilha cheia, resolve o topo e recomeça.
pub fn give_priority(game: &mut Game) {
    let mut guard = 0u32;
    game.state.priority_player = game.state.active_player;
    game.state.consecutive_passes = 0;

    loop {
        guard += 1;
        if guard > PRIORITY_GUARD {
            force_draw(game, "laço de prioridade não terminou");
            break;
        }

        // CR 117.5 — antes de qualquer jogador receber prioridade: SBA, depois
        // gatilhos esperando vão para a pilha.
        sba::check_until_stable(game);
        if game.state.is_over() {
            break;
        }
        triggers::collect(game);
        stack::put_triggers_on_stack(game);
        if game.state.is_over() {
            break;
        }

        let alive = alive_count(&game.state);
        if alive <= 1 {
            break;
        }
        let Some(player) = next_alive(&game.state, game.state.priority_player) else {
            break;
        };
        game.state.priority_player = player;

        let action = game.ask(Request::Priority { player });
        if game.state.is_over() {
            break;
        }

        match action {
            Action::PassPriority => {
                game.state.consecutive_passes = game.state.consecutive_passes.saturating_add(1);
                if (game.state.consecutive_passes as usize) < alive {
                    game.state.priority_player = game.state.next_player(player);
                    continue;
                }
                game.state.consecutive_passes = 0;
                // CR 117.4 — todos passaram com pilha vazia: o passo termina.
                if game.state.stack.is_empty() {
                    break;
                }
                stack::resolve_top(game);
                // CR 117.3b — depois de resolver, o jogador ativo recebe prioridade.
                game.state.priority_player = game.state.active_player;
            }
            Action::Concede => lose_game(game, player, LossReason::Concede),
            other => {
                if let Err(err) = cast::execute(game, player, other) {
                    game.state
                        .push_log(format!("ação rejeitada: {err}"), Some(player));
                }
                // CR 117.3c — quem agiu recebe prioridade de novo.
                game.state.consecutive_passes = 0;
                game.state.priority_player = player;
            }
        }
    }

    empty_mana_pools(game);
}

/// CR 500.4 — o pool de mana esvazia no fim de cada passo e fase.
fn empty_mana_pools(game: &mut Game) {
    for player in game.state.players.iter_mut() {
        player.mana_pool.clear();
    }
}

fn alive_count(state: &GameState) -> usize {
    state.players.iter().filter(|p| !p.has_lost).count()
}

fn next_alive(state: &GameState, from: PlayerId) -> Option<PlayerId> {
    let total = state.players.len();
    let mut current = from;
    for _ in 0..total {
        match state.players.get(current.index()) {
            Some(p) if !p.has_lost => return Some(current),
            _ => current = state.next_player(current),
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Mudança de zona (CR 400.7)
// ---------------------------------------------------------------------------

fn zone_key(zone: ZoneId) -> (ZoneKind, u8) {
    (zone.kind, zone.owner.map_or(SHARED_ZONE_KEY, |p| p.0))
}

/// Conteúdo de uma zona, sem risco de pânico se a chave não existir.
pub fn zone_objects(game: &Game, zone: ZoneId) -> Vec<ObjectId> {
    game.state
        .zones
        .get(&zone_key(zone))
        .map(|z| z.objects.clone())
        .unwrap_or_default()
}

/// Move um objeto de zona. Ponto único: só aqui o motor aplica CR 400.7 e emite
/// o par de eventos correto. Destinos ordenados recebem a carta no topo, exceto
/// biblioteca — para o topo da biblioteca use `move_object_top`.
pub fn move_object(game: &mut Game, obj: ObjectId, to: ZoneId) {
    move_object_inner(game, obj, to, false);
}

/// Igual a `move_object`, mas força a posição de topo (topo da biblioteca).
pub fn move_object_top(game: &mut Game, obj: ObjectId, to: ZoneId) {
    move_object_inner(game, obj, to, true);
}

fn move_object_inner(game: &mut Game, obj: ObjectId, to: ZoneId, force_top: bool) {
    let Some(snapshot) = game.state.object(obj) else {
        game.state
            .push_log(format!("{obj} não existe: mudança de zona ignorada"), None);
        return;
    };
    let from = snapshot.zone;
    let owner = snapshot.owner;
    let controller = snapshot.controller;
    let is_token = snapshot.is_token;

    detach_all(game, obj);

    let turn = game.state.turn;
    let timestamp = game.state.next_timestamp();

    if let Some(zone) = game.state.zones.get_mut(&zone_key(from)) {
        zone.remove(obj);
    }
    if let Some(object) = game.state.object_mut(obj) {
        // CR 400.7 — objeto novo: nada do estado anterior sobrevive.
        object.reset_for_zone_change(to, turn, timestamp);
        // CR 108.4 — fora do campo e da pilha, o dono volta a controlar.
        if !matches!(to.kind, ZoneKind::Battlefield | ZoneKind::Stack) {
            object.controller = owner;
        }
    }

    // CR 111.7 — ficha que deixa o campo deixa de existir; some sem entrar em
    // zona alguma, mas os eventos de saída ainda acontecem.
    let ceases_to_exist = is_token && from.kind == ZoneKind::Battlefield;
    if !ceases_to_exist {
        match game.state.zones.get_mut(&zone_key(to)) {
            Some(zone) => {
                let top = force_top
                    || matches!(to.kind, ZoneKind::Stack | ZoneKind::Graveyard);
                if top {
                    zone.push_top(obj);
                } else {
                    zone.push_bottom(obj);
                }
            }
            None => {
                game.state
                    .push_log(format!("zona destino inexistente para {obj}"), Some(owner));
            }
        }
    }

    game.state.emit(GameEvent::ObjectMoved { object: obj, from, to });
    if from.kind == ZoneKind::Battlefield {
        game.state.emit(GameEvent::LeftBattlefield { object: obj, to });
        if to.kind == ZoneKind::Graveyard {
            game.state.emit(GameEvent::Died { object: obj, controller });
            game.push_event(MatchEvent::Died { card: obj });
        }
    }
    if to.kind == ZoneKind::Battlefield {
        game.state
            .emit(GameEvent::EnteredBattlefield { object: obj, controller });
    }
    if to.kind == ZoneKind::Exile {
        game.state.emit(GameEvent::Exiled { object: obj });
        game.push_event(MatchEvent::Exiled { card: obj });
    }

    game.push_event(MatchEvent::CardMoved {
        card: obj,
        from: from.kind,
        to: to.kind,
        owner,
        reveal: !to.kind.is_hidden(),
    });
}

/// Solta auras/equipamentos ligados ao objeto e o solta do que ele equipava —
/// referência pendurada é o jeito mais fácil de corromper o campo de batalha.
fn detach_all(game: &mut Game, obj: ObjectId) {
    let (host, attachments) = match game.state.object(obj) {
        Some(o) => (o.attached_to, o.attachments.clone()),
        None => return,
    };
    if let Some(h) = host {
        if let Some(host_obj) = game.state.object_mut(h) {
            host_obj.attachments.retain(|x| *x != obj);
        }
        game.state
            .emit(GameEvent::Unattached { equipment: obj, from: h });
    }
    for attached in attachments {
        if let Some(a) = game.state.object_mut(attached) {
            a.attached_to = None;
        }
        game.state.emit(GameEvent::Unattached {
            equipment: attached,
            from: obj,
        });
    }
}

// ---------------------------------------------------------------------------
// Cartas
// ---------------------------------------------------------------------------

/// Compra uma carta. Biblioteca vazia não derrota na hora: marca o jogador e
/// deixa a SBA aplicar (CR 704.5b) — o que permite ganhar antes de perder.
pub fn draw_card(game: &mut Game, player: PlayerId) -> Option<ObjectId> {
    let top = zone_objects(game, ZoneId::library(player)).first().copied();
    let Some(card) = top else {
        let p = game.state.player_mut(player);
        if p.loss_reason.is_none() {
            p.loss_reason = Some(LossReason::DrewFromEmptyLibrary);
        }
        game.state
            .push_log("tentou comprar de biblioteca vazia", Some(player));
        return None;
    };

    move_object(game, card, ZoneId::hand(player));
    game.state.player_mut(player).cards_drawn_this_turn += 1;
    game.state.emit(GameEvent::CardDrawn { player, object: card });
    game.push_event(MatchEvent::CardDrawn { card, player });
    Some(card)
}

/// Descarta uma carta específica da mão do jogador.
pub fn discard_card(game: &mut Game, player: PlayerId, card: ObjectId) {
    if !zone_objects(game, ZoneId::hand(player)).contains(&card) {
        return;
    }
    let name = game.card_name(card);
    move_object(game, card, ZoneId::graveyard(player));
    game.state
        .emit(GameEvent::CardDiscarded { player, object: card });
    game.state
        .push_log(format!("descartou {name}"), Some(player));
}

// ---------------------------------------------------------------------------
// Fim de jogo
// ---------------------------------------------------------------------------

/// Tira um jogador da partida e recalcula o resultado.
pub fn lose_game(game: &mut Game, player: PlayerId, reason: LossReason) {
    if game.state.players.get(player.index()).is_none_or(|p| p.has_lost) {
        return;
    }
    if let Some(p) = game.state.players.get_mut(player.index()) {
        p.has_lost = true;
        p.loss_reason = Some(reason);
    }
    let name = game.state.player(player).name.clone();
    let label = loss_label(reason);
    game.state
        .push_log(format!("{name} perdeu a partida: {label}"), Some(player));
    game.state.emit(GameEvent::PlayerLost { player, reason });
    game.push_event(MatchEvent::PlayerLost {
        player,
        reason: label.to_string(),
    });

    if game.state.is_over() {
        return;
    }
    let remaining: Vec<PlayerId> = game
        .state
        .players
        .iter()
        .filter(|p| !p.has_lost)
        .map(|p| p.id)
        .collect();
    match remaining.as_slice() {
        // CR 104.4b — todos saíram ao mesmo tempo: a partida empata.
        [] => {
            game.state.outcome = GameOutcome::Draw;
            game.state.emit(GameEvent::GameDrawn);
        }
        [winner] => {
            game.state.outcome = GameOutcome::Winner(*winner);
            game.state.emit(GameEvent::PlayerWon { player: *winner });
        }
        _ => return,
    }
    let outcome = game.state.outcome;
    game.push_event(MatchEvent::GameOver { outcome });
    game.state.pending = Request::GameOver;
}

/// Encerra a partida em empate — trava de segurança da simulação, não regra.
pub fn force_draw(game: &mut Game, reason: &str) {
    if game.state.is_over() {
        return;
    }
    game.state.outcome = GameOutcome::Draw;
    game.state.push_log(format!("empate: {reason}"), None);
    game.state.emit(GameEvent::GameDrawn);
    game.push_event(MatchEvent::GameOver {
        outcome: GameOutcome::Draw,
    });
    game.state.pending = Request::GameOver;
}

fn finish_game(game: &mut Game) {
    if !game.state.is_over() {
        force_draw(game, "laço de turnos encerrado sem vencedor");
    }
    game.state.pending = Request::GameOver;
    let outcome = game.state.outcome;
    // Empresta os agentes para poder passar `&Game` imutável — mesmo truque de
    // `Game::ask`, sem o qual o borrow checker rejeita a chamada.
    let mut agents = std::mem::take(&mut game.agents);
    for agent in agents.iter_mut() {
        agent.on_game_end(game, outcome);
    }
    game.agents = agents;
}

fn loss_label(reason: LossReason) -> &'static str {
    match reason {
        LossReason::ZeroLife => "vida chegou a zero",
        LossReason::DrewFromEmptyLibrary => "comprou de biblioteca vazia",
        LossReason::PoisonCounters => "dez marcadores de veneno",
        LossReason::Effect => "efeito de carta",
        LossReason::Concede => "desistiu",
    }
}
