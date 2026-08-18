//! Suíte de interações — itens 1 a 41 de `docs/RULES_TESTS.md`.
//!
//! Seções cobertas: estrutura de turno e prioridade (1–10), pilha/alvos/
//! anulação (11–18), camadas (19–25), ações baseadas em estado (26–35) e
//! gatilhos (36–41).
//!
//! Cada teste tem o nome exato do item no documento e cita a regra. Nenhum
//! teste aqui pode passar sem afirmar: onde o caminho feliz precisa acontecer,
//! `let ... else { panic! }` derruba o teste em vez de deixá-lo verde à toa.
mod common;

use common::*;

use mtg_core::action::{Action, Request, TargetChoice};
use mtg_core::card::{Ability, StaticAbility, StaticMod, TriggerCondition, TriggeredAbility};
use mtg_core::engine::query::EvalCtx;
use mtg_core::engine::{cast, layers, query, resolve, sba, stack, triggers, turn};
use mtg_core::event::{GameEvent, LossReason, Step};
use mtg_core::ids::{ObjectId, PlayerId};
use mtg_core::ir::{
    Condition, Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind,
    TargetSpec, Value,
};
use mtg_core::mana::Color;
use mtg_core::state::{GameOutcome, StackItemKind};
use mtg_core::types::CounterKind;
use mtg_core::view::MatchEvent;
use mtg_core::zone::ZoneId;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

// ---------------------------------------------------------------------------
// Peças reutilizadas
// ---------------------------------------------------------------------------

fn creature_target(desc: &str) -> TargetSpec {
    TargetSpec {
        kind: TargetKind::Object(Selector::creatures()),
        description: desc.to_string(),
    }
}

fn gain_life(n: i32) -> Effect {
    Effect::GainLife {
        amount: Value::c(n),
        player: PlayerRef::You,
    }
}

fn triggered(trigger: TriggerCondition, effect: Effect) -> TriggeredAbility {
    TriggeredAbility {
        trigger,
        intervening_if: Condition::Always,
        targets: Vec::new(),
        effect,
        optional: false,
        once_per_turn: false,
        triggers_from_graveyard: false,
        text: "gatilho de teste".to_string(),
    }
}

// ===========================================================================
// 1. Estrutura de turno e prioridade
// ===========================================================================

/// CR 103.7a — quem joga primeiro pula a compra do primeiro turno.
#[test]
fn primeiro_jogador_nao_compra_no_primeiro_turno() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 20);
    setup.fill(P1, bear, 20);
    setup.config.max_turns = 2;

    let mut game = setup.build_passing();
    // Mão inicial de zero cartas: toda compra observada abaixo veio de um passo
    // de compra, e não da distribuição.
    assert!(hand(&game, P0).is_empty() && hand(&game, P1).is_empty());
    game.run();

    let events = game.match_events.clone();
    let starts: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, MatchEvent::TurnStart { .. }))
        .map(|(i, _)| i)
        .collect();
    let Some(first) = starts.first().copied() else {
        panic!("a partida não registrou o começo do turno 1")
    };
    let Some(second) = starts.get(1).copied() else {
        panic!("a partida não chegou ao turno 2: sem contraste para medir a compra")
    };
    assert_eq!(
        events[first],
        MatchEvent::TurnStart {
            turn: 1,
            player: P0
        }
    );

    let drew_in_first_turn: Vec<&MatchEvent> = events[..second]
        .iter()
        .filter(|e| matches!(e, MatchEvent::CardDrawn { .. }))
        .collect();
    assert!(
        drew_in_first_turn.is_empty(),
        "CR 103.7a: ninguém compra no turno 1, mas houve {} compra(s): {:?}",
        drew_in_first_turn.len(),
        drew_in_first_turn
    );

    let drew_in_second_turn: Vec<PlayerId> = events[second..]
        .iter()
        .filter_map(|e| match e {
            MatchEvent::CardDrawn { player, .. } => Some(*player),
            _ => None,
        })
        .collect();
    assert_eq!(
        drew_in_second_turn,
        vec![P1],
        "no turno 2 só o jogador ativo compra, e exatamente uma carta"
    );
}

/// CR 502.1 — o passo de desvirar desvira só os permanentes do jogador ativo.
#[test]
fn passo_de_desvirar_desvira_so_do_jogador_ativo() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 10);
    setup.fill(P1, bear, 10);
    setup.config.max_turns = 1;

    let mut game = setup.build_passing();
    let mine = put_on_battlefield(&mut game, "Urso de Teste", P0);
    let theirs = put_on_battlefield(&mut game, "Urso de Teste", P1);
    tap(&mut game, mine);
    tap(&mut game, theirs);

    // Turno 1 é do jogador ativo P0.
    assert_eq!(game.state.active_player, P0);
    game.run();

    assert!(on_battlefield(&game, mine) && on_battlefield(&game, theirs));
    assert!(
        !is_tapped(&game, mine),
        "CR 502.1: o permanente do jogador ativo desvira"
    );
    assert!(
        is_tapped(&game, theirs),
        "CR 502.1: o permanente do oponente continua virado"
    );
}

/// CR 500.4 — o pool de mana esvazia no fim de cada passo.
#[test]
fn pool_de_mana_esvazia_no_fim_do_passo() {
    let mut setup = Setup::with_catalog();
    let forest = setup.id("Forest");
    setup.fill(P0, forest, 10);
    setup.fill(P1, forest, 10);

    let seen = log::<u32>();
    let probe = seen.clone();
    let agents = vec![
        ScriptedAgent::observing("A", move |g, r| {
            if matches!(r, Request::Priority { .. }) {
                push_log(&probe, g.state.player(P0).mana_pool.total());
            }
        })
        .boxed(),
        FixedAgent::passing("B").boxed(),
    ];
    let mut game = setup.build(agents);

    // Uma segunda ação legal (jogar terreno) é o que faz o motor de fato
    // consultar o agente: `Game::ask` devolve sozinho quando só há uma.
    put_in_hand(&mut game, "Forest", P0);
    goto_step(&mut game, Step::PrecombatMain);
    game.state.player_mut(P0).mana_pool.add(Some(Color::Red), 3);
    assert_eq!(game.state.player(P0).mana_pool.total(), 3);

    turn::give_priority(&mut game);

    let observed = read_log(&seen);
    let Some(during) = observed.first().copied() else {
        panic!("o jogador P0 nunca recebeu prioridade: nada foi observado")
    };
    assert_eq!(
        during, 3,
        "o mana continua no pool enquanto o passo corre (CR 106.4)"
    );
    assert!(
        game.state.player(P0).mana_pool.is_empty(),
        "CR 500.4: o pool tem de esvaziar ao fim do passo"
    );
}

/// CR 117.4 — quando todos passam com a pilha vazia, o passo termina.
#[test]
fn ambos_passam_com_pilha_vazia_encerra_o_passo() {
    let mut setup = Setup::with_catalog();
    let forest = setup.id("Forest");
    let flash = setup.add_card(instant_def("Instantâneo de Teste", gain_life(1), Vec::new()));
    setup.fill(P0, forest, 10);
    setup.fill(P1, flash, 10);

    let asked_a = counter();
    let asked_b = counter();
    let agents = vec![
        FixedAgent::passing("A").counting(asked_a.clone()).boxed(),
        FixedAgent::passing("B").counting(asked_b.clone()).boxed(),
    ];
    let mut game = setup.build(agents);

    // Cada jogador precisa de duas ações legais para ser consultado de fato.
    put_in_hand(&mut game, "Forest", P0);
    put_in_hand(&mut game, "Instantâneo de Teste", P1);
    goto_step(&mut game, Step::PrecombatMain);
    assert!(game.state.stack.is_empty());

    turn::give_priority(&mut game);

    assert_eq!(
        count_of(&asked_a),
        1,
        "CR 117.4: o jogador ativo recebe prioridade uma única vez"
    );
    assert_eq!(
        count_of(&asked_b),
        1,
        "CR 117.4: o não-ativo recebe prioridade uma única vez e o passo acaba"
    );
    assert!(game.state.stack.is_empty());
}

