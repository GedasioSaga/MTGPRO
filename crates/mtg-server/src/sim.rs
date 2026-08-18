//! Executa uma partida inteira numa thread bloqueante e transmite o
//! progresso pelo canal para a tarefa do WebSocket.
//!
//! Ponto crítico de design (ver prompt da tarefa): o motor (`Game::run`) é
//! síncrono e roda a partida inteira numa chamada só. Para a UI animar aos
//! poucos sem travar o runtime async, a simulação roda em
//! `spawn_blocking` e transmite eventos por um `mpsc` assim que ocorrem; a
//! tarefa do WebSocket (em `routes.rs`) é quem espaça a entrega no tempo —
//! ver `MatchEvent::suggested_duration_ms`. Isso mantém a simulação rápida e
//! a reprodução no ritmo certo, e permite pausar/retomar a reprodução sem
//! jamais pausar o motor.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use mtg_core::card::CardDatabase;
use mtg_core::engine::{Agent, Game, GameConfig, PlayerConfig};
use mtg_core::ids::CardDefId;
use mtg_core::state::GameOutcome;
use mtg_core::view::Observer;
use mtg_core::{Action, Request};
use tokio::sync::mpsc::Sender;
use tracing::warn;

use crate::bot::SeededBot;
use crate::protocol::ServerFrame;

/// O que o cliente pediu em `{"type":"start", ...}`, já resolvido para
/// listas de carta concretas (a resolução de nome de deck → cartas mora em
/// `routes.rs`, que tem acesso ao catálogo).
pub struct MatchRequest {
    pub name_a: String,
    pub name_b: String,
    pub deck_a: Vec<CardDefId>,
    pub deck_b: Vec<CardDefId>,
    pub seed: u64,
    /// De quem e a visao transmitida. Ver `protocol::Perspective`.
    pub observer: Observer,
}

/// Decorador de `Agent`: nunca muta `Game` porque a assinatura de
/// `Agent::decide` só empresta `&Game` (CR de motor: decisão nunca muda
/// estado por si só). Por isso ele não pode chamar `Game::drain_events`
/// (exige `&mut`) — em vez disso lê o campo público `match_events` e usa um
/// cursor compartilhado entre os dois agentes da partida, para nunca
/// reenviar um evento que o outro agente já drenou.
struct StreamingAgent {
    inner: Box<dyn Agent>,
    tx: Sender<ServerFrame>,
    cursor: Arc<AtomicUsize>,
    observer: Observer,
}

impl StreamingAgent {
    fn flush(&self, game: &Game) {
        let already_sent = self.cursor.load(Ordering::Relaxed);
        let total = game.match_events.len();
        if total <= already_sent {
            return;
        }
        let batch = game.match_events[already_sent..total].to_vec();
        self.cursor.store(total, Ordering::Relaxed);
        let view = game.view(self.observer);
        // Melhor esforço: se o cliente WS já desconectou, o receptor foi
        // dropado e o envio falha — a simulação segue até o fim mesmo assim
        // (ela tem teto de decisões em `GameConfig::max_decisions`).
        if self.tx.blocking_send(ServerFrame::Events { events: batch, view }).is_err() {
            warn!("descartando eventos: cliente WS desconectado");
        }
    }
}

impl Agent for StreamingAgent {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn decide(&mut self, game: &Game, request: &Request, legal: &[Action]) -> Action {
        self.flush(game);
        self.inner.decide(game, request, legal)
    }

    fn on_game_end(&mut self, game: &Game, outcome: GameOutcome) {
        self.flush(game);
        self.inner.on_game_end(game, outcome);
    }
}

/// Roda a partida do início ao fim, mandando `Init`, `Events*` e `Done`
/// pelo canal. Chamar de dentro de `tokio::task::spawn_blocking` — a função
/// em si bloqueia a thread até a partida acabar.
pub fn run_match_blocking(db: Arc<CardDatabase>, req: MatchRequest, tx: Sender<ServerFrame>) {
    let players = vec![
        PlayerConfig { name: req.name_a.clone(), deck: req.deck_a, commander: None },
        PlayerConfig { name: req.name_b.clone(), deck: req.deck_b, commander: None },
    ];

    // Semente derivada, não a semente crua, para que os dois bots não
    // tomem exatamente as mesmas decisões "aleatórias" quando os estados
    // calham de ser simétricos.
    let cursor = Arc::new(AtomicUsize::new(0));
    let agent_a: Box<dyn Agent> = Box::new(StreamingAgent {
        observer: req.observer,
        inner: Box::new(SeededBot::new(req.name_a.clone(), req.seed ^ 0x51_0A)),
        tx: tx.clone(),
        cursor: cursor.clone(),
    });
    let agent_b: Box<dyn Agent> = Box::new(StreamingAgent {
        observer: req.observer,
        inner: Box::new(SeededBot::new(req.name_b.clone(), req.seed ^ 0x51_0B)),
        tx: tx.clone(),
        cursor: cursor.clone(),
    });

    let mut game = match Game::new(db, players, vec![agent_a, agent_b], GameConfig::default(), req.seed) {
        Ok(g) => g,
        Err(err) => {
            let _ = tx.blocking_send(ServerFrame::Error {
                message: format!("falha ao montar a partida: {err}"),
            });
            return;
        }
    };

    let init_view = game.view(req.observer);
    if tx
        .blocking_send(ServerFrame::Init { view: init_view, players: [req.name_a, req.name_b], seed: req.seed })
        .is_err()
    {
        return; // cliente já foi embora antes da partida começar
    }

    let start = Instant::now();
    let outcome = game.run();

    // Eventos emitidos depois da última decisão (ex.: a ação baseada em
    // estado que fecha o jogo) nunca passam por `StreamingAgent::decide` —
    // drena o que sobrou direto do estado, que agora está livre (`run`
    // devolveu, então não há mais empréstimo ativo do motor).
    let already_sent = cursor.load(Ordering::Relaxed);
    if game.match_events.len() > already_sent {
        let batch = game.match_events[already_sent..].to_vec();
        let view = game.view(req.observer);
        let _ = tx.blocking_send(ServerFrame::Events { events: batch, view });
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let _ = tx.blocking_send(ServerFrame::Done { outcome, turns: game.state.turn, duration_ms });
}
