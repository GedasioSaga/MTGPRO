//! Bot provisório usado pelo servidor até `mtg-ai` publicar uma
//! implementação de `Agent` de verdade — hoje esse crate está vazio,
//! sendo escrito em paralelo.
//!
//! `Agent::decide` só recebe `&Game` (o motor nunca cede mutabilidade a um
//! agente), então não há como usar `game.rng` aqui — cada bot carrega o
//! próprio `ChaCha8Rng`, semeado a partir da semente da partida, e ainda
//! assim a partida inteira permanece determinística por semente porque a
//! semente do bot é função pura da semente da partida.
use mtg_core::engine::{Agent, Game};
use mtg_core::state::GameOutcome;
use mtg_core::{Action, Request};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub struct SeededBot {
    name: String,
    rng: ChaCha8Rng,
}

impl SeededBot {
    pub fn new(name: impl Into<String>, seed: u64) -> Self {
        SeededBot { name: name.into(), rng: ChaCha8Rng::seed_from_u64(seed) }
    }
}

impl Agent for SeededBot {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(&mut self, _game: &Game, _request: &Request, legal: &[Action]) -> Action {
        // `Game::ask` já garante `legal` não-vazio antes de chamar o agente,
        // mas o `Agent` não deve confiar nisso sem checar — chamador pode
        // mudar no futuro.
        if legal.is_empty() {
            return Action::PassPriority;
        }
        let idx = self.rng.gen_range(0..legal.len());
        legal[idx].clone()
    }

    fn on_game_end(&mut self, _game: &Game, _outcome: GameOutcome) {}
}
