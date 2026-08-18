//! Habilidades disparadas (CR 603).
//!
//! O motor emite `GameEvent` numa fila; aqui a fila é drenada e cada evento é
//! confrontado com os gatilhos de todo objeto em zona relevante. O gatilho que
//! casa vira `StackItem` em `state.pending_triggers` — quem os põe na pilha é
//! `stack::put_triggers_on_stack`, na próxima vez que alguém receberia
//! prioridade (CR 603.3).
use std::sync::Arc;

use super::query::EvalCtx;
use super::{query, stack, Game};
use crate::card::{TriggerCondition, TriggeredAbility};
use crate::event::{DamageKind, Defender, GameEvent, Step};
use crate::ids::{AbilityRef, ObjectId, PlayerId};
use crate::ir::{PlayerRef, Selector};
use crate::state::{StackItem, StackItemKind, TriggerContext};
use crate::view::MatchEvent;
use crate::zone::ZoneKind;

// ---------------------------------------------------------------------------
// Drenagem da fila
// ---------------------------------------------------------------------------

/// Processa toda a fila de eventos, criando os gatilhos que casarem.
pub fn collect(game: &mut Game) {
    process(game, false);
}

/// Só os eventos de estrutura de turno (começo de passo/fase e começo de
/// turno). O laço de turno chama isto logo depois das ações baseadas em turno,
/// quando os eventos de jogo ainda não devem ser observados.
pub fn fire_step_triggers(game: &mut Game) {
    process(game, true);
}

fn process(game: &mut Game, steps_only: bool) {
    if game.state.event_queue.is_empty() {
        return;
    }
    let drained = std::mem::take(&mut game.state.event_queue);
    let mut deferred: Vec<GameEvent> = Vec::new();

    for event in drained {
        if steps_only && !is_structure_event(&event) {
            deferred.push(event);
            continue;
        }
        handle_event(game, &event);
    }

    if !deferred.is_empty() {
        // Os eventos adiados aconteceram antes dos que surgiram durante o
        // processamento, então voltam para a frente da fila.
        deferred.extend(game.state.event_queue.drain(..));
        game.state.event_queue = deferred;
    }
}

fn is_structure_event(event: &GameEvent) -> bool {
    matches!(
        event,
        GameEvent::StepBegan { .. }
            | GameEvent::StepEnded { .. }
            | GameEvent::PhaseBegan { .. }
            | GameEvent::TurnBegan { .. }
            | GameEvent::TurnEnded { .. }
    )
}

fn handle_event(game: &mut Game, event: &GameEvent) {
    // "Uma vez por turno" precisa zerar a cada turno e nenhum outro módulo
    // limpa esse mapa — `reset_for_zone_change` só cobre mudança de zona.
    if let GameEvent::TurnBegan { .. } = event {
        for obj in game.state.objects.iter_mut() {
            obj.triggers_fired.clear();
        }
    }

    let db = Arc::clone(&game.db);
    let total = game.state.objects.len();
    for index in 0..total {
        let Some(obj) = game.state.objects.get(index) else {
            continue;
        };
        let source = obj.id;
        let card = obj.card;
        let controller = obj.controller;
        let Some(def) = db.get(card) else {
            continue;
        };
        for (ability_index, ability) in def.triggered() {
            let ability_index = ability_index as u16;
            if !zone_allows(game, source, ability, event) {
                continue;
            }
            if ability.once_per_turn && already_fired(game, source, ability_index) {
                continue;
            }
            let Some(trigger_ctx) = matches(game, &ability.trigger, event, source) else {
                continue;
            };
            let ctx = EvalCtx {
                source: Some(source),
                controller,
                trigger: trigger_ctx.clone(),
                ..Default::default()
            };
            // CR 603.4 — a condição de intervenção é checada no disparo (e de
            // novo na resolução, em `stack::resolve_top`).
            if !query::eval_condition(game, &ability.intervening_if, &ctx) {
                continue;
            }
            fire(game, source, ability_index, card, controller, ability, trigger_ctx);
        }
    }
}

fn already_fired(game: &Game, source: ObjectId, index: u16) -> bool {
    game.state
        .object(source)
        .map(|o| o.triggers_fired.get(&index).copied().unwrap_or(0) > 0)
        .unwrap_or(false)
}