/// CR 117.4 — com a pilha cheia, os dois passando resolvem o item do topo.
#[test]
fn ambos_passam_com_pilha_cheia_resolve_o_topo() {
    let mut setup = Setup::empty();
    let bolt = setup.add_card(instant_def("Ganho de Vida", gain_life(3), Vec::new()));
    setup.fill(P0, bolt, 10);
    setup.fill(P1, bolt, 10);

    let mut game = setup.build_passing();
    set_life(&mut game, P0, 10);
    let spell = cast_onto_stack(&mut game, "Ganho de Vida", P0, Vec::new());
    goto_step(&mut game, Step::PrecombatMain);
    assert_eq!(game.state.stack.len(), 1);

    turn::give_priority(&mut game);

    assert!(
        game.state.stack.is_empty(),
        "CR 117.4: dois passes seguidos resolvem o topo da pilha"
    );
    assert_eq!(life(&game, P0), 13, "o efeito da mágica resolvida se aplicou");
    assert!(
        in_graveyard(&game, spell),
        "CR 608.2m: instantâneo resolvido vai ao cemitério"
    );
}

/// CR 514.1 — na limpeza o jogador ativo descarta até o limite de mão.
#[test]
fn limpeza_descarta_ate_sete_cartas() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 20);
    setup.fill(P1, bear, 20);
    setup.config.max_turns = 1;

    let mut game = setup.build_passing();
    for _ in 0..9 {
        put_in_hand(&mut game, "Urso de Teste", P0);
    }
    assert_eq!(hand(&game, P0).len(), 9);

    game.run();

    assert_eq!(
        hand(&game, P0).len(),
        7,
        "CR 514.1: a mão do jogador ativo cai ao limite de sete"
    );
    assert_eq!(
        graveyard(&game, P0).len(),
        2,
        "as duas cartas excedentes foram descartadas"
    );
}

/// CR 514.2 — o dano marcado some na limpeza.
#[test]
fn limpeza_remove_dano_marcado() {
    let mut setup = Setup::empty();
    let wall = setup.add_card(creature_def("Muralha de Teste", 0, 4));
    setup.fill(P0, wall, 10);
    setup.fill(P1, wall, 10);
    setup.config.max_turns = 1;

    let mut game = setup.build_passing();
    let target = put_on_battlefield(&mut game, "Muralha de Teste", P1);
    set_damage(&mut game, target, 3);
    assert_eq!(damage(&game, target), 3);

    game.run();

    assert!(
        on_battlefield(&game, target),
        "3 de dano numa 0/4 não é letal: a criatura tem de sobreviver"
    );
    assert_eq!(
        damage(&game, target),
        0,
        "CR 514.2: o dano marcado é removido na limpeza"
    );
}

/// CR 514.3a — gatilho na limpeza dá prioridade e faz outra limpeza acontecer.
#[test]
fn gatilho_na_limpeza_da_prioridade_e_repete_a_limpeza() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    let mut watcher = enchantment_def("Vigília da Limpeza");
    let mut ability = triggered(TriggerCondition::Cleanup(PlayerRef::Each), gain_life(1));
    // Sem "uma vez por turno" a limpeza se redisparia para sempre e a trava do
    // motor empataria a partida — o que testaria a trava, não a regra.
    ability.once_per_turn = true;
    watcher.abilities.push(Ability::Triggered(ability));
    let watcher_id = setup.add_card(watcher);
    setup.deck(P0, &[watcher_id]);
    setup.fill(P0, bear, 10);
    setup.fill(P1, bear, 10);
    setup.config.max_turns = 1;

    let mut game = setup.build_passing();
    put_on_battlefield(&mut game, "Vigília da Limpeza", P0);
    let before = life(&game, P0);

    game.run();

    let cleanups = game
        .match_events
        .iter()
        .filter(|e| matches!(e, MatchEvent::StepChange { step: Step::Cleanup, .. }))
        .count();
    assert!(
        cleanups >= 2,
        "CR 514.3a: o gatilho força uma segunda limpeza, mas houve {cleanups}"
    );
    assert_eq!(
        life(&game, P0) - before,
        1,
        "o gatilho de limpeza recebeu prioridade e resolveu"
    );
    assert!(
        !matches!(game.state.outcome, GameOutcome::Ongoing),
        "a partida de um turno tem de terminar"
    );
}

/// CR 305.1 — um terreno por turno, e só na janela de feitiço.
#[test]
fn terreno_so_pode_ser_jogado_uma_vez_por_turno() {
    let mut setup = Setup::with_catalog();
    let forest = setup.id("Forest");
    setup.fill(P0, forest, 10);
    setup.fill(P1, forest, 10);

    let mut game = setup.build_passing();
    let first = put_in_hand(&mut game, "Forest", P0);
    let second = put_in_hand(&mut game, "Forest", P0);
    goto_step(&mut game, Step::PrecombatMain);

    let lands_before = cast::priority_actions(&game, P0)
        .into_iter()
        .filter(|a| matches!(a, Action::PlayLand { .. }))
        .count();
    assert_eq!(lands_before, 2, "as duas florestas da mão são jogáveis");

    if let Err(e) = cast::execute(&mut game, P0, Action::PlayLand { object: first }) {
        panic!("o primeiro terreno do turno tinha de ser legal: {e}");
    }
    assert!(on_battlefield(&game, first));

    let lands_after = cast::priority_actions(&game, P0)
        .into_iter()
        .filter(|a| matches!(a, Action::PlayLand { .. }))
        .count();
    assert_eq!(
        lands_after, 0,
        "CR 305.1: depois do primeiro, nenhum terreno aparece nas ações legais"
    );

    let Err(_) = cast::execute(&mut game, P0, Action::PlayLand { object: second }) else {
        panic!("CR 305.1: o segundo terreno do turno tinha de ser rejeitado")
    };
    assert!(
        !on_battlefield(&game, second),
        "o terreno rejeitado não pode ter entrado em campo"
    );
}

