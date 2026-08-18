//! Itens 61–65 de `docs/RULES_TESTS.md`: determinismo e robustez do simulador.
//!
//! Os testes pesados estão marcados com `#[ignore]` porque rodam centenas de
//! partidas completas e o catálogo inteiro. Para rodá-los:
//!
//! ```text
//! cargo test -p mtg-core --test fuzz -- --ignored --nocapture
//! ```
//!
//! O `--nocapture` importa: as métricas (taxa de empate, cartas reprovadas) são
//! impressas, não só afirmadas.

mod support;

use support::*;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use mtg_core::action::{Action, Request};
use mtg_core::card::CardDatabase;
use mtg_core::engine::{cast, stack, turn, Agent, FirstLegalAgent, Game, GameConfig};
use mtg_core::event::Step;
use mtg_core::ids::{CardDefId, ObjectId, PlayerId};
use mtg_core::state::GameOutcome;
use mtg_core::zone::ZoneId;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

/// Partidas rodadas nos testes de robustez: sementes 0..GAMES-1.
const GAMES: u64 = 200;
/// Teto tolerado de empates por teto de turnos, em porcentagem. Empate não é
/// sucesso: é métrica, e métrica precisa de limite explícito.
///
/// O número é 50% por um motivo defensável e não retroajustado: os bots deste
/// teste jogam ao acaso, então parte das partidas longas é o bot sendo ruim, e
/// não o motor travando. Acima da metade, porém, não existe leitura benigna —
/// seria o motor não progredindo. O empate por trava de segurança (laço de
/// prioridade, limite de decisões) é medido à parte, com tolerância zero.
const MAX_DRAW_RATE: f64 = 50.0;
/// Objeto que não existe: usado para forjar uma ação garantidamente ilegal.
const GHOST: ObjectId = ObjectId(u32::MAX - 5);

const DECK_NAMES: [&str; 4] = [
    "Goblin Onslaught",
    "Azorius Control",
    "Selesnya Valor",
    "Gruul Stampede",
];

fn fuzz_config() -> GameConfig {
    GameConfig {
        starting_life: 20,
        starting_hand_size: 7,
        allow_mulligan: true,
        max_turns: 40,
        max_decisions: 60_000,
    }
}

/// Pares de deck variados por semente: rodar 200 vezes o mesmo confronto
/// exercitaria sempre o mesmo punhado de cartas.
fn deck_pair(db: &CardDatabase, seed: u64) -> (Vec<CardDefId>, Vec<CardDefId>) {
    let expand = |name: &str| match mtg_cards::deck_by_name(db, name) {
        Some(d) => d,
        None => panic!("deck '{name}' não existe ou cita carta fora do catálogo"),
    };
    (
        expand(DECK_NAMES[(seed % 4) as usize]),
        expand(DECK_NAMES[((seed / 4 + 1) % 4) as usize]),
    )
}

fn random_agents(seed: u64) -> Vec<Box<dyn Agent>> {
    vec![
        Box::new(RandomAgent::new("alice", seed)),
        Box::new(RandomAgent::new("bob", seed ^ 0x5DEE_CE66)),
    ]
}

/// Como a partida terminou, do ponto de vista de quem mede robustez.
struct Finish {
    outcome: GameOutcome,
    turn: u32,
    /// Motivo registrado por `turn::force_draw`, quando houve empate.
    draw_reason: Option<String>,
}

/// Roda uma partida com bots aleatórios.
fn play_random(db: Arc<CardDatabase>, seed: u64) -> Finish {
    let (a, b) = deck_pair(&db, seed);
    let mut game = game_with_catalog(db, a, b, random_agents(seed), fuzz_config(), seed);
    let outcome = game.run();
    let draw_reason = game
        .state
        .log
        .iter()
        .rev()
        .find_map(|e| e.text.strip_prefix("empate: ").map(|s| s.to_string()));
    Finish {
        outcome,
        turn: game.state.turn,
        draw_reason,
    }
}

