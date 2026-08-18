//! Ponte entre o servidor e os agentes de `mtg-ai`.
//!
//! O servidor não decide nada: ele traduz o nome pedido no frame `start` do
//! WebSocket para um `Box<dyn Agent>` de `mtg_ai::bot_by_name`, e é o motor
//! que chama o agente. Nome desconhecido cai no padrão (`heuristic`) em vez de
//! derrubar a partida — o cliente é entrada hostil, não fonte de verdade.
//!
//! `Agent::decide` só recebe `&Game` (o motor nunca cede mutabilidade a um
//! agente), então nenhum bot usa `game.rng`: cada um carrega o próprio RNG
//! semeado a partir da semente da partida. A partida inteira continua
//! determinística porque a semente do bot é função pura da semente da partida.
use mtg_core::engine::{Agent, Game};
use mtg_core::state::GameOutcome;
use mtg_core::{Action, Request};

/// Bot da partida, resolvido por nome. Mantém `SeededBot::new(nome, semente)`
/// — a assinatura que `sim.rs` já chama — mas o que ele faz por dentro agora é
/// delegar para `mtg-ai`.
pub struct SeededBot {
    label: String,
    inner: Box<dyn Agent>,
}

impl SeededBot {
    /// `label` é o nome exibido do jogador (vem do deck escolhido). O tipo de
    /// bot é o padrão do servidor — use `with_kind` para escolher outro.
    pub fn new(label: impl Into<String>, seed: u64) -> Self {
        SeededBot::with_kind(label, mtg_ai::DEFAULT_BOT, seed)
    }

    /// `kind` é o nome do bot vindo do frame `start` (`random`, `heuristic`,
    /// `greedy`). Desconhecido ou vazio → padrão.
    pub fn with_kind(label: impl Into<String>, kind: &str, seed: u64) -> Self {
        let inner = mtg_ai::bot_by_name(kind, seed)
            .or_else(|| mtg_ai::bot_by_name(mtg_ai::DEFAULT_BOT, seed))
            .unwrap_or_else(|| Box::new(mtg_core::engine::FirstLegalAgent));
        SeededBot {
            label: label.into(),
            inner,
        }
    }

    /// Nome do tipo de bot em uso — útil para log e para a UI conferir o que
    /// de fato subiu quando o pedido veio com nome desconhecido.
    ///
    /// `allow(dead_code)`: hoje só os testes chamam. É a leitura que o
    /// `routes.rs` precisa quando o campo `bot` do frame `start` for fiado
    /// até aqui, e removê-la agora só faria alguém reescrevê-la depois.
    #[allow(dead_code)]
    pub fn kind(&self) -> &str {
        self.inner.name()
    }
}

impl Agent for SeededBot {
    fn name(&self) -> &str {
        &self.label
    }

    fn decide(&mut self, game: &Game, request: &Request, legal: &[Action]) -> Action {
        self.inner.decide(game, request, legal)
    }

    fn on_game_end(&mut self, game: &Game, outcome: GameOutcome) {
        self.inner.on_game_end(game, outcome);
    }
}

