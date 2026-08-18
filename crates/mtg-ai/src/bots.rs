//! Fábrica de agentes e a linha de base aleatória.
//!
//! `Agent::decide` só recebe `&Game` — o motor nunca cede mutabilidade a um
//! agente —, então nenhum bot pode usar `game.rng`. Cada bot carrega o próprio
//! `ChaCha8Rng`, semeado a partir da semente da partida. A partida inteira
//! continua determinística porque a semente do bot é função pura da semente da
//! partida, e nenhum bot lê relógio, endereço ou ordem de `HashMap`.
use mtg_core::action::{Action, Request};
use mtg_core::engine::{Agent, Game};
use mtg_core::state::GameOutcome;
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub use crate::greedy::GreedyBot;
pub use crate::heuristic::HeuristicBot;

/// Nomes aceitos por `bot_by_name`, na ordem em que uma interface deve
/// oferecê-los (do mais fraco ao mais forte).
pub const BOT_NAMES: [&str; 3] = ["random", "heuristic", "greedy"];

/// Nome do bot usado quando nada é pedido.
pub const DEFAULT_BOT: &str = "heuristic";

/// Constrói um agente pelo nome. `None` quando o nome não é conhecido — quem
/// chama decide se cai no padrão ou se recusa o pedido.
pub fn bot_by_name(name: &str, seed: u64) -> Option<Box<dyn Agent>> {
    match name.trim().to_ascii_lowercase().as_str() {
        "random" | "aleatorio" | "rng" => Some(Box::new(RandomBot::new(seed))),
        "heuristic" | "heuristico" => Some(Box::new(HeuristicBot::new(seed))),
        "greedy" | "guloso" => Some(Box::new(GreedyBot::new(seed))),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Linha de base
// ---------------------------------------------------------------------------

/// Sorteia uniformemente entre as ações legais. Serve de piso: um bot que não
/// vence este com folga não está decidindo nada.
pub struct RandomBot {
    rng: ChaCha8Rng,
}

impl RandomBot {
    pub fn new(seed: u64) -> RandomBot {
        RandomBot {
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }
}

impl Agent for RandomBot {
    fn name(&self) -> &str {
        "random"
    }

    fn decide(&mut self, _game: &Game, _request: &Request, legal: &[Action]) -> Action {
        // `Game::ask` já garante lista não-vazia antes de chamar o agente, mas
        // o agente não deve confiar num invariante do chamador.
        if legal.is_empty() {
            return Action::PassPriority;
        }
        let index = self.rng.gen_range(0..legal.len());
        match legal.get(index) {
            Some(action) => action.clone(),
            None => Action::PassPriority,
        }
    }

    fn on_game_end(&mut self, _game: &Game, _outcome: GameOutcome) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabrica_reconhece_os_tres_nomes() {
        for name in BOT_NAMES {
            let bot = bot_by_name(name, 7).unwrap_or_else(|| panic!("nome {name} não construiu"));
            assert_eq!(bot.name(), name);
        }
        assert_eq!(bot_by_name(DEFAULT_BOT, 1).map(|b| b.name().to_string()), Some("heuristic".to_string()));
    }

    #[test]
    fn fabrica_ignora_caixa_e_espaco() {
        assert!(bot_by_name("  HEURISTIC ", 1).is_some());
        assert!(bot_by_name("Greedy", 1).is_some());
        assert!(bot_by_name("nao-existe", 1).is_none());
    }

    #[test]
    fn aleatorio_com_a_mesma_semente_da_a_mesma_sequencia() {
        // Determinismo por semente é requisito: dois bots semeados igual
        // precisam sortear o mesmo índice na mesma ordem.
        let mut a = RandomBot::new(99);
        let mut b = RandomBot::new(99);
        let mut c = RandomBot::new(100);
        let mut diverged = false;
        for _ in 0..64 {
            let x = a.rng.gen_range(0..8);
            assert_eq!(x, b.rng.gen_range(0..8));
            if x != c.rng.gen_range(0..8) {
                diverged = true;
            }
        }
        assert!(diverged, "sementes diferentes produziram a mesma sequência");
    }
}