fn describe(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "pânico sem mensagem legível".to_string()
}

// ---------------------------------------------------------------------------
// 61. Determinismo
// ---------------------------------------------------------------------------

#[test]
fn mesma_semente_produz_partida_identica() {
    // Determinismo por semente é requisito do projeto: sem ele não há replay
    // nem depuração de partida. Comparar o log inteiro, e não só o placar, é o
    // que pega a divergência que o resultado esconde.
    let db = catalog();

    for seed in [0u64, 1, 7, 19, 123] {
        let (a, b) = deck_pair(&db, seed);
        let mut first = game_with_catalog(
            Arc::clone(&db),
            a.clone(),
            b.clone(),
            random_agents(seed),
            fuzz_config(),
            seed,
        );
        let mut second =
            game_with_catalog(Arc::clone(&db), a, b, random_agents(seed), fuzz_config(), seed);

        let outcome_a = first.run();
        let outcome_b = second.run();

        assert_eq!(outcome_a, outcome_b, "semente {seed}: resultados divergem");
        assert_eq!(
            first.state.turn, second.state.turn,
            "semente {seed}: turno final diverge"
        );
        assert!(
            !first.state.log.is_empty(),
            "semente {seed}: log vazio — a comparação seria vácua"
        );
        assert_eq!(
            first.state.log.len(),
            second.state.log.len(),
            "semente {seed}: tamanho do log diverge"
        );
        for (i, (x, y)) in first
            .state
            .log
            .iter()
            .zip(second.state.log.iter())
            .enumerate()
        {
            assert_eq!(
                x, y,
                "semente {seed}: log diverge na entrada {i}\nA: {x:?}\nB: {y:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 62 e 63. Robustez em massa
// ---------------------------------------------------------------------------

/// Motivo de empate que é regra de simulação, não defeito: a partida durou mais
/// que `max_turns`. Qualquer outro motivo vem de uma trava de segurança do
/// motor (laço de prioridade, limite de decisões, limpeza que não estabiliza) e
/// é bug, não lentidão.
const TURN_CAP_REASON: &str = "limite de turnos atingido";

/// Resultado agregado de uma bateria de partidas.
struct Suite {
    winners: usize,
    /// Empates por ter batido no teto de turnos: partida lenta, não defeito.
    turn_cap_draws: usize,
    /// Empates por trava de segurança do motor: defeito, tolerância zero.
    guard_draws: Vec<String>,
    panics: Vec<String>,
    ongoing: Vec<u64>,
    over_limit: Vec<u64>,
}

fn run_suite(db: Arc<CardDatabase>) -> Suite {
    let mut suite = Suite {
        winners: 0,
        turn_cap_draws: 0,
        guard_draws: Vec::new(),
        panics: Vec::new(),
        ongoing: Vec::new(),
        over_limit: Vec::new(),
    };
    let limit = fuzz_config().max_turns;

    for seed in 0..GAMES {
        let handle = Arc::clone(&db);
        let finish = match catch_unwind(AssertUnwindSafe(move || play_random(handle, seed))) {
            Ok(finish) => finish,
            Err(payload) => {
                suite
                    .panics
                    .push(format!("semente {seed}: {}", describe(payload.as_ref())));
                continue;
            }
        };
        if finish.turn > limit + 1 {
            suite.over_limit.push(seed);
        }
        match finish.outcome {
            GameOutcome::Winner(_) => suite.winners += 1,
            GameOutcome::Draw => match finish.draw_reason.as_deref() {
                Some(TURN_CAP_REASON) => suite.turn_cap_draws += 1,
                other => suite.guard_draws.push(format!(
                    "semente {seed}: {}",
                    other.unwrap_or("empate sem motivo registrado")
                )),
            },
            GameOutcome::Ongoing => suite.ongoing.push(seed),
        }
    }
    suite
}

impl Suite {
    fn draws(&self) -> usize {
        self.turn_cap_draws + self.guard_draws.len()
    }
}

#[test]
#[ignore = "lento: 200 partidas completas; rode com --ignored"]
fn nenhuma_partida_entra_em_panico() {
    // Item 63: 200 partidas, sementes 0..199, bots aleatórios. A afirmação é
    // sobre o NÚMERO DE PÂNICOS, não sobre "terminou de algum jeito".
    let suite = run_suite(catalog());

    println!(
        "[63] {GAMES} partidas: {} com vencedor, {} empates, {} pânicos",
        suite.winners,
        suite.draws(),
        suite.panics.len()
    );
    assert_eq!(
        suite.panics.len(),
        0,
        "o motor entrou em pânico em {} partida(s):\n{}",
        suite.panics.len(),
        suite.panics.join("\n")
    );
    assert_eq!(
        suite.winners + suite.draws(),
        GAMES as usize,
        "nem toda partida chegou a um resultado; em andamento: {:?}",
        suite.ongoing
    );
}

#[test]
#[ignore = "lento: 200 partidas completas; rode com --ignored"]
fn partida_completa_termina_dentro_do_limite_de_turnos() {
    // Item 62: nenhuma partida pode estourar `max_turns` sem resultado. O
    // empate por teto é métrica reportada contra um limite explícito — não um
    // sucesso silencioso.
    let suite = run_suite(catalog());
    let total = GAMES as usize;
    let draw_rate = 100.0 * suite.turn_cap_draws as f64 / total as f64;

    println!(
        "[62] {total} partidas: {} com vencedor ({:.1}%), {} empates por teto de turnos ({draw_rate:.1}%, teto tolerado {MAX_DRAW_RATE:.0}%), {} empates por trava de segurança",
        suite.winners,
        100.0 * suite.winners as f64 / total as f64,
        suite.turn_cap_draws,
        suite.guard_draws.len()
    );

    assert_eq!(
        suite.panics.len(),
        0,
        "pânico invalida a medição:\n{}",
        suite.panics.join("\n")
    );
    assert!(
        suite.ongoing.is_empty(),
        "partidas sem resultado (semente): {:?}",
        suite.ongoing
    );
    assert!(
        suite.over_limit.is_empty(),
        "partidas que passaram de max_turns (semente): {:?}",
        suite.over_limit
    );
    // Trava de segurança disparada é defeito do motor, não partida lenta:
    // tolerância zero, sem margem discutível.
    assert!(
        suite.guard_draws.is_empty(),
        "{} partida(s) empataram por trava de segurança do motor:\n{}",
        suite.guard_draws.len(),
        suite.guard_draws.join("\n")
    );
    assert!(
        draw_rate <= MAX_DRAW_RATE,
        "{} empates por teto de turnos ({draw_rate:.1}%) passa do teto tolerado de {MAX_DRAW_RATE:.0}%",
        suite.turn_cap_draws
    );
}

// ---------------------------------------------------------------------------
// 64. Ação ilegal de agente
// ---------------------------------------------------------------------------

/// Agente que só devolve ação inválida. `Game::ask` tem de descartá-la e cair
/// na primeira ação legal — exatamente o que `FirstLegalAgent` faria.
struct IllegalAgent;

impl Agent for IllegalAgent {
    fn name(&self) -> &str {
        "illegal"
    }
    fn decide(&mut self, _game: &Game, _request: &Request, _legal: &[Action]) -> Action {
        Action::PlayLand { object: GHOST }
    }
}

#[test]
fn acao_ilegal_de_agente_e_descartada_sem_corromper_estado() {
    let db = catalog();
    let seed = 11u64;
    let (a, b) = deck_pair(&db, seed);

    let rogue_agents: Vec<Box<dyn Agent>> = vec![Box::new(IllegalAgent), Box::new(IllegalAgent)];
    let mut rogue = game_with_catalog(
        Arc::clone(&db),
        a.clone(),
        b.clone(),
        rogue_agents,
        fuzz_config(),
        seed,
    );
    let rogue_outcome = rogue.run();

    // Referência: mesmo estado inicial conduzido pelo comportamento que o motor
    // usa como descarte — se a ação ilegal foi mesmo ignorada, os dois logs
    // coincidem entrada por entrada.
    let honest_agents: Vec<Box<dyn Agent>> =
        vec![Box::new(FirstLegalAgent), Box::new(FirstLegalAgent)];
    let mut honest = game_with_catalog(db, a, b, honest_agents, fuzz_config(), seed);
    let honest_outcome = honest.run();

    let rejections = rogue
        .state
        .log
        .iter()
        .filter(|e| e.text.starts_with("ação ilegal do agente descartada"))
        .count();
    assert!(
        rejections > 0,
        "nenhuma ação ilegal foi rejeitada — o teste não exercitou o caminho que pretende testar"
    );

    assert!(
        !matches!(rogue_outcome, GameOutcome::Ongoing),
        "partida com agente ilegal não terminou"
    );
    assert_eq!(
        rogue_outcome, honest_outcome,
        "a ação ilegal mudou o resultado da partida"
    );
    assert_eq!(
        rogue.state.turn, honest.state.turn,
        "a ação ilegal mudou o número de turnos"
    );

    let cleaned: Vec<_> = rogue
        .state
        .log
        .iter()
        .filter(|e| !e.text.starts_with("ação ilegal do agente descartada"))
        .collect();
    let reference: Vec<_> = honest.state.log.iter().collect();
    assert_eq!(
        cleaned.len(),
        reference.len(),
        "descontadas as rejeições, o log tinha de ser idêntico ao da referência"
    );
    for (i, (x, y)) in cleaned.iter().zip(reference.iter()).enumerate() {
        assert_eq!(x, y, "log diverge na entrada {i}\nrogue: {x:?}\nref:   {y:?}");
    }

    assert_zone_bookkeeping(&rogue);
}

// ---------------------------------------------------------------------------
// 65. Catálogo inteiro lançável
// ---------------------------------------------------------------------------

/// Cenário mínimo que dá alvo legal a tudo que o catálogo pede: criatura
/// terrestre, criatura voadora, artefato, encantamento e terreno, dos dois
/// lados, mais uma criatura no cemitério para as mágicas de reanimação.
const FIXTURES: [&str; 5] = [
    "Grizzly Bears",
    "Wind Drake",
    "Sol Ring",
    "Glorious Anthem",
    "Forest",
];
/// Cópias de cada carta-cenário no deck. O cenário usa 6 no pior caso.
const FIXTURE_COPIES: usize = 8;
/// Mana posto no pool: o "mana abundante" que o item 65 pede.
const ABUNDANT_MANA: u16 = 30;

fn catalog_deck(db: &CardDatabase) -> Vec<CardDefId> {
    let mut deck: Vec<CardDefId> = db.cards.iter().map(|c| c.id).collect();
    for name in FIXTURES {
        let Some(id) = db.id_by_name(name) else {
            panic!("carta-cenário '{name}' sumiu do catálogo");
        };
        for _ in 0..FIXTURE_COPIES {
            deck.push(id);
        }
    }
    deck
}

fn stage_fixtures(game: &mut Game) {
    for player in [P0, P1] {
        for name in FIXTURES {
            put_ready(game, name, player);
        }
    }
    // Alvo de reanimação: carta de criatura no cemitério.
    put_in_graveyard(game, "Grizzly Bears", P0);

    // "Assassinate" pede criatura virada e "Divine Verdict" pede criatura
    // atacante ou bloqueando. Sem estes dois estados o cenário reprovaria duas
    // cartas corretas por falta de alvo, e não por defeito do motor.
    let tapped = put_ready(game, "Grizzly Bears", P1);
    tap(game, tapped);
    let attacker = put_ready(game, "Wind Drake", P1);
    set_attacking(game, attacker, P0);

    fill_mana_pool(game, P0, ABUNDANT_MANA);
    goto_step(game, Step::PrecombatMain);
    clear_pending_events(game);
}

/// Põe uma mágica do oponente na pilha, para que contramágicas tenham alvo.
fn stack_opposing_spell(game: &mut Game, name: &str) {
    let id = put_in_hand(game, name, P1);
    turn::move_object(game, id, ZoneId::STACK);
    stack::push_spell(game, id, P1, Vec::new(), 0, Vec::new());
}

/// Primeira ação de prioridade que joga ou lança este objeto.
fn action_for(game: &Game, object: ObjectId) -> Option<Action> {
    cast::priority_actions(game, P0)
        .into_iter()
        .find(|a| match a {
            Action::CastSpell { object: o, .. } | Action::PlayLand { object: o } => *o == object,
            _ => false,
        })
}

/// Monta o cenário, lança a carta e resolve. `Err` traz o motivo da reprovação.
fn try_card(db: Arc<CardDatabase>, def: CardDefId) -> Result<(), String> {
    let deck = catalog_deck(&db);
    let agents: Vec<Box<dyn Agent>> = vec![Box::new(FirstLegalAgent), Box::new(FirstLegalAgent)];
    let mut game = game_with_catalog(
        db,
        deck.clone(),
        deck,
        agents,
        GameConfig {
            starting_life: 20,
            starting_hand_size: 0,
            allow_mulligan: false,
            max_turns: 60,
            max_decisions: 200_000,
        },
        4242,
    );
    stage_fixtures(&mut game);
    let subject = put_in_hand_by_def(&mut game, def, P0);
    clear_pending_events(&mut game);

    // Passo 1: pilha vazia — cobre terreno, permanente, feitiço e instantâneo.
    let mut action = action_for(&game, subject);
    // Passo 2: com mágica do oponente na pilha — cobre contramágica. Duas iscas
    // porque "contra mágica de criatura" e "contra mágica que não seja de
    // criatura" pedem coisas diferentes.
    for bait in ["Grizzly Bears", "Divination"] {
        if action.is_some() {
            break;
        }
        stack_opposing_spell(&mut game, bait);
        clear_pending_events(&mut game);
        action = action_for(&game, subject);
        if action.is_none() {
            resolve_stack(&mut game);
            clear_pending_events(&mut game);
        }
    }

    let Some(action) = action else {
        return Err(
            "não apareceu nas ações legais nem com pilha vazia nem com mágica na pilha".to_string(),
        );
    };

    if let Err(err) = cast::execute(&mut game, P0, action) {
        return Err(format!("execute recusou: {err}"));
    }
    if turn::zone_objects(&game, ZoneId::hand(P0)).contains(&subject) {
        return Err("continuou na mão depois de lançada".to_string());
    }

    resolve_stack(&mut game);
    assert_zone_bookkeeping(&game);
    Ok(())
}

#[test]
#[ignore = "lento: monta uma partida por carta do catálogo; rode com --ignored"]
fn todas_as_cartas_do_catalogo_sao_lancaveis() {
    let db = catalog();
    let total = db.cards.len();
    assert!(total > 0, "catálogo vazio: o teste não afirmaria nada");

    let mut failures: Vec<String> = Vec::new();
    for card in &db.cards {
        let name = card.name.clone();
        let def = card.id;
        let handle = Arc::clone(&db);
        match catch_unwind(AssertUnwindSafe(move || try_card(handle, def))) {
            Ok(Ok(())) => {}
            Ok(Err(reason)) => failures.push(format!("{name}: {reason}")),
            Err(payload) => {
                failures.push(format!("{name}: PÂNICO — {}", describe(payload.as_ref())))
            }
        }
    }

    println!(
        "[65] {}/{total} cartas lançáveis e resolvidas sem erro",
        total - failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} de {total} cartas do catálogo reprovaram:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
