//! Teste de fumaça: partida completa bot vs bot com cartas e decks reais.
//!
//! O que ele protege: o motor precisa TERMINAR. Loop infinito de prioridade,
//! gatilho que se redispara e pânico em caminho normal são as três falhas que
//! matam um simulador automático, e nenhuma delas aparece em teste unitário de
//! módulo isolado — só numa partida inteira.
//!
//! Determinismo por semente também é verificado aqui: mesma semente tem de dar
//! exatamente o mesmo resultado e o mesmo log, senão replay e depuração de
//! partida ficam impossíveis.
use std::sync::Arc;

use mtg_core::action::{Action, Request};
use mtg_core::card::CardDatabase;
use mtg_core::engine::{Agent, Game, GameConfig, PlayerConfig};
use mtg_core::ids::CardDefId;
use mtg_core::state::GameOutcome;

/// Agente trivial e determinístico: escolhe pelo resto da divisão de um
/// contador linear-congruente próprio.
///
/// Por que não usar `game.rng`: `Agent::decide` só recebe `&Game`, e é de
/// propósito — o motor nunca cede mutabilidade a um agente. A semente do bot é
/// função pura da semente da partida, então a partida inteira continua
/// determinística.
struct DiceAgent {
    name: String,
    state: u64,
}

impl DiceAgent {
    fn new(name: &str, seed: u64) -> DiceAgent {
        DiceAgent { name: name.to_string(), state: seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1) }
    }

    fn next(&mut self) -> u64 {
        // LCG de Knuth: barato, reprodutível e suficiente para variar escolha.
        self.state = self.state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.state >> 33
    }
}

impl Agent for DiceAgent {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(&mut self, _game: &Game, _request: &Request, legal: &[Action]) -> Action {
        // `Game::ask` já garante lista não-vazia, mas o agente não deve confiar
        // no chamador: devolver `PassPriority` é sempre seguro.
        if legal.is_empty() {
            return Action::PassPriority;
        }
        let idx = (self.next() as usize) % legal.len();
        match legal.get(idx) {
            Some(a) => a.clone(),
            None => Action::PassPriority,
        }
    }
}

/// Agente que sempre toma a ação mais "avançada" da lista. Serve de contraste
/// ao `DiceAgent`: exercita caminhos que a escolha aleatória raramente cobre.
struct LastLegalAgent;

impl Agent for LastLegalAgent {
    fn name(&self) -> &str {
        "last-legal"
    }
    fn decide(&mut self, _game: &Game, _request: &Request, legal: &[Action]) -> Action {
        legal.last().cloned().unwrap_or(Action::PassPriority)
    }
}

/// Catálogo real, carregado uma vez. `expect` é aceitável aqui: é setup de
/// teste, não caminho de jogo — se o catálogo não carrega não há o que testar.
fn database() -> Arc<CardDatabase> {
    Arc::new(mtg_cards::build_database().expect("catálogo de cartas deve carregar"))
}

fn deck(db: &CardDatabase, name: &str) -> Vec<CardDefId> {
    match mtg_cards::deck_by_name(db, name) {
        Some(d) => d,
        None => panic!("deck '{name}' não existe ou cita carta fora do catálogo"),
    }
}

fn config() -> GameConfig {
    GameConfig {
        starting_life: 20,
        starting_hand_size: 7,
        allow_mulligan: true,
        // Curto de propósito: se o motor precisa de mais que isso para fechar
        // uma partida entre dois decks de 60, há algo travado.
        max_turns: 40,
        max_decisions: 60_000,
    }
}