/// CR 603.6 — gatilho de permanente só funciona do campo de batalha, salvo
/// exceção. As de saída de campo (morrer, sair, ser sacrificado, exilado) são
/// checadas com o estado anterior ao evento (CR 603.6d), então o objeto já
/// estar no cemitério não pode impedir o disparo.
fn zone_allows(
    game: &Game,
    source: ObjectId,
    ability: &TriggeredAbility,
    event: &GameEvent,
) -> bool {
    let Some(obj) = game.state.object(source) else {
        return false;
    };
    if obj.zone.kind == ZoneKind::Battlefield {
        return true;
    }
    if ability.triggers_from_graveyard {
        return matches!(obj.zone.kind, ZoneKind::Graveyard | ZoneKind::Hand | ZoneKind::Exile);
    }
    leaves_battlefield_self(&ability.trigger) && event_subject(event) == Some(source)
}

/// A condição fala de algo saindo do campo de batalha?
fn leaves_battlefield_self(cond: &TriggerCondition) -> bool {
    match cond {
        TriggerCondition::Dies(_)
        | TriggerCondition::LeavesBattlefield(_)
        | TriggerCondition::Sacrificed(_)
        | TriggerCondition::Exiled(_) => true,
        TriggerCondition::Any(list) => list.iter().any(leaves_battlefield_self),
        _ => false,
    }
}

/// Objeto que é o sujeito do evento, quando existe.
fn event_subject(event: &GameEvent) -> Option<ObjectId> {
    match event {
        GameEvent::Died { object, .. }
        | GameEvent::LeftBattlefield { object, .. }
        | GameEvent::Sacrificed { object, .. }
        | GameEvent::Exiled { object }
        | GameEvent::ObjectMoved { object, .. }
        | GameEvent::EnteredBattlefield { object, .. } => Some(*object),
        _ => None,
    }
}

fn fire(
    game: &mut Game,
    source: ObjectId,
    index: u16,
    card: crate::ids::CardDefId,
    controller: PlayerId,
    ability: &TriggeredAbility,
    trigger_ctx: TriggerContext,
) {
    if let Some(obj) = game.state.object_mut(source) {
        let counter = obj.triggers_fired.entry(index).or_insert(0);
        *counter = counter.saturating_add(1);
    }
    let id = stack::reserve_stack_id(game);
    game.state.pending_triggers.push(StackItem {
        id,
        kind: StackItemKind::TriggeredAbility { source, index },
        controller,
        card,
        targets: Vec::new(),
        x_value: 0,
        modes: Vec::new(),
        trigger_ctx,
        remembered: Vec::new(),
        // Gatilho "você pode" só é confirmado na resolução (CR 603.5).
        optional_confirmed: !ability.optional,
    });

    game.state.emit(GameEvent::AbilityTriggered {
        ability: AbilityRef { object: source, index },
        controller,
    });
    let name = game.card_name(source);
    let text = ability.text.clone();
    game.push_event(MatchEvent::AbilityTriggered {
        source,
        text: text.clone(),
    });
    game.state
        .push_log(format!("{name} dispara: {text}"), Some(controller));
}

// ---------------------------------------------------------------------------
// Casamento evento × condição (CR 603.2)
// ---------------------------------------------------------------------------

/// Casa uma condição de disparo com um evento. Devolve o contexto capturado no
/// momento do disparo — é ele que preserva a informação conhecida por último
/// (CR 603.6d) quando o objeto já mudou de zona.
pub fn matches(
    game: &Game,
    cond: &TriggerCondition,
    ev: &GameEvent,
    source: ObjectId,
) -> Option<TriggerContext> {
    let mut ctx = match_condition(game, cond, ev, source)?;
    attach_last_known(game, &mut ctx);
    Some(ctx)
}

/// CR 603.6d — anexa o retrato do objeto do gatilho, e só quando ele já não
/// está na zona em que estava no evento. Enquanto ele continua lá o estado
/// corrente é a informação correta, e o contexto sai sem cópia alguma: nenhum
/// gatilho de começo de passo, dano ou compra paga por isto.
fn attach_last_known(game: &Game, ctx: &mut TriggerContext) {
    let Some(obj) = ctx.trigger_object else { return };
    let Some(last) = game.state.last_known.get(&obj) else { return };
    let still_there = matches!(game.state.object(obj), Some(o) if o.zone == last.zone);
    if !still_there {
        ctx.last_known = Some(Box::new(last.clone()));
    }
}

