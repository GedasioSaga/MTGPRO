//! Tipos de fio do servidor: WebSocket `/ws/match` e respostas REST.
//!
//! Os nomes de campo aqui são contrato com o frontend (ver
//! `docs/ENGINE_CONTRACT.md`, seção "Protocolo de rede") — mudar um nome
//! quebra o cliente sem aviso de compilador, então não renomeie por conta
//! própria.
use mtg_core::state::GameOutcome;
use mtg_core::view::{GameView, MatchEvent};
use serde::{Deserialize, Serialize};

/// Mensagem recebida do cliente em `/ws/match`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    Start {
        deck_a: String,
        deck_b: String,
        seed: u64,
        #[serde(default = "default_speed")]
        speed: f64,
    },
    Pause,
    Resume,
    Step,
}

fn default_speed() -> f64 {
    1.0
}

/// Frame enviado ao cliente em `/ws/match`.
///
/// `Init`, `Events` e `Done` são o contrato. `Error` é extensão nossa: sem
/// ela um pedido malformado deixaria o cliente esperando em silêncio.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerFrame {
    Init {
        view: GameView,
        players: [String; 2],
        seed: u64,
    },
    Events {
        events: Vec<MatchEvent>,
        view: GameView,
    },
    Done {
        outcome: GameOutcome,
        turns: u32,
        duration_ms: u64,
    },
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// REST
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: &'static str,
    pub cards: usize,
    pub decks: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub colors: Vec<String>,
    pub card_count: usize,
}