/// CR 307.1 — feitiço só com a pilha vazia; instantâneo não tem essa trava.
#[test]
fn feitico_nao_pode_ser_lancado_com_pilha_nao_vazia() {
    let mut setup = Setup::empty();
    let sorcery = setup.add_card(sorcery_def("Feitiço de Teste", gain_life(1), Vec::new()));
    let instant = setup.add_card(instant_def("Instantâneo de Teste", gain_life(1), Vec::new()));
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.deck(P0, &[sorcery, instant]);
    setup.fill(P0, bear, 10);
    setup.fill(P1, bear, 10);

    let mut game = setup.build_passing();
    let sorcery_obj = put_in_hand(&mut game, "Feitiço de Teste", P0);
    let instant_obj = put_in_hand(&mut game, "Instantâneo de Teste", P0);
    goto_step(&mut game, Step::PrecombatMain);

    let castable = |game: &mtg_core::engine::Game, obj: ObjectId| {
        cast::priority_actions(game, P0)
            .into_iter()
            .any(|a| matches!(a, Action::CastSpell { object, .. } if object == obj))
    };
    assert!(
        castable(&game, sorcery_obj),
        "com a pilha vazia o feitiço é lançável"
    );
    assert!(castable(&game, instant_obj));

    // Qualquer coisa na pilha fecha a janela de feitiço (CR 307.1).
    cast_onto_stack(&mut game, "Urso de Teste", P1, Vec::new());
    assert_eq!(game.state.stack.len(), 1);

    assert!(
        !castable(&game, sorcery_obj),
        "CR 307.1: feitiço não aparece nas ações legais com a pilha cheia"
    );
    assert!(
        castable(&game, instant_obj),
        "CR 117.1a: o instantâneo continua lançável"
    );

    let action = Action::CastSpell {
        object: sorcery_obj,
        targets: Vec::new(),
        x: 0,
        modes: Vec::new(),
        mana_plan: Vec::new(),
    };
    let Err(_) = cast::execute(&mut game, P0, action) else {
        panic!("CR 307.1: lançar feitiço com a pilha cheia tinha de ser rejeitado")
    };
}

// ===========================================================================
// 2. Pilha, alvos e anulação
// ===========================================================================

/// A pilha é LIFO: o último item a entrar é o primeiro a resolver (CR 608.1).
#[test]
fn pilha_resolve_em_ordem_inversa() {
    let mut setup = Setup::empty();
    let five = setup.add_card(instant_def(
        "Define Cinco",
        Effect::SetLife {
            amount: Value::c(5),
            player: PlayerRef::You,
        },
        Vec::new(),
    ));
    let nine = setup.add_card(instant_def(
        "Define Nove",
        Effect::SetLife {
            amount: Value::c(9),
            player: PlayerRef::You,
        },
        Vec::new(),
    ));
    setup.deck(P0, &[five, nine]);
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    cast_onto_stack(&mut game, "Define Cinco", P0, Vec::new());
    cast_onto_stack(&mut game, "Define Nove", P0, Vec::new());
    assert_eq!(game.state.stack.len(), 2);

    stack::resolve_top(&mut game);
    assert_eq!(
        life(&game, P0),
        9,
        "a última mágica a entrar na pilha resolve primeiro"
    );

    stack::resolve_top(&mut game);
    assert_eq!(
        life(&game, P0),
        5,
        "a primeira a entrar resolve por último e sobrescreve"
    );
    assert!(game.state.stack.is_empty());
}

/// CR 608.2b — mágica cujo único alvo ficou ilegal não resolve (fizzle).
#[test]
fn magica_com_unico_alvo_ilegal_nao_resolve() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 3, 3));
    let bolt = setup.add_card(instant_def(
        "Raio de Teste",
        Effect::seq([
            Effect::DealDamage {
                amount: Value::c(2),
                target: ObjRef::Target(0),
            },
            gain_life(5),
        ]),
        vec![creature_target("alvo de criatura")],
    ));
    setup.deck(P0, &[bolt]);
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let victim = put_on_battlefield(&mut game, "Urso de Teste", P1);
    let spell = cast_onto_stack(
        &mut game,
        "Raio de Teste",
        P0,
        vec![TargetChoice::Object(victim)],
    );
    let before = life(&game, P0);

    // O alvo deixa o campo antes da resolução: já não é alvo legal.
    turn::move_object(&mut game, victim, ZoneId::graveyard(P1));
    game.state.event_queue.clear();

    stack::resolve_top(&mut game);

    assert_eq!(
        life(&game, P0),
        before,
        "CR 608.2b: nada da mágica acontece quando o único alvo é ilegal"
    );
    assert!(
        in_graveyard(&game, spell),
        "a mágica que não resolve vai para o cemitério do dono"
    );
    assert!(
        game.state
            .event_queue
            .iter()
            .any(|e| matches!(e, GameEvent::SpellCountered { .. })),
        "o fizzle emite SpellCountered"
    );
    assert!(
        !game
            .state
            .event_queue
            .iter()
            .any(|e| matches!(e, GameEvent::SpellResolved { .. })),
        "mágica que não resolve não pode emitir SpellResolved"
    );
}

/// CR 608.2b — com dois alvos e só um ilegal, a mágica resolve no que sobrou.
#[test]
fn magica_com_dois_alvos_e_um_ilegal_ainda_resolve_no_outro() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 3, 3));
    let split = setup.add_card(instant_def(
        "Raio Duplo",
        Effect::seq([
            Effect::DealDamage {
                amount: Value::c(2),
                target: ObjRef::Target(0),
            },
            Effect::DealDamage {
                amount: Value::c(2),
                target: ObjRef::Target(1),
            },
            gain_life(5),
        ]),
        vec![
            creature_target("primeira criatura alvo"),
            creature_target("segunda criatura alvo"),
        ],
    ));
    setup.deck(P0, &[split]);
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let gone = put_on_battlefield(&mut game, "Urso de Teste", P1);
    let survivor = put_on_battlefield(&mut game, "Urso de Teste", P1);
    let spell = cast_onto_stack(
        &mut game,
        "Raio Duplo",
        P0,
        vec![TargetChoice::Object(gone), TargetChoice::Object(survivor)],
    );
    let before = life(&game, P0);

    turn::move_object(&mut game, gone, ZoneId::graveyard(P1));
    game.state.event_queue.clear();

    stack::resolve_top(&mut game);

    assert_eq!(
        damage(&game, survivor),
        2,
        "CR 608.2b: o alvo que continua legal recebe o efeito"
    );
    assert_eq!(
        life(&game, P0) - before,
        5,
        "o resto do efeito da mágica também acontece"
    );
    assert!(
        game.state
            .event_queue
            .iter()
            .any(|e| matches!(e, GameEvent::SpellResolved { .. })),
        "a mágica resolveu de verdade"
    );
    assert!(in_graveyard(&game, spell));
}

/// CR 701.5a — mágica anulada vai para o cemitério do dono.
#[test]
fn contra_magica_manda_o_alvo_para_o_cemiterio() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    let counter_spell = setup.add_card(instant_def(
        "Anular de Teste",
        Effect::CounterSpell {
            target: ObjRef::Target(0),
            unless_pays: None,
        },
        vec![TargetSpec {
            kind: TargetKind::SpellOnStack(Filter::Any),
            description: "mágica alvo".to_string(),
        }],
    ));
    setup.deck(P0, &[counter_spell]);
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let victim = cast_onto_stack(&mut game, "Urso de Teste", P1, Vec::new());
    cast_onto_stack(
        &mut game,
        "Anular de Teste",
        P0,
        vec![TargetChoice::Object(victim)],
    );
    assert_eq!(game.state.stack.len(), 2);

    stack::resolve_top(&mut game);

    assert!(
        game.state.stack.is_empty(),
        "a contra-magia resolveu e tirou a mágica anulada da pilha"
    );
    assert!(
        in_graveyard(&game, victim),
        "CR 701.5a: a mágica anulada vai para o cemitério do dono"
    );
    assert!(
        !on_battlefield(&game, victim),
        "criatura anulada nunca chega ao campo de batalha"
    );
    assert!(
        game.state
            .event_queue
            .iter()
            .any(|e| matches!(e, GameEvent::SpellCountered { object } if *object == victim)),
        "a anulação emite SpellCountered para a mágica certa"
    );
}