/// Roda uma partida e devolve `(resultado, turno final, tamanho do log)`.
fn play(db: Arc<CardDatabase>, deck_a: Vec<CardDefId>, deck_b: Vec<CardDefId>, seed: u64) -> (GameOutcome, u32, usize) {
    let players = vec![
        PlayerConfig { name: "Alice".to_string(), deck: deck_a },
        PlayerConfig { name: "Bob".to_string(), deck: deck_b },
    ];
    let agents: Vec<Box<dyn Agent>> = vec![
        Box::new(DiceAgent::new("alice", seed)),
        Box::new(DiceAgent::new("bob", seed ^ 0x5DEE_CE66)),
    ];
    let mut game = match Game::new(db, players, agents, config(), seed) {
        Ok(g) => g,
        Err(err) => panic!("Game::new falhou com semente {seed}: {err:?}"),
    };
    let outcome = game.run();
    (outcome, game.state.turn, game.state.log.len())
}

#[test]
fn partida_termina_em_vinte_sementes() {
    let db = database();
    let a = deck(&db, "Goblin Onslaught");
    let b = deck(&db, "Azorius Control");

    for seed in 0..20u64 {
        let (outcome, turn, log_len) = play(Arc::clone(&db), a.clone(), b.clone(), seed);

        assert!(
            !matches!(outcome, GameOutcome::Ongoing),
            "semente {seed}: partida não terminou (turno {turn})"
        );
        assert!(
            turn <= config().max_turns + 1,
            "semente {seed}: turno {turn} passou de max_turns"
        );
        assert!(log_len > 0, "semente {seed}: log vazio, motor não registrou nada");
    }
}

#[test]
fn todos_os_pares_de_deck_terminam() {
    let db = database();
    let names = ["Goblin Onslaught", "Azorius Control", "Selesnya Valor", "Gruul Stampede"];

    for (i, left) in names.iter().enumerate() {
        for right in names.iter().skip(i) {
            let a = deck(&db, left);
            let b = deck(&db, right);
            let seed = (i as u64) * 7 + 3;
            let (outcome, turn, _) = play(Arc::clone(&db), a, b, seed);
            assert!(
                !matches!(outcome, GameOutcome::Ongoing),
                "{left} x {right} (semente {seed}) não terminou no turno {turn}"
            );
        }
    }
}

#[test]
fn mesma_semente_da_mesma_partida() {
    let db = database();
    let a = deck(&db, "Gruul Stampede");
    let b = deck(&db, "Selesnya Valor");

    for seed in [0u64, 1, 7, 19] {
        let first = play(Arc::clone(&db), a.clone(), b.clone(), seed);
        let second = play(Arc::clone(&db), a.clone(), b.clone(), seed);
        assert_eq!(first, second, "semente {seed}: partida não é determinística");
    }
}

#[test]
fn sementes_diferentes_divergem() {
    let db = database();
    let a = deck(&db, "Goblin Onslaught");
    let b = deck(&db, "Gruul Stampede");

    // Não exigimos que TODA semente difira — duas partidas podem coincidir por
    // acaso. Exigimos que o conjunto não seja constante, que é o sintoma de
    // semente ignorada em algum lugar do motor.
    let runs: Vec<_> = (0..8u64).map(|s| play(Arc::clone(&db), a.clone(), b.clone(), s)).collect();
    let all_equal = runs.windows(2).all(|w| w[0] == w[1]);
    assert!(!all_equal, "todas as sementes deram partida idêntica: rng não está sendo usado");
}

#[test]
fn agentes_degenerados_nao_travam_o_motor() {
    // Agente que sempre passa e agente que sempre pega a última ação são os
    // dois extremos que costumam expor laço infinito de prioridade.
    let db = database();
    let players = vec![
        PlayerConfig { name: "Passiva".to_string(), deck: deck(&db, "Azorius Control") },
        PlayerConfig { name: "Gulosa".to_string(), deck: deck(&db, "Goblin Onslaught") },
    ];
    let agents: Vec<Box<dyn Agent>> = vec![
        Box::new(mtg_core::engine::FirstLegalAgent),
        Box::new(LastLegalAgent),
    ];
    let mut game = match Game::new(db, players, agents, config(), 42) {
        Ok(g) => g,
        Err(err) => panic!("Game::new falhou: {err:?}"),
    };
    let outcome = game.run();
    assert!(!matches!(outcome, GameOutcome::Ongoing), "motor travou com agentes degenerados");
}
