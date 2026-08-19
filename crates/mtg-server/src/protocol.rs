//! Tipos de fio do servidor: WebSocket `/ws/match` e respostas REST.
//!
//! Os nomes de campo aqui são contrato com o frontend (ver
//! `docs/ENGINE_CONTRACT.md`, seção "Protocolo de rede") — mudar um nome
//! quebra o cliente sem aviso de compilador, então não renomeie por conta
//! própria.
use mtg_core::state::GameOutcome;
use mtg_core::view::{GameView, MatchEvent};
use serde::{Deserialize, Serialize};

/// Um assento pedido pelo cliente no frame `start`.
///
/// `deck` é o id de `GET /api/decks`. `bot` e `commander` são opcionais: bot
/// ausente cai no padrão do servidor, comandante ausente usa o que a própria
/// lista declara (CR 903.3).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatRequest {
    pub deck: String,
    #[serde(default)]
    pub bot: Option<String>,
    #[serde(default)]
    pub commander: Option<String>,
}

/// Mensagem recebida do cliente em `/ws/match`.
///
/// # Duas formas de `start`
///
/// A forma nova traz `seats[]` (2 a 4) e `format`. A antiga traz `deckA`/
/// `deckB` e nada mais — continua aceita porque cliente já publicado não pode
/// quebrar por causa de campo novo. Por isso os quatro campos são opcionais no
/// fio: quem decide qual forma chegou é `routes.rs`, e pedido que não é nem uma
/// nem outra vira `ServerFrame::Error`, não pânico de desserialização.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ClientMessage {
    Start {
        #[serde(default)]
        deck_a: Option<String>,
        #[serde(default)]
        deck_b: Option<String>,
        /// Slug do formato (`commander`, `standard`, `modern`, `pauper`,
        /// `casual`). Ausente na forma antiga, onde o duelo é sempre casual.
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        seats: Vec<SeatRequest>,
        seed: u64,
        #[serde(default = "default_speed")]
        speed: f64,
        /// De quem e a visao transmitida. Omitido = "player0", que e o que o
        /// espectador espera: mao de baixo aberta, mao de cima de costas.
        /// "omniscient" existe para analise, mas precisa ser pedido.
        #[serde(default)]
        perspective: Perspective,
    },
    Pause,
    Resume,
    Step,
}

fn default_speed() -> f64 {
    1.0
}

/// Quem esta assistindo. O padrao NAO e onisciente de proposito: default que
/// revela informacao oculta transforma bug de esquecimento em vazamento.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Perspective {
    #[default]
    Player0,
    Player1,
    Spectator,
    Omniscient,
}

impl Perspective {
    pub fn observer(self) -> mtg_core::view::Observer {
        use mtg_core::view::Observer;
        match self {
            Perspective::Player0 => Observer::Player(mtg_core::ids::PlayerId(0)),
            Perspective::Player1 => Observer::Player(mtg_core::ids::PlayerId(1)),
            Perspective::Spectator => Observer::Spectator,
            Perspective::Omniscient => Observer::Omniscient,
        }
    }
}

/// Frame enviado ao cliente em `/ws/match`.
///
/// `Init`, `Events` e `Done` são o contrato. `Error` é extensão nossa: sem
/// ela um pedido malformado deixaria o cliente esperando em silêncio.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ServerFrame {
    Init {
        view: GameView,
        /// Nomes dos jogadores, na ordem dos assentos. Era `[String; 2]`
        /// enquanto só havia duelo; virou `Vec` para a mesa de três e quatro.
        /// O JSON é o mesmo (lista de strings), então o cliente não muda.
        players: Vec<String>,
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
    /// Slug do formato para o qual a lista foi montada.
    pub format: String,
    pub commander: Option<String>,
    /// Veredito por formato, para a tela de abertura poder mostrar o que está
    /// errado ANTES de pedir a partida. A checagem é a mesma que o `start`
    /// refaz — aqui ela só chega cedo.
    pub legality: Vec<DeckLegality>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckLegality {
    pub format: String,
    pub legal: bool,
    /// Mensagens já em português, prontas para a tela. Vazio quando `legal`.
    pub violations: Vec<String>,
}

/// Um formato jogável, com as regras de construção que a UI precisa exibir.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatSummary {
    pub slug: String,
    pub name: String,
    /// Mínimo de cartas. CR 100.2a nos construídos, CR 903.5a em Commander.
    pub min_deck_size: u32,
    /// `Some` só onde o tamanho é fechado (Commander, 100).
    pub exact_deck_size: Option<u32>,
    pub max_copies: Option<u8>,
    pub requires_commander: bool,
    pub min_players: usize,
    pub max_players: usize,
    /// Quantos decks do servidor passam na validação deste formato.
    pub deck_count: usize,
}