/// CR 702.11b — hexproof barra o oponente, não o próprio controlador.
#[test]
fn hexproof_impede_alvo_de_oponente_mas_nao_do_controlador() {
    let mut setup = Setup::empty();
    let mut hexed = creature_def("Guardião Esquivo", 2, 2);
    hexed.abilities.push(Ability::Keyword(Keyword::Hexproof));
    let hexed_id = setup.add_card(hexed);
    let plain = setup.add_card(creature_def("Urso de Teste", 2, 2));
    let bolt = setup.add_card(instant_def(
        "Raio de Teste",
        Effect::DealDamage {
            amount: Value::c(2),
            target: ObjRef::Target(0),
        },
        vec![creature_target("alvo de criatura")],
    ));
    setup.deck(P0, &[hexed_id, plain]);
    setup.deck(P1, &[bolt]);
    setup.fill(P0, plain, 5);
    setup.fill(P1, plain, 5);

    let mut game = setup.build_passing();
    let guarded = put_on_battlefield(&mut game, "Guardião Esquivo", P0);
    let open = put_on_battlefield(&mut game, "Urso de Teste", P0);
    let source = put_in_hand(&mut game, "Raio de Teste", P1);

    let spec = creature_target("alvo de criatura");
    let by_opponent = query::legal_targets(&game, &spec, &eval_ctx(source, P1, Vec::new()));
    let by_controller = query::legal_targets(&game, &spec, &eval_ctx(source, P0, Vec::new()));

    assert!(
        !by_opponent.contains(&TargetChoice::Object(guarded)),
        "CR 702.11b: o oponente não pode escolher a criatura com hexproof"
    );
    assert!(
        by_opponent.contains(&TargetChoice::Object(open)),
        "a criatura sem hexproof continua alvo legal do oponente"
    );
    assert!(
        by_controller.contains(&TargetChoice::Object(guarded)),
        "CR 702.11b: o controlador pode escolher a própria criatura com hexproof"
    );
    assert!(!query::can_be_targeted(&game, guarded, Some(source), P1));
    assert!(query::can_be_targeted(&game, guarded, Some(source), P0));
}

/// CR 702.18a — shroud impede qualquer alvo, do oponente e do controlador.
#[test]
fn shroud_impede_alvo_inclusive_do_controlador() {
    let mut setup = Setup::empty();
    let mut shrouded = creature_def("Eremita Velado", 2, 2);
    shrouded.abilities.push(Ability::Keyword(Keyword::Shroud));
    let shrouded_id = setup.add_card(shrouded);
    let plain = setup.add_card(creature_def("Urso de Teste", 2, 2));
    let bolt = setup.add_card(instant_def(
        "Raio de Teste",
        Effect::DealDamage {
            amount: Value::c(2),
            target: ObjRef::Target(0),
        },
        vec![creature_target("alvo de criatura")],
    ));
    setup.deck(P0, &[shrouded_id, plain, bolt]);
    setup.fill(P0, plain, 5);
    setup.fill(P1, plain, 5);

    let mut game = setup.build_passing();
    let veiled = put_on_battlefield(&mut game, "Eremita Velado", P0);
    let open = put_on_battlefield(&mut game, "Urso de Teste", P0);
    let source = put_in_hand(&mut game, "Raio de Teste", P0);

    let spec = creature_target("alvo de criatura");
    let by_controller = query::legal_targets(&game, &spec, &eval_ctx(source, P0, Vec::new()));
    let by_opponent = query::legal_targets(&game, &spec, &eval_ctx(source, P1, Vec::new()));

    assert!(
        !by_controller.contains(&TargetChoice::Object(veiled)),
        "CR 702.18a: nem o controlador escolhe uma criatura com shroud"
    );
    assert!(
        !by_opponent.contains(&TargetChoice::Object(veiled)),
        "CR 702.18a: o oponente também não escolhe"
    );
    assert!(
        by_controller.contains(&TargetChoice::Object(open)),
        "a criatura sem shroud continua alvo legal"
    );
}

/// CR 702.16b — proteção contra vermelho barra alvo de fonte vermelha.
#[test]
fn protecao_contra_vermelho_impede_alvo_de_magica_vermelha() {
    let mut setup = Setup::empty();
    let mut protected = creature_def("Cavaleiro Protegido", 2, 2);
    protected
        .abilities
        .push(Ability::Keyword(Keyword::Protection(Color::Red)));
    let protected_id = setup.add_card(protected);
    let plain = setup.add_card(creature_def("Urso de Teste", 2, 2));
    let red_bolt = setup.add_card(colored(
        instant_def(
            "Raio Vermelho",
            Effect::DealDamage {
                amount: Value::c(2),
                target: ObjRef::Target(0),
            },
            vec![creature_target("alvo de criatura")],
        ),
        Color::Red,
    ));
    let white_bolt = setup.add_card(colored(
        instant_def(
            "Lança Branca",
            Effect::DealDamage {
                amount: Value::c(2),
                target: ObjRef::Target(0),
            },
            vec![creature_target("alvo de criatura")],
        ),
        Color::White,
    ));
    setup.deck(P0, &[protected_id, plain]);
    setup.deck(P1, &[red_bolt, white_bolt]);
    setup.fill(P0, plain, 5);
    setup.fill(P1, plain, 5);

    let mut game = setup.build_passing();
    let knight = put_on_battlefield(&mut game, "Cavaleiro Protegido", P0);
    let red_source = put_in_hand(&mut game, "Raio Vermelho", P1);
    let white_source = put_in_hand(&mut game, "Lança Branca", P1);

    let spec = creature_target("alvo de criatura");
    let from_red = query::legal_targets(&game, &spec, &eval_ctx(red_source, P1, Vec::new()));
    let from_white = query::legal_targets(&game, &spec, &eval_ctx(white_source, P1, Vec::new()));

    assert!(
        !from_red.contains(&TargetChoice::Object(knight)),
        "CR 702.16b: mágica vermelha não pode escolher quem tem proteção contra vermelho"
    );
    assert!(
        from_white.contains(&TargetChoice::Object(knight)),
        "a proteção é só contra a cor nomeada: a mágica branca ainda mira"
    );
}

/// CR 702.16c — proteção previne o dano de uma fonte da cor protegida.
///
/// Corrigido em `resolve::protected_from`: antes só `combat::damage_prevented`
/// fazia a checagem, então dano fora de combate atravessava a proteção.
#[test]
fn protecao_previne_dano_da_cor_protegida() {
    let mut setup = Setup::empty();
    let mut protected = creature_def("Cavaleiro Protegido", 2, 4);
    protected
        .abilities
        .push(Ability::Keyword(Keyword::Protection(Color::Red)));
    let protected_id = setup.add_card(protected);
    let plain = setup.add_card(creature_def("Urso de Teste", 2, 2));
    let red_source = setup.add_card(colored(creature_def("Diabrete Vermelho", 1, 1), Color::Red));
    setup.deck(P0, &[protected_id]);
    setup.deck(P1, &[red_source]);
    setup.fill(P0, plain, 5);
    setup.fill(P1, plain, 5);

    let mut game = setup.build_passing();
    let knight = put_on_battlefield(&mut game, "Cavaleiro Protegido", P0);
    let imp = put_on_battlefield(&mut game, "Diabrete Vermelho", P1);
    let Some(imp_chars) = layers::characteristics(&game, imp) else {
        panic!("a fonte de dano precisa existir")
    };
    assert!(
        imp_chars.colors.contains(Color::Red),
        "pré-condição: a fonte do dano é vermelha"
    );

    // Dano sem escolha de alvo: a proteção não pode se esconder atrás de
    // "não pode ser alvo" — o que se mede aqui é a prevenção do dano.
    let mut ctx = eval_ctx(imp, P1, vec![TargetChoice::Object(knight)]);
    resolve::resolve_effect(
        &mut game,
        &Effect::DealDamage {
            amount: Value::c(2),
            target: ObjRef::Target(0),
        },
        &mut ctx,
    );

    assert_eq!(
        damage(&game, knight),
        0,
        "CR 702.16c: dano de fonte vermelha em quem tem proteção contra vermelho é prevenido"
    );
}