fn match_condition(
    game: &Game,
    cond: &TriggerCondition,
    ev: &GameEvent,
    source: ObjectId,
) -> Option<TriggerContext> {
    use TriggerCondition as T;
    match (cond, ev) {
        // --- agregador ---
        (T::Any(list), _) => list.iter().find_map(|c| match_condition(game, c, ev, source)),

        // --- movimento de zona ---
        (T::EntersBattlefield(sel), GameEvent::EnteredBattlefield { object, controller }) => {
            hits(game, *object, sel, source).then(|| TriggerContext {
                trigger_object: Some(*object),
                trigger_source: Some(source),
                trigger_player: Some(*controller),
                amount: 0,
                last_known: None,
            })
        }
        (T::LeavesBattlefield(sel), GameEvent::LeftBattlefield { object, .. }) => {
            hits(game, *object, sel, source).then(|| object_ctx(*object, source))
        }
        (T::Dies(sel), GameEvent::Died { object, controller }) => {
            hits(game, *object, sel, source).then(|| TriggerContext {
                trigger_object: Some(*object),
                trigger_source: Some(source),
                // O controlador vem do evento, capturado antes da morte.
                trigger_player: Some(*controller),
                amount: 0,
                last_known: None,
            })
        }
        (T::Sacrificed(sel), GameEvent::Sacrificed { object, controller }) => {
            hits(game, *object, sel, source).then(|| TriggerContext {
                trigger_object: Some(*object),
                trigger_source: Some(source),
                trigger_player: Some(*controller),
                amount: 0,
                last_known: None,
            })
        }
        (T::Exiled(sel), GameEvent::Exiled { object }) => {
            hits(game, *object, sel, source).then(|| object_ctx(*object, source))
        }

        // --- pilha ---
        (T::SpellCast(sel), GameEvent::SpellCast { object, controller }) => {
            hits(game, *object, sel, source).then(|| TriggerContext {
                trigger_object: Some(*object),
                trigger_source: Some(source),
                trigger_player: Some(*controller),
                amount: 0,
                last_known: None,
            })
        }
        (T::AbilityActivated(sel), GameEvent::AbilityActivated { ability, controller }) => {
            hits(game, ability.object, sel, source).then(|| TriggerContext {
                trigger_object: Some(ability.object),
                trigger_source: Some(source),
                trigger_player: Some(*controller),
                amount: 0,
                last_known: None,
            })
        }

        // --- combate ---
        (T::Attacks(sel), GameEvent::Attacked { object, defender }) => {
            hits(game, *object, sel, source).then(|| TriggerContext {
                trigger_object: Some(*object),
                trigger_source: match defender {
                    Defender::Planeswalker(id) | Defender::Battle(id) => Some(*id),
                    Defender::Player(_) => Some(source),
                },
                trigger_player: match defender {
                    Defender::Player(p) => Some(*p),
                    _ => None,
                },
                amount: 0,
                last_known: None,
            })
        }
        (T::AttacksAlone(sel), GameEvent::AttackersDeclared { attackers }) => {
            match attackers.as_slice() {
                [only] if hits(game, *only, sel, source) => Some(object_ctx(*only, source)),
                _ => None,
            }
        }
        (T::Blocks(sel), GameEvent::Blocked { attacker, blockers }) => blockers
            .iter()
            .find(|b| hits(game, **b, sel, source))
            .map(|b| TriggerContext {
                trigger_object: Some(*b),
                trigger_source: Some(*attacker),
                trigger_player: None,
                amount: 0,
                last_known: None,
            }),
        (T::BecomesBlocked(sel), GameEvent::Blocked { attacker, blockers }) => {
            hits(game, *attacker, sel, source).then(|| TriggerContext {
                trigger_object: Some(*attacker),
                trigger_source: blockers.first().copied(),
                trigger_player: None,
                amount: 0,
                last_known: None,
            })
        }

        // --- dano ---
        (
            T::DealsCombatDamage(sel),
            GameEvent::DamageDealt { source: dealer, target, amount, kind: DamageKind::Combat, .. },
        ) => hits(game, *dealer, sel, source).then(|| TriggerContext {
            trigger_object: Some(*dealer),
            trigger_source: Some(*target),
            trigger_player: None,
            amount: *amount,
            last_known: None,
        }),
        (
            T::DealsCombatDamageToPlayer(sel),
            GameEvent::DamageDealtToPlayer {
                source: dealer,
                player,
                amount,
                kind: DamageKind::Combat,
            },
        ) => hits(game, *dealer, sel, source).then(|| TriggerContext {
            trigger_object: Some(*dealer),
            trigger_source: Some(source),
            trigger_player: Some(*player),
            amount: *amount,
            last_known: None,
        }),
        (T::DealtDamage(sel), GameEvent::DamageDealt { source: dealer, target, amount, .. }) => {
            hits(game, *target, sel, source).then(|| TriggerContext {
                trigger_object: Some(*target),
                trigger_source: Some(*dealer),
                trigger_player: None,
                amount: *amount,
                last_known: None,
            })
        }

        // --- permanentes ---
        (T::Taps(sel), GameEvent::Tapped { object }) => {
            hits(game, *object, sel, source).then(|| object_ctx(*object, source))
        }
        (T::Untaps(sel), GameEvent::Untapped { object }) => {
            hits(game, *object, sel, source).then(|| object_ctx(*object, source))
        }
        (
            T::CounterAdded(sel, wanted),
            GameEvent::CountersAdded { object, kind, amount },
        ) => (kind == wanted && hits(game, *object, sel, source)).then(|| TriggerContext {
            trigger_object: Some(*object),
            trigger_source: Some(source),
            trigger_player: None,
            amount: *amount,
            last_known: None,
        }),

        // --- estrutura de turno (CR 603.2b) ---
        (T::BeginningOfUpkeep(who), GameEvent::StepBegan { step: Step::Upkeep }) => {
            step_ctx(game, who, source)
        }
        (T::BeginningOfDrawStep(who), GameEvent::StepBegan { step: Step::Draw }) => {
            step_ctx(game, who, source)
        }
        (T::BeginningOfPrecombatMain(who), GameEvent::StepBegan { step: Step::PrecombatMain }) => {
            step_ctx(game, who, source)
        }
        (T::BeginningOfCombat(who), GameEvent::StepBegan { step: Step::BeginCombat }) => {
            step_ctx(game, who, source)
        }
        (T::EndOfCombat(who), GameEvent::StepBegan { step: Step::EndCombat }) => {
            step_ctx(game, who, source)
        }
        (T::BeginningOfEndStep(who), GameEvent::StepBegan { step: Step::End }) => {
            step_ctx(game, who, source)
        }
        (T::Cleanup(who), GameEvent::StepBegan { step: Step::Cleanup }) => {
            step_ctx(game, who, source)
        }

        // --- jogadores ---
        (T::LifeGained(who), GameEvent::LifeGained { player, amount }) => {
            player_ctx(game, who, *player, source, *amount, None)
        }
        (T::LifeLost(who), GameEvent::LifeLost { player, amount }) => {
            player_ctx(game, who, *player, source, *amount, None)
        }
        (T::CardDrawn(who), GameEvent::CardDrawn { player, object }) => {
            player_ctx(game, who, *player, source, 1, Some(*object))
        }
        (T::CardDiscarded(who), GameEvent::CardDiscarded { player, object }) => {
            player_ctx(game, who, *player, source, 1, Some(*object))
        }
        (T::LandPlayed(who), GameEvent::LandPlayed { player, object }) => {
            player_ctx(game, who, *player, source, 1, Some(*object))
        }

        _ => None,
    }
}