// ---------------------------------------------------------------------------
// Teste de força
// ---------------------------------------------------------------------------
//
// Este teste mora aqui, e não em `mtg-ai`, porque `mtg-ai` não depende de
// `mtg-cards` de propósito (a IA não sabe o que é um arquivo `.lua`). O
// servidor é o primeiro ponto do grafo que tem catálogo, decks e agentes ao
// mesmo tempo — é aqui que dá para provar que a heurística joga melhor que o
// acaso usando as cartas de verdade.

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::card::CardDatabase;
    use mtg_core::engine::{GameConfig, PlayerConfig};
    use mtg_core::ids::{CardDefId, PlayerId};
    use std::sync::Arc;

    fn database() -> Arc<CardDatabase> {
        match mtg_cards::build_database() {
            Ok(db) => Arc::new(db),
            Err(err) => panic!("catálogo não carregou: {err:?}"),
        }
    }

    fn deck(db: &CardDatabase, name: &str) -> Vec<CardDefId> {
        match mtg_cards::deck_by_name(db, name) {
            Some(cards) => cards,
            None => panic!("deck desconhecido no catálogo: {name}"),
        }
    }

    fn config() -> GameConfig {
        GameConfig {
            starting_life: 20,
            starting_hand_size: 7,
            allow_mulligan: true,
            max_turns: 60,
            max_decisions: 100_000,
            ..GameConfig::default()
        }
    }

    /// Roda uma partida entre dois tipos de bot e devolve o resultado.
    /// `first` joga como jogador 0 (que começa).
    fn duel(
        db: Arc<CardDatabase>,
        decks: (Vec<CardDefId>, Vec<CardDefId>),
        kinds: (&str, &str),
        seed: u64,
    ) -> GameOutcome {
        let players = vec![
            PlayerConfig { name: "A".to_string(), deck: decks.0, commander: None },
            PlayerConfig { name: "B".to_string(), deck: decks.1, commander: None },
        ];
        // Sementes derivadas distintas: dois bots com a mesma semente tomariam
        // exatamente as mesmas decisões "aleatórias" em posições simétricas.
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(SeededBot::with_kind("A", kinds.0, seed ^ 0x51_0A)),
            Box::new(SeededBot::with_kind("B", kinds.1, seed ^ 0x51_0B)),
        ];
        let mut game = match Game::new(db, players, agents, config(), seed) {
            Ok(g) => g,
            Err(err) => panic!("Game::new falhou com semente {seed}: {err:?}"),
        };
        game.run()
    }

    #[test]
    fn nome_desconhecido_cai_no_padrao() {
        assert_eq!(SeededBot::with_kind("x", "greedy", 1).kind(), "greedy");
        assert_eq!(SeededBot::with_kind("x", "random", 1).kind(), "random");
        assert_eq!(SeededBot::with_kind("x", "", 1).kind(), "heuristic");
        assert_eq!(SeededBot::with_kind("x", "trapaceiro", 1).kind(), "heuristic");
        assert_eq!(SeededBot::new("x", 1).kind(), "heuristic");
    }

    #[test]
    fn mesma_semente_mesma_partida() {
        let db = database();
        let a = deck(&db, "Goblin Onslaught");
        let b = deck(&db, "Azorius Control");
        for seed in [3u64, 11, 29] {
            let first = duel(Arc::clone(&db), (a.clone(), b.clone()), ("heuristic", "heuristic"), seed);
            let again = duel(Arc::clone(&db), (a.clone(), b.clone()), ("heuristic", "heuristic"), seed);
            assert_eq!(first, again, "semente {seed}: partida não foi determinística");
        }
    }

    /// A prova de que a IA decide alguma coisa: 50 partidas com sementes fixas
    /// contra o bot aleatório, trocando quem começa a cada semente para que a
    /// vantagem de iniciativa não seja o que está sendo medido.
    ///
    /// Marcado `#[ignore]` porque são 50 partidas completas com o catálogo em
    /// Lua carregado — roda com `cargo test -p mtg-server -- --ignored`.
    #[test]
    #[ignore = "50 partidas completas: lento demais para a suíte padrão"]
    fn heuristico_vence_o_acaso_com_folga() {
        let db = database();
        // Os quatro decks entram em rodízio: um só par de decks mediria a
        // heurística num arquétipo, e o `Azorius Control` no espelho quase
        // sempre empata por limite de turno — mediria a lista, não o bot.
        let lists: Vec<Vec<CardDefId>> = mtg_cards::decks()
            .iter()
            .map(|d| match d.expand(&db) {
                Some(cards) => cards,
                None => panic!("deck {} não expandiu", d.name),
            })
            .collect();

        let mut wins = 0usize;
        let mut losses = 0usize;
        let mut draws = 0usize;
        for seed in 0..50u64 {
            // Semente par: o heurístico começa. Ímpar: o aleatório começa.
            let heuristic_first = seed % 2 == 0;
            let kinds = if heuristic_first {
                ("heuristic", "random")
            } else {
                ("random", "heuristic")
            };
            // Os dois lados usam o mesmo deck na mesma partida: o que sobra de
            // diferença entre eles é decisão, não lista de cartas.
            let list = &lists[seed as usize % lists.len()];
            let outcome = duel(
                Arc::clone(&db),
                (list.clone(), list.clone()),
                kinds,
                seed,
            );
            let heuristic_player = if heuristic_first { PlayerId::P0 } else { PlayerId::P1 };
            match outcome {
                GameOutcome::Winner(p) if p == heuristic_player => wins += 1,
                GameOutcome::Winner(_) => losses += 1,
                _ => draws += 1,
            }
        }

        let rate = wins as f64 / 50.0;
        // Impresso sempre: a taxa medida é o resultado do teste, não só o
        // fato de ele passar. `cargo test -- --ignored --nocapture` mostra.
        println!(
            "heurístico x aleatório em 50 partidas: {wins} vitórias ({:.0}%),              {losses} derrotas, {draws} empates",
            rate * 100.0
        );
        assert!(
            rate >= 0.65,
            "heurístico venceu {wins}/50 ({:.0}%), perdeu {losses}, empatou {draws} — \
             abaixo do piso de 65%. Melhore o bot, não o teste.",
            rate * 100.0
        );
    }
}