// ===========================================================================
// 3. Camadas (CR 613)
// ===========================================================================

/// CR 613.4c/d — bônus estático (7c) e marcador (7d) somam.
#[test]
fn anthem_soma_com_marcador_de_mais_um() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    let mut anthem = enchantment_def("Hino de Teste");
    anthem.abilities.push(Ability::Static(StaticAbility {
        condition: Condition::Always,
        affects: Selector::creatures().yours(),
        modification: StaticMod::ModifyPT(Value::c(1), Value::c(1)),
        text: "Criaturas que você controla recebem +1/+1.".to_string(),
    }));
    let anthem_id = setup.add_card(anthem);
    setup.deck(P0, &[anthem_id]);
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P0);
    put_on_battlefield(&mut game, "Hino de Teste", P0);
    give_counters(&mut game, creature, CounterKind::PlusOnePlusOne, 1);

    assert_eq!(
        pt(&game, creature),
        (4, 4),
        "2/2 impresso + 1/1 do hino (7c) + 1/1 do marcador (7d)"
    );

    let Some(base) = layers::base_characteristics(&game, creature) else {
        panic!("a criatura precisa ter características impressas")
    };
    assert_eq!((base.power, base.toughness), (2, 2));
}

/// CR 613.4b/c — define P/T é camada 7b e roda antes de modifica P/T (7c),
/// mesmo tendo timestamp mais novo.
#[test]
fn define_pt_aplicado_depois_de_modifica_pt_ganha() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P0);
    let mut ctx = eval_ctx(creature, P0, vec![TargetChoice::Object(creature)]);

    // Primeiro o modifica (+3/+3), timestamp menor.
    resolve::resolve_effect(
        &mut game,
        &Effect::ModifyPT {
            target: ObjRef::Target(0),
            power: Value::c(3),
            toughness: Value::c(3),
            duration: Duration::EndOfTurn,
        },
        &mut ctx,
    );
    assert_eq!(pt(&game, creature), (5, 5));

    // Depois o define (1/1), timestamp maior — e ainda assim aplica antes.
    resolve::resolve_effect(
        &mut game,
        &Effect::SetPT {
            target: ObjRef::Target(0),
            power: Value::c(1),
            toughness: Value::c(1),
            duration: Duration::EndOfTurn,
        },
        &mut ctx,
    );

    assert_eq!(
        pt(&game, creature),
        (4, 4),
        "CR 613.4: 7b define 1/1, 7c soma +3/+3 por cima"
    );
}

/// CR 613.7 — dentro da mesma camada vence o timestamp mais novo.
#[test]
fn dois_efeitos_de_define_pt_o_mais_novo_ganha() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P0);
    let mut ctx = eval_ctx(creature, P0, vec![TargetChoice::Object(creature)]);

    let set = |p: i32, t: i32| Effect::SetPT {
        target: ObjRef::Target(0),
        power: Value::c(p),
        toughness: Value::c(t),
        duration: Duration::EndOfTurn,
    };

    resolve::resolve_effect(&mut game, &set(1, 1), &mut ctx);
    assert_eq!(pt(&game, creature), (1, 1));

    resolve::resolve_effect(&mut game, &set(5, 5), &mut ctx);

    assert_eq!(game.state.continuous.len(), 2, "os dois efeitos coexistem");
    assert_eq!(
        pt(&game, creature),
        (5, 5),
        "CR 613.7: os dois estão na camada 7b, então o mais novo é quem vale"
    );
}

/// CR 613.7 — perda de palavra-chave com timestamp posterior remove o ganho.
#[test]
fn perda_de_palavra_chave_depois_de_ganho_remove() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P0);
    let mut ctx = eval_ctx(creature, P0, vec![TargetChoice::Object(creature)]);
    assert!(!has_keyword(&game, creature, &Keyword::Flying));

    resolve::resolve_effect(
        &mut game,
        &Effect::GrantKeywords {
            target: ObjRef::Target(0),
            keywords: vec![Keyword::Flying],
            duration: Duration::EndOfTurn,
        },
        &mut ctx,
    );
    assert!(
        has_keyword(&game, creature, &Keyword::Flying),
        "a concessão entrou na camada 6"
    );

    resolve::resolve_effect(
        &mut game,
        &Effect::LoseKeywords {
            target: ObjRef::Target(0),
            keywords: vec![Keyword::Flying],
            duration: Duration::EndOfTurn,
        },
        &mut ctx,
    );

    assert!(
        !has_keyword(&game, creature, &Keyword::Flying),
        "CR 613.7: a perda com timestamp maior apaga o ganho anterior"
    );
}

/// CR 514.2 — efeito "até o fim do turno" acaba na limpeza.
#[test]
fn efeito_de_fim_de_turno_expira_na_limpeza() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 10);
    setup.fill(P1, bear, 10);
    setup.config.max_turns = 1;

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P1);
    let mut ctx = eval_ctx(creature, P1, vec![TargetChoice::Object(creature)]);
    resolve::resolve_effect(
        &mut game,
        &Effect::ModifyPT {
            target: ObjRef::Target(0),
            power: Value::c(3),
            toughness: Value::c(3),
            duration: Duration::EndOfTurn,
        },
        &mut ctx,
    );
    assert_eq!(pt(&game, creature), (5, 5));

    game.run();

    assert!(
        game.state.continuous.is_empty(),
        "CR 514.2: o efeito de fim de turno some na limpeza"
    );
    assert_eq!(
        pt(&game, creature),
        (2, 2),
        "a criatura volta ao P/T impresso"
    );
}

/// CR 611.2b — efeito que depende da fonte no campo acaba quando ela sai.
///
/// Corrigido em `layers::effect_source_present`: antes só a limpeza do turno
/// (`expire_continuous_effects`) removia o efeito, e entre a saída da fonte e a
/// limpeza a criatura continuava com o bônus.
#[test]
fn efeito_expira_quando_a_fonte_sai_do_campo() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    let totem = setup.add_card(enchantment_def("Totem de Teste"));
    setup.deck(P0, &[totem]);
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P0);
    let source = put_on_battlefield(&mut game, "Totem de Teste", P0);

    let mut ctx = eval_ctx(source, P0, vec![TargetChoice::Object(creature)]);
    resolve::resolve_effect(
        &mut game,
        &Effect::ModifyPT {
            target: ObjRef::Target(0),
            power: Value::c(2),
            toughness: Value::c(2),
            duration: Duration::WhileSourcePresent,
        },
        &mut ctx,
    );
    assert_eq!(pt(&game, creature), (4, 4), "com a fonte em campo, o bônus vale");

    turn::move_object(&mut game, source, ZoneId::graveyard(P0));

    assert_eq!(
        pt(&game, creature),
        (2, 2),
        "CR 611.2b: sem a fonte no campo o efeito já não se aplica"
    );
}