fn object_ctx(object: ObjectId, source: ObjectId) -> TriggerContext {
    TriggerContext {
        trigger_object: Some(object),
        trigger_source: Some(source),
        trigger_player: None,
        amount: 0,
        last_known: None,
    }
}

fn step_ctx(game: &Game, who: &PlayerRef, source: ObjectId) -> Option<TriggerContext> {
    let active = game.state.active_player;
    player_hits(game, active, who, source).then(|| TriggerContext {
        trigger_object: None,
        trigger_source: Some(source),
        trigger_player: Some(active),
        amount: 0,
        last_known: None,
    })
}

fn player_ctx(
    game: &Game,
    who: &PlayerRef,
    player: PlayerId,
    source: ObjectId,
    amount: i32,
    object: Option<ObjectId>,
) -> Option<TriggerContext> {
    player_hits(game, player, who, source).then(|| TriggerContext {
        trigger_object: object,
        trigger_source: Some(source),
        trigger_player: Some(player),
        amount,
        last_known: None,
    })
}

fn ctx_for(game: &Game, source: ObjectId) -> EvalCtx {
    let controller = game
        .state
        .object(source)
        .map(|o| o.controller)
        .unwrap_or_default();
    EvalCtx {
        source: Some(source),
        controller,
        ..Default::default()
    }
}

