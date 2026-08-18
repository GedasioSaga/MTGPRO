//! `GreedyBot`: busca rasa de um passo.
//!
//! **Por que a busca não roda no motor.** O jeito ideal seria clonar `Game`,
//! aplicar a ação e perguntar ao próprio motor como ficou o estado. Não dá:
//! `Game` carrega `Vec<Box<dyn Agent>>`, que não é `Clone` (nem poderia ser —
//! um agente pode ter estado próprio, canal de rede, RNG), e o motor não expõe
//! "aplique esta ação e devolva o estado": `turn::run_game` só sabe jogar a
//! partida inteira, do começo ao fim.
//!
//! **A escolha feita.** O passo é dado sobre uma cópia do `Snapshot`, que é
//! `Clone` e existe justamente para isso, com `sim::apply_action` — o mesmo
//! modelo aproximado que `HeuristicBot` usa para prever combate. A conta é a
//! mesma que um minimax de profundidade 1 faria, só que sobre o modelo em vez
//! de sobre o motor.
//!
//! **O que se perde, dito em voz alta.** Gatilhos não disparam na previsão,
//! efeitos de substituição não se aplicam, e efeito fora do vocabulário de
//! `sim::predict_effect` vira no-op. Por isso o `GreedyBot` é uma alternativa
//! de comparação, não o bot padrão do servidor.
use mtg_core::action::{Action, Request};
use mtg_core::engine::{Agent, Game};
use mtg_core::state::GameOutcome;

use crate::heuristic::{self, HeuristicBot};

pub struct GreedyBot {
    seed: u64,
    decisions: u64,
    /// Decisão que `sim::apply_action` não modela cai na heurística — prever
    /// no-op para tudo faria a busca escolher a primeira opção sempre.
    fallback: HeuristicBot,
}

impl GreedyBot {
    pub fn new(seed: u64) -> GreedyBot {
        GreedyBot {
            seed,
            decisions: 0,
            fallback: HeuristicBot::new(seed),
        }
    }
}

/// Requests em que dar um passo à frente muda de fato o snapshot.
fn is_modeled(request: &Request) -> bool {
    matches!(
        request,
        Request::Priority { .. } | Request::DeclareAttackers { .. } | Request::DeclareBlockers { .. }
    )
}

impl Agent for GreedyBot {
    fn name(&self) -> &str {
        "greedy"
    }

    fn decide(&mut self, game: &Game, request: &Request, legal: &[Action]) -> Action {
        let Some(first) = legal.first() else {
            return Action::PassPriority;
        };
        if legal.len() == 1 {
            return first.clone();
        }
        if !is_modeled(request) {
            return self.fallback.decide(game, request, legal);
        }
        self.decisions = self.decisions.wrapping_add(1);
        let Some(me) = request.player() else {
            return first.clone();
        };
        let s = heuristic::snapshot_for(game, me, request);
        let ctx = heuristic::make_ctx(game, &s, me, request);
        heuristic::pick_best(legal, self.seed, self.decisions, |action| {
            heuristic::lookahead_score(&ctx, action)
        })
    }

    fn on_game_end(&mut self, _game: &Game, _outcome: GameOutcome) {}
}