/// CR 613.4d + CR 704.5f — marcadores −1/−1 zeram a resistência e matam.
#[test]
fn marcadores_menos_um_reduzem_resistencia_e_matam() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P0);
    give_counters(&mut game, creature, CounterKind::MinusOneMinusOne, 2);

    assert_eq!(
        pt(&game, creature),
        (0, 0),
        "camada 7d: dois marcadores −1/−1 numa 2/2"
    );

    assert!(sba::check(&mut game), "CR 704.5f é uma ação baseada em estado");
    assert!(
        in_graveyard(&game, creature),
        "CR 704.5f: resistência 0 põe a criatura no cemitério"
    );
}

// ===========================================================================
// 4. Ações baseadas em estado (CR 704)
// ===========================================================================

/// CR 704.5a — vida 0 ou menos derrota o jogador.
#[test]
fn vida_zero_perde_o_jogo() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    set_life(&mut game, P0, 0);

    assert!(sba::check(&mut game));

    assert!(game.state.player(P0).has_lost, "CR 704.5a: vida 0 derrota");
    assert_eq!(
        game.state.player(P0).loss_reason,
        Some(LossReason::ZeroLife)
    );
    assert_eq!(game.state.outcome, GameOutcome::Winner(P1));
}

/// CR 704.5b — comprar de biblioteca vazia não derrota na hora; a derrota é da
/// próxima ação baseada em estado.
#[test]
fn comprar_de_biblioteca_vazia_perde_na_proxima_sba() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 3);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    for id in library(&game, P0) {
        turn::move_object(&mut game, id, ZoneId::EXILE);
    }
    assert!(library(&game, P0).is_empty());

    let drawn = turn::draw_card(&mut game, P0);

    assert!(drawn.is_none(), "não há carta a comprar");
    assert!(
        !game.state.player(P0).has_lost,
        "CR 704.5b: a derrota não é imediata"
    );
    assert_eq!(
        game.state.player(P0).loss_reason,
        Some(LossReason::DrewFromEmptyLibrary),
        "a tentativa fica marcada até a próxima SBA"
    );
    assert_eq!(game.state.outcome, GameOutcome::Ongoing);

    assert!(sba::check(&mut game));
    assert!(
        game.state.player(P0).has_lost,
        "CR 704.5b: a SBA seguinte é que derrota"
    );
}

/// CR 704.5f — criatura com resistência 0 vai para o cemitério.
#[test]
fn criatura_com_resistencia_zero_vai_para_o_cemiterio() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P0);
    let mut ctx = eval_ctx(creature, P0, vec![TargetChoice::Object(creature)]);
    resolve::resolve_effect(
        &mut game,
        &Effect::SetPT {
            target: ObjRef::Target(0),
            power: Value::c(2),
            toughness: Value::c(0),
            duration: Duration::EndOfTurn,
        },
        &mut ctx,
    );
    assert_eq!(pt(&game, creature), (2, 0));

    assert!(sba::check(&mut game));
    assert!(in_graveyard(&game, creature), "CR 704.5f");
}

/// CR 704.5g — dano marcado igual ou maior que a resistência destrói.
#[test]
fn dano_letal_destroi() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P0);
    set_damage(&mut game, creature, 1);
    assert!(
        !sba::check(&mut game),
        "1 de dano numa 2/2 não é letal: nenhuma SBA se aplica"
    );
    assert!(on_battlefield(&game, creature));

    set_damage(&mut game, creature, 2);
    assert!(sba::check(&mut game));

    assert!(in_graveyard(&game, creature), "CR 704.5g: dano letal destrói");
    assert!(
        game.match_events
            .iter()
            .any(|e| matches!(e, MatchEvent::Destroyed { card } if *card == creature)),
        "morte por dano é destruição e tem de aparecer como tal"
    );
}

/// CR 704.5g + CR 702.12b — indestrutível ignora dano letal.
#[test]
fn indestrutivel_sobrevive_a_dano_letal() {
    let mut setup = Setup::empty();
    let mut tough = creature_def("Guardião Indestrutível", 2, 2);
    tough
        .abilities
        .push(Ability::Keyword(Keyword::Indestructible));
    let tough_id = setup.add_card(tough);
    setup.fill(P0, tough_id, 5);
    setup.fill(P1, tough_id, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Guardião Indestrutível", P0);
    set_damage(&mut game, creature, 99);

    sba::check(&mut game);

    assert!(
        on_battlefield(&game, creature),
        "CR 702.12b: indestrutível não é destruído por dano letal"
    );
    assert_eq!(
        damage(&game, creature),
        99,
        "o dano continua marcado até a limpeza (CR 120.3)"
    );
}

/// CR 704.5f — indestrutível não salva de resistência 0.
#[test]
fn indestrutivel_com_resistencia_zero_ainda_morre() {
    let mut setup = Setup::empty();
    let mut tough = creature_def("Guardião Indestrutível", 2, 2);
    tough
        .abilities
        .push(Ability::Keyword(Keyword::Indestructible));
    let tough_id = setup.add_card(tough);
    setup.fill(P0, tough_id, 5);
    setup.fill(P1, tough_id, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Guardião Indestrutível", P0);
    assert!(has_keyword(&game, creature, &Keyword::Indestructible));
    give_counters(&mut game, creature, CounterKind::MinusOneMinusOne, 2);
    assert_eq!(pt(&game, creature), (0, 0));

    assert!(sba::check(&mut game));
    assert!(
        in_graveyard(&game, creature),
        "CR 704.5f não é destruição: indestrutível não protege"
    );
}

/// CR 704.5h — qualquer dano de fonte com toque mortal é letal.
#[test]
fn toque_mortal_com_um_de_dano_destroi() {
    let mut setup = Setup::empty();
    let mut assassin = creature_def("Assassino de Teste", 1, 1);
    assassin.abilities.push(Ability::Keyword(Keyword::Deathtouch));
    let assassin_id = setup.add_card(assassin);
    let giant = setup.add_card(creature_def("Gigante de Teste", 4, 4));
    setup.deck(P1, &[assassin_id]);
    setup.fill(P0, giant, 5);
    setup.fill(P1, giant, 5);

    let mut game = setup.build_passing();
    let big = put_on_battlefield(&mut game, "Gigante de Teste", P0);
    let killer = put_on_battlefield(&mut game, "Assassino de Teste", P1);

    let mut ctx = eval_ctx(killer, P1, vec![TargetChoice::Object(big)]);
    resolve::resolve_effect(
        &mut game,
        &Effect::DealDamage {
            amount: Value::c(1),
            target: ObjRef::Target(0),
        },
        &mut ctx,
    );

    assert_eq!(damage(&game, big), 1, "só 1 de dano numa 4/4");
    let Some(state) = game.state.object(big) else {
        panic!("o gigante precisa existir para o dano ser medido")
    };
    assert!(
        state.deathtouch_damage,
        "CR 702.2b: o dano foi marcado como de toque mortal"
    );

    assert!(sba::check(&mut game));
    assert!(
        in_graveyard(&game, big),
        "CR 704.5h: 1 de dano com toque mortal destrói"
    );
}

/// CR 704.5j — duas lendas iguais do mesmo controlador: fica uma, escolhida
/// pelo controlador.
#[test]
fn regra_da_lenda_mantem_uma() {
    let mut setup = Setup::empty();
    let legend = setup.add_card(legendary_creature_def("Lenda de Teste", 3, 3));
    let filler = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.deck(P0, &[legend, legend]);
    setup.fill(P0, filler, 5);
    setup.fill(P1, filler, 5);

    let kept = log::<ObjectId>();
    let record = kept.clone();
    let agents = vec![
        ScriptedAgent::new("A", move |request, legal| {
            if let Request::SelectObjects { candidates, .. } = request {
                if candidates.len() == 2 {
                    // Escolhe a segunda de propósito: se o motor ignorasse a
                    // resposta e ficasse com a primeira, o teste falharia.
                    let choice = Action::SelectObjects {
                        objects: vec![candidates[1]],
                    };
                    assert!(
                        legal.contains(&choice),
                        "a escolha da regra da lenda tem de estar entre as ações legais"
                    );
                    push_log(&record, candidates[1]);
                    return choice;
                }
            }
            default_action(legal)
        })
        .boxed(),
        FixedAgent::passing("B").boxed(),
    ];
    let mut game = setup.build(agents);

    let first = put_on_battlefield(&mut game, "Lenda de Teste", P0);
    let second = put_on_battlefield(&mut game, "Lenda de Teste", P0);

    assert!(sba::check(&mut game), "CR 704.5j se aplica");

    let chosen = read_log(&kept);
    let Some(survivor) = chosen.first().copied() else {
        panic!("o controlador nunca foi consultado sobre qual lenda fica")
    };
    let doomed = if survivor == first { second } else { first };

    assert!(
        on_battlefield(&game, survivor),
        "a lenda escolhida pelo controlador permanece"
    );
    assert!(
        in_graveyard(&game, doomed),
        "CR 704.5j: a outra vai para o cemitério"
    );
    let legends = battlefield(&game)
        .into_iter()
        .filter(|id| *id == first || *id == second)
        .count();
    assert_eq!(legends, 1, "sobra exatamente uma lenda em campo");
}

/// CR 704.5r — marcadores +1/+1 e −1/−1 se anulam aos pares.
#[test]
fn marcadores_opostos_se_anulam() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso de Teste", P0);
    give_counters(&mut game, creature, CounterKind::PlusOnePlusOne, 3);
    give_counters(&mut game, creature, CounterKind::MinusOneMinusOne, 2);

    assert!(sba::check(&mut game));

    let Some(state) = game.state.object(creature) else {
        panic!("a criatura precisa continuar existindo")
    };
    assert_eq!(state.counter(&CounterKind::PlusOnePlusOne), 1);
    assert_eq!(state.counter(&CounterKind::MinusOneMinusOne), 0);
    assert_eq!(
        pt(&game, creature),
        (3, 3),
        "sobra um +1/+1 numa 2/2 depois da anulação"
    );
}

/// CR 704.5m — aura que não está presa a nada legal vai para o cemitério.
#[test]
fn aura_sem_alvo_legal_vai_para_o_cemiterio() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    let aura = setup.add_card(aura_def("Encantamento de Teste"));
    setup.deck(P0, &[aura]);
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    let host = put_on_battlefield(&mut game, "Urso de Teste", P0);
    let enchantment = put_on_battlefield(&mut game, "Encantamento de Teste", P0);
    let Some(o) = game.state.object_mut(enchantment) else {
        panic!("{enchantment} não existe: a aura precisa estar em campo");
    };
    o.attached_to = Some(host);
    let Some(o) = game.state.object_mut(host) else {
        panic!("{host} não existe: a criatura hospedeira precisa estar em campo");
    };
    o.attachments.push(enchantment);
    assert!(
        !sba::check(&mut game),
        "aura presa a uma criatura em campo é legal"
    );

    turn::move_object(&mut game, host, ZoneId::graveyard(P0));

    assert!(sba::check(&mut game));
    assert!(
        in_graveyard(&game, enchantment),
        "CR 704.5m: aura sem permanente ao qual estar presa vai ao cemitério"
    );
}