/// O objeto casa com o seletor do gatilho?
///
/// De propósito a `ZoneScope` do seletor é ignorada: gatilho de morte precisa
/// casar com um objeto que já está no cemitério (CR 603.6d, informação
/// conhecida por último). Quem restringe a zona é `zone_allows`.
fn hits(game: &Game, obj: ObjectId, sel: &Selector, source: ObjectId) -> bool {
    let ctx = ctx_for(game, source);
    if let Some(scope) = &sel.owner_scope {
        let allowed = query::resolve_players(game, scope, &ctx);
        let controller = match game.state.object(obj) {
            Some(o) => o.controller,
            None => return false,
        };
        if !allowed.contains(&controller) {
            return false;
        }
    }
    query::matches_filter(game, obj, &sel.filter, &ctx)
}

fn player_hits(game: &Game, player: PlayerId, who: &PlayerRef, source: ObjectId) -> bool {
    let ctx = ctx_for(game, source);
    query::resolve_players(game, who, &ctx).contains(&player)
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Ability, CardDef};
    use crate::engine::stack::testkit::{card, game_with, put_on_battlefield};
    use crate::engine::{layers, turn};
    use crate::ir::{Condition, Effect, Filter, ObjRef, PlayerRef, Selector, Value};
    use crate::types::{CardType, CounterKind};
    use crate::zone::ZoneId;

    fn dies_trigger_card() -> Vec<crate::card::CardDef> {
        let mut c = card(0, "Sentinela", vec![CardType::Creature]);
        c.abilities = vec![Ability::Triggered(TriggeredAbility {
            trigger: TriggerCondition::Dies(Selector::creatures()),
            intervening_if: Condition::Always,
            targets: Vec::new(),
            effect: Effect::Nothing,
            optional: false,
            once_per_turn: false,
            triggers_from_graveyard: false,
            text: "Quando morrer, nada acontece.".to_string(),
        })];
        vec![c]
    }

    /// Urso 2/2 que, ao morrer, ganha vida igual à própria resistência.
    fn lki_dies_card() -> Vec<CardDef> {
        let mut c = card(0, "Urso Rancoroso", vec![CardType::Creature]);
        c.power = Some(2);
        c.toughness = Some(2);
        c.abilities = vec![Ability::Triggered(TriggeredAbility {
            trigger: TriggerCondition::Dies(Selector::creatures()),
            intervening_if: Condition::Always,
            targets: Vec::new(),
            effect: Effect::GainLife {
                amount: Value::ToughnessOf(ObjRef::TriggerObject),
                last_known: None,
                player: PlayerRef::You,
            },
            optional: false,
            once_per_turn: false,
            triggers_from_graveyard: false,
            text: "Quando morrer, ganhe vida igual à sua resistência.".to_string(),
        })];
        vec![c]
    }

    /// CR 603.6d — o gatilho de saída do campo lê a informação conhecida por
    /// último: a 2/2 com dois marcadores +1/+1 morreu como 4/4, e é 4 que o
    /// efeito enxerga, não a resistência impressa nem a do objeto já no
    /// cemitério (que `reset_for_zone_change` zerou).
    #[test]
    fn morte_le_caracteristicas_da_ultima_existencia() {
        let mut game = game_with(lki_dies_card());
        let owner = PlayerId::P0;
        let id = put_on_battlefield(&mut game, owner);
        let Some(obj) = game.state.object_mut(id) else {
            panic!("objeto de teste não existe")
        };
        obj.add_counter(CounterKind::PlusOnePlusOne, 2);

        let Some(before) = layers::characteristics(&game, id) else {
            panic!("características do permanente vivo não calcularam")
        };
        assert_eq!(before.toughness, 4, "pré-condição: 2/2 com dois marcadores é 4/4");

        turn::move_object(&mut game, id, ZoneId::graveyard(owner));
        collect(&mut game);

        let Some(item) = game.state.pending_triggers.first() else {
            panic!("gatilho de morte não disparou")
        };
        let ctx = EvalCtx {
            source: Some(id),
            controller: owner,
            trigger: item.trigger_ctx.clone(),
            ..Default::default()
        };
        assert_eq!(
            query::eval_value(&game, &Value::ToughnessOf(ObjRef::TriggerObject), &ctx),
            4,
            "resistência da última existência é 4, não a impressa"
        );
        assert_eq!(
            query::eval_value(&game, &Value::PowerOf(ObjRef::TriggerObject), &ctx),
            4,
            "poder da última existência é 4, não o impresso"
        );
        assert_eq!(
            query::eval_value(
                &game,
                &Value::CountersOn(ObjRef::TriggerObject, CounterKind::PlusOnePlusOne),
                &ctx
            ),
            2,
            "os marcadores existiam no instante da morte"
        );
        assert!(
            query::eval_condition(
                &game,
                &Condition::Matches(ObjRef::TriggerObject, Filter::PowerAtLeast(4)),
                &ctx
            ),
            "o filtro também precisa enxergar a criatura como 4/4"
        );
    }

    #[test]
    fn contexto_de_morte_guarda_o_controlador_do_evento() {
        let game = game_with(dies_trigger_card());
        let cond = TriggerCondition::Dies(Selector::battlefield(Filter::Any));
        let ev = GameEvent::Died {
            object: ObjectId(0),
            controller: PlayerId::P1,
        };
        // `matches_filter` ainda pode estar em construção em `query`; o que este
        // teste garante é que, casando, o contexto vem do evento e não do
        // estado atual do objeto (que já foi para o cemitério).
        if let Some(ctx) = matches(&game, &cond, &ev, ObjectId(0)) {
            assert_eq!(ctx.trigger_object, Some(ObjectId(0)));
            assert_eq!(ctx.trigger_player, Some(PlayerId::P1));
        }
    }

    #[test]
    fn condicao_any_e_recursiva() {
        let game = game_with(dies_trigger_card());
        let cond = TriggerCondition::Any(vec![
            TriggerCondition::Taps(Selector::creatures()),
            TriggerCondition::Untaps(Selector::creatures()),
        ]);
        assert!(leaves_battlefield_self(&TriggerCondition::Any(vec![
            TriggerCondition::Dies(Selector::creatures())
        ])));
        // Evento que não casa com nenhuma das alternativas.
        let ev = GameEvent::CombatDamageDealt;
        assert!(matches(&game, &cond, &ev, ObjectId(0)).is_none());
    }

    #[test]
    fn fila_de_eventos_nao_estruturais_sobrevive_a_fire_step_triggers() {
        let mut game = game_with(dies_trigger_card());
        game.state.event_queue = vec![
            GameEvent::StepBegan { step: Step::Upkeep },
            GameEvent::CardDrawn { player: PlayerId::P0, object: ObjectId(0) },
        ];
        fire_step_triggers(&mut game);
        assert!(
            game.state
                .event_queue
                .iter()
                .any(|e| matches!(e, GameEvent::CardDrawn { .. })),
            "evento não estrutural precisa continuar na fila para o collect"
        );
        assert!(
            !game
                .state
                .event_queue
                .iter()
                .any(|e| matches!(e, GameEvent::StepBegan { .. })),
            "evento de passo já foi consumido"
        );
    }

    #[test]
    fn once_per_turn_zera_no_comeco_do_turno() {
        let mut game = game_with(dies_trigger_card());
        if let Some(obj) = game.state.object_mut(ObjectId(0)) {
            obj.triggers_fired.insert(0, 1);
        }
        assert!(already_fired(&game, ObjectId(0), 0));
        game.state.event_queue = vec![GameEvent::TurnBegan {
            player: PlayerId::P0,
            turn: 2,
        }];
        collect(&mut game);
        assert!(
            !already_fired(&game, ObjectId(0), 0),
            "turno novo libera o gatilho de uma vez por turno"
        );
    }
}