// ===========================================================================
// 5. Gatilhos (CR 603)
// ===========================================================================

/// CR 603.3 — o gatilho espera fora da pilha e entra nela na próxima vez que
/// alguém receberia prioridade.
#[test]
fn gatilho_de_entrada_vai_para_a_pilha_antes_da_proxima_prioridade() {
    let mut setup = Setup::empty();
    let mut sentry = creature_def("Sentinela de Teste", 1, 1);
    sentry.abilities.push(Ability::Triggered(triggered(
        TriggerCondition::EntersBattlefield(Selector::creatures()),
        gain_life(2),
    )));
    let sentry_id = setup.add_card(sentry);
    setup.fill(P0, sentry_id, 5);
    setup.fill(P1, sentry_id, 5);

    let mut game = setup.build_passing();
    let before = life(&game, P0);
    put_on_battlefield(&mut game, "Sentinela de Teste", P0);

    triggers::collect(&mut game);

    assert_eq!(
        game.state.pending_triggers.len(),
        1,
        "o gatilho de entrada disparou"
    );
    assert!(
        game.state.stack.is_empty(),
        "CR 603.3: o gatilho ainda não está na pilha"
    );

    goto_step(&mut game, Step::PrecombatMain);
    turn::give_priority(&mut game);

    assert!(game.state.pending_triggers.is_empty());
    assert!(game.state.stack.is_empty(), "o gatilho entrou e resolveu");
    assert_eq!(
        life(&game, P0) - before,
        2,
        "CR 603.3: o gatilho foi para a pilha antes da prioridade e resolveu"
    );
}

/// CR 603.6d — o gatilho de morte lê a informação conhecida por último.
#[test]
fn gatilho_de_morte_ve_o_estado_de_antes_da_morte() {
    let mut setup = Setup::empty();
    let mut spiteful = creature_def("Urso Rancoroso", 2, 2);
    spiteful.abilities.push(Ability::Triggered(triggered(
        TriggerCondition::Dies(Selector::creatures()),
        Effect::GainLife {
            amount: Value::ToughnessOf(ObjRef::TriggerObject),
            player: PlayerRef::You,
        },
    )));
    let spiteful_id = setup.add_card(spiteful);
    setup.fill(P0, spiteful_id, 5);
    setup.fill(P1, spiteful_id, 5);

    let mut game = setup.build_passing();
    let creature = put_on_battlefield(&mut game, "Urso Rancoroso", P0);
    give_counters(&mut game, creature, CounterKind::PlusOnePlusOne, 2);
    assert_eq!(pt(&game, creature), (4, 4), "pré-condição: morre como 4/4");
    let before = life(&game, P0);

    turn::move_object(&mut game, creature, ZoneId::graveyard(P0));
    triggers::collect(&mut game);
    stack::put_triggers_on_stack(&mut game);

    let Some(item) = stack::peek(&game) else {
        panic!("o gatilho de morte não chegou à pilha")
    };
    assert!(matches!(item.kind, StackItemKind::TriggeredAbility { .. }));

    stack::resolve_top(&mut game);

    assert_eq!(
        life(&game, P0) - before,
        4,
        "CR 603.6d: vale a resistência de quando morreu (4), não a impressa (2)"
    );
}

/// CR 603.3b — gatilhos simultâneos entram na pilha em ordem APNAP: primeiro os
/// do jogador ativo.
#[test]
fn gatilhos_simultaneos_apnap_jogador_ativo_primeiro() {
    let mut setup = Setup::empty();
    let mut bell = enchantment_def("Sino de Teste");
    bell.abilities.push(Ability::Triggered(triggered(
        TriggerCondition::BeginningOfUpkeep(PlayerRef::Each),
        gain_life(1),
    )));
    let bell_id = setup.add_card(bell);
    setup.fill(P0, bell_id, 5);
    setup.fill(P1, bell_id, 5);

    let mut game = setup.build_passing();
    put_on_battlefield(&mut game, "Sino de Teste", P0);
    put_on_battlefield(&mut game, "Sino de Teste", P1);
    // O jogador ativo é o não-ativo por padrão: inverter deixa claro que a
    // ordem sai da APNAP e não da ordem em que os permanentes entraram.
    set_active(&mut game, P1);
    clear_events(&mut game);

    game.state.emit(GameEvent::StepBegan { step: Step::Upkeep });
    triggers::collect(&mut game);
    assert_eq!(
        game.state.pending_triggers.len(),
        2,
        "os dois sinos disparam na mesma manutenção"
    );

    stack::put_triggers_on_stack(&mut game);

    let controllers: Vec<PlayerId> = game.state.stack.iter().map(|i| i.controller).collect();
    assert_eq!(
        controllers,
        vec![P1, P0],
        "CR 603.3b: o jogador ativo (P1) põe o dele primeiro; o do oponente fica por cima"
    );
    let Some(top) = stack::peek(&game) else {
        panic!("a pilha tinha de ter dois gatilhos")
    };
    assert_eq!(
        top.controller, P0,
        "o último a entrar é o topo e resolve primeiro"
    );
}

/// CR 603.4 — condição de intervenção falsa impede o disparo.
#[test]
fn condicao_de_intervencao_falsa_impede_o_disparo() {
    let mut setup = Setup::empty();
    let mut warden = creature_def("Guarda Condicional", 1, 1);
    let mut ability = triggered(
        TriggerCondition::EntersBattlefield(Selector::creatures()),
        gain_life(1),
    );
    ability.intervening_if = Condition::YouControlAtLeast(3, Filter::creature());
    warden.abilities.push(Ability::Triggered(ability));
    let warden_id = setup.add_card(warden);
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.deck(P0, &[warden_id]);
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();

    put_on_battlefield(&mut game, "Guarda Condicional", P0);
    triggers::collect(&mut game);
    assert!(
        game.state.pending_triggers.is_empty(),
        "CR 603.4: com uma criatura só, a condição é falsa e nada dispara"
    );

    put_on_battlefield(&mut game, "Urso de Teste", P0);
    triggers::collect(&mut game);
    assert!(
        game.state.pending_triggers.is_empty(),
        "CR 603.4: com duas criaturas a condição ainda é falsa"
    );

    put_on_battlefield(&mut game, "Urso de Teste", P0);
    triggers::collect(&mut game);
    assert_eq!(
        game.state.pending_triggers.len(),
        1,
        "com três criaturas a condição passa a ser verdadeira e o gatilho dispara"
    );
}

/// CR 603.5 — "você pode": recusar o gatilho não faz nada.
#[test]
fn gatilho_opcional_recusado_nao_faz_nada() {
    let mut setup = Setup::empty();
    let mut helper = creature_def("Ajudante Opcional", 1, 1);
    let mut ability = triggered(
        TriggerCondition::EntersBattlefield(Selector::creatures()),
        gain_life(5),
    );
    ability.optional = true;
    helper.abilities.push(Ability::Triggered(ability));
    let helper_id = setup.add_card(helper);
    setup.fill(P0, helper_id, 5);
    setup.fill(P1, helper_id, 5);

    let run = |answer: bool, setup: &Setup| -> (i32, i32, usize) {
        let asked = counter();
        let seen = asked.clone();
        let agents = vec![
            ScriptedAgent::new("A", move |request, legal| {
                if matches!(request, Request::ConfirmOptional { .. }) {
                    match seen.lock() {
                        Ok(mut g) => *g += 1,
                        Err(e) => panic!("contador envenenado: {e}"),
                    }
                    return Action::Confirm { yes: answer };
                }
                default_action(legal)
            })
            .boxed(),
            FixedAgent::passing("B").boxed(),
        ];
        let mut game = setup.build(agents);
        let before = life(&game, P0);
        put_on_battlefield(&mut game, "Ajudante Opcional", P0);
        triggers::collect(&mut game);
        stack::put_triggers_on_stack(&mut game);
        assert_eq!(game.state.stack.len(), 1, "o gatilho opcional foi para a pilha");
        stack::resolve_top(&mut game);
        (before, life(&game, P0), count_of(&asked))
    };

    let (before_no, after_no, asked_no) = run(false, &setup);
    assert_eq!(asked_no, 1, "CR 603.5: a escolha é feita na resolução");
    assert_eq!(
        after_no, before_no,
        "gatilho opcional recusado não pode causar efeito nenhum"
    );

    let (before_yes, after_yes, asked_yes) = run(true, &setup);
    assert_eq!(asked_yes, 1);
    assert_eq!(
        after_yes - before_yes,
        5,
        "aceito, o mesmo gatilho tem de fazer efeito — senão a recusa não provou nada"
    );
}

/// Gatilho "uma vez por turno" não dispara duas vezes no mesmo turno.
#[test]
fn uma_vez_por_turno_nao_dispara_duas_vezes() {
    let mut setup = Setup::empty();
    let mut watcher = enchantment_def("Vigia de Teste");
    let mut ability = triggered(
        TriggerCondition::EntersBattlefield(Selector::creatures()),
        gain_life(1),
    );
    ability.once_per_turn = true;
    watcher.abilities.push(Ability::Triggered(ability));
    let watcher_id = setup.add_card(watcher);
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.deck(P0, &[watcher_id]);
    setup.fill(P0, bear, 5);
    setup.fill(P1, bear, 5);

    let mut game = setup.build_passing();
    put_on_battlefield(&mut game, "Vigia de Teste", P0);
    game.state.event_queue.clear();

    put_on_battlefield(&mut game, "Urso de Teste", P0);
    triggers::collect(&mut game);
    assert_eq!(
        game.state.pending_triggers.len(),
        1,
        "a primeira criatura do turno dispara o gatilho"
    );
    game.state.pending_triggers.clear();

    put_on_battlefield(&mut game, "Urso de Teste", P0);
    triggers::collect(&mut game);
    assert!(
        game.state.pending_triggers.is_empty(),
        "uma vez por turno: a segunda entrada no mesmo turno não dispara"
    );

    // Turno novo zera o registro de disparos.
    game.state.emit(GameEvent::TurnBegan {
        player: P0,
        turn: 2,
    });
    triggers::collect(&mut game);
    assert!(game.state.pending_triggers.is_empty());

    put_on_battlefield(&mut game, "Urso de Teste", P0);
    triggers::collect(&mut game);
    assert_eq!(
        game.state.pending_triggers.len(),
        1,
        "no turno seguinte o gatilho volta a estar disponível"
    );
}

// ---------------------------------------------------------------------------
// Sanidade do próprio ferramental — teste que passa sem afirmar nada é pior
// que teste ausente, e o helper é o lugar mais fácil de isso acontecer.
// ---------------------------------------------------------------------------

#[test]
fn ferramental_de_teste_monta_estado_previsivel() {
    let mut setup = Setup::empty();
    let bear = setup.add_card(creature_def("Urso de Teste", 2, 2));
    setup.fill(P0, bear, 4);
    setup.fill(P1, bear, 4);

    let mut game = setup.build_passing();
    assert!(hand(&game, P0).is_empty(), "mão inicial de zero cartas");
    assert_eq!(library(&game, P0).len(), 4);

    let id = put_on_battlefield(&mut game, "Urso de Teste", P0);
    assert!(on_battlefield(&game, id));
    assert_eq!(library(&game, P0).len(), 3);
    assert_eq!(pt(&game, id), (2, 2));

    let ctx: EvalCtx = eval_ctx(id, P0, vec![TargetChoice::Object(id)]);
    assert_eq!(ctx.source, Some(id));
    assert_eq!(ctx.controller, P0);
}
