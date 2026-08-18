//! Rotas HTTP/WS, estado compartilhado e o laço de reprodução do WebSocket.
//!
//! Pausa/retomada/passo controlam só a *reprodução* — a simulação em si
//! (`sim::run_match_blocking`) roda inteira, sem parar, numa thread
//! bloqueante à parte; o que este módulo pausa é a entrega dos frames já
//! computados ao cliente. Ver `docs/ENGINE_CONTRACT.md`.
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderValue;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use mtg_core::card::{CardDatabase, CardDef};
use tokio::sync::{mpsc, Notify};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::{error, warn};

use crate::catalog::DeckInfo;
use crate::protocol::{ClientMessage, DeckSummary, HealthResponse, ServerFrame};
use crate::sim::{self, MatchRequest};

pub struct AppState {
    pub db: Arc<CardDatabase>,
    pub decks: Vec<DeckInfo>,
}

impl AppState {
    pub fn deck(&self, id: &str) -> Option<&DeckInfo> {
        self.decks.iter().find(|d| d.id == id)
    }
}

pub fn build_router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/cards", get(cards))
        .route("/api/decks", get(decks))
        .route("/ws/match", get(ws_match))
        .with_state(state);

    // Vite roda em porta separada durante o desenvolvimento — libera
    // qualquer porta de localhost/127.0.0.1 em vez de fixar uma.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            origin.as_bytes().starts_with(b"http://localhost:")
                || origin.as_bytes().starts_with(b"http://127.0.0.1:")
        }))
        .allow_methods(Any)
        .allow_headers(Any);

    let app = with_static_files(api);
    app.layer(cors).layer(TraceLayer::new_for_http())
}

/// Serve `./web/dist` quando o build de produção do frontend existe, com
/// fallback para `index.html` (roteamento client-side). Em desenvolvimento
/// esse diretório não existe — a UI roda via Vite em porta própria — e o
/// servidor Rust responde só as rotas de API/WS.
fn with_static_files(api: Router) -> Router {
    let dist = Path::new("web/dist");
    if !dist.is_dir() {
        return api;
    }
    let index = dist.join("index.html");
    let serve = ServeDir::new(dist).not_found_service(ServeFile::new(index));
    api.fallback_service(serve)
}

// ---------------------------------------------------------------------------
// REST
// ---------------------------------------------------------------------------

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok", cards: state.db.cards.len(), decks: state.decks.len() })
}

async fn cards(State(state): State<Arc<AppState>>) -> Json<Vec<CardDef>> {
    Json(state.db.cards.clone())
}

async fn decks(State(state): State<Arc<AppState>>) -> Json<Vec<DeckSummary>> {
    let list = state
        .decks
        .iter()
        .map(|d| DeckSummary {
            id: d.id.clone(),
            name: d.name.clone(),
            description: d.description.clone(),
            colors: d.colors.clone(),
            card_count: d.cards.len(),
        })
        .collect();
    Json(list)
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

async fn ws_match(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Controla a *reprodução* dos eventos já computados, nunca a simulação.
/// `notify_one` foi escolhido de propósito: guarda no máximo 1 permissão,
/// então "retomar" ou "passo" mandado antes de `gate()` bloquear não se
/// perde — é o padrão recomendado pela documentação do `tokio::sync::Notify`
/// para o caso de um único esperador.
struct PlaybackControl {
    paused: AtomicBool,
    step_credit: AtomicUsize,
    wake: Notify,
}

impl PlaybackControl {
    fn new() -> Self {
        PlaybackControl { paused: AtomicBool::new(false), step_credit: AtomicUsize::new(0), wake: Notify::new() }
    }
    fn reset(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.step_credit.store(0, Ordering::SeqCst);
        self.wake.notify_one();
    }
    fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }
    fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.wake.notify_one();
    }
    fn step(&self) {
        self.step_credit.fetch_add(1, Ordering::SeqCst);
        self.wake.notify_one();
    }
    /// Bloqueia enquanto pausado, exceto por créditos de passo acumulados.
    async fn gate(&self) {
        loop {
            if !self.paused.load(Ordering::SeqCst) {
                return;
            }
            let mut cur = self.step_credit.load(Ordering::SeqCst);
            while cur > 0 {
                match self.step_credit.compare_exchange(cur, cur - 1, Ordering::SeqCst, Ordering::SeqCst) {
                    Ok(_) => return,
                    Err(actual) => cur = actual,
                }
            }
            self.wake.notified().await;
        }
    }
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(64);

    // Única dona do lado de envio do socket — o laço de leitura abaixo e a
    // tarefa de reprodução só mandam `Message` por este canal, nunca tocam
    // `ws_tx` diretamente. Evita a necessidade de sincronizar dois
    // escritores concorrentes no mesmo `WebSocket`.
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let control = Arc::new(PlaybackControl::new());
    let mut playback: Option<tokio::task::JoinHandle<()>> = None;

    while let Some(Ok(msg)) = ws_rx.next().await {
        let Message::Text(text) = msg else { continue };
        match serde_json::from_str::<ClientMessage>(text.as_str()) {
            Ok(ClientMessage::Start { deck_a, deck_b, seed, speed, perspective }) => {
                if playback.as_ref().is_some_and(|h| !h.is_finished()) {
                    send_error(&out_tx, "partida já em andamento nesta conexão").await;
                    continue;
                }
                let Some(cfg_a) = state.deck(&deck_a) else {
                    send_error(&out_tx, &format!("deck desconhecido: {deck_a}")).await;
                    continue;
                };
                let Some(cfg_b) = state.deck(&deck_b) else {
                    send_error(&out_tx, &format!("deck desconhecido: {deck_b}")).await;
                    continue;
                };

                let req = MatchRequest {
                    name_a: cfg_a.name.clone(),
                    name_b: cfg_b.name.clone(),
                    observer: perspective.observer(),
                    deck_a: cfg_a.cards.clone(),
                    deck_b: cfg_b.cards.clone(),
                    seed,
                };
                let db = state.db.clone();
                let (sim_tx, sim_rx) = mpsc::channel::<ServerFrame>(64);
                // Thread bloqueante dedicada: o motor roda a partida inteira
                // numa chamada só e não pode compartilhar a runtime async.
                tokio::task::spawn_blocking(move || sim::run_match_blocking(db, req, sim_tx));

                control.reset();
                let control_for_playback = control.clone();
                let out_for_playback = out_tx.clone();
                let speed = if speed.is_finite() && speed > 0.0 { speed } else { 1.0 };
                playback = Some(tokio::spawn(playback_loop(sim_rx, out_for_playback, control_for_playback, speed)));
            }
            Ok(ClientMessage::Pause) => control.pause(),
            Ok(ClientMessage::Resume) => control.resume(),
            Ok(ClientMessage::Step) => control.step(),
            Err(err) => {
                send_error(&out_tx, &format!("mensagem inválida: {err}")).await;
            }
        }
    }

    if let Some(handle) = playback {
        handle.abort();
    }
    drop(out_tx);
    let _ = writer.await;
}

/// Repassa os frames da simulação para o cliente, espaçados no tempo por
/// `MatchEvent::suggested_duration_ms` dividido pela velocidade pedida.
/// `Init`/`Error` passam direto; só `Events` é pausável — pausar antes da
/// partida começar ou depois que ela termina não faz sentido.
async fn playback_loop(
    mut rx: mpsc::Receiver<ServerFrame>,
    out: mpsc::Sender<Message>,
    control: Arc<PlaybackControl>,
    speed: f64,
) {
    while let Some(frame) = rx.recv().await {
        let is_final = matches!(frame, ServerFrame::Done { .. } | ServerFrame::Error { .. });

        if let ServerFrame::Events { ref events, .. } = frame {
            control.gate().await;
            let total_ms: f64 = events.iter().map(|e| e.suggested_duration_ms() as f64).sum();
            let delay_ms = (total_ms / speed).round() as u64;
            if delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        if !send_frame(&out, &frame).await || is_final {
            return;
        }
    }
}

async fn send_error(out: &mpsc::Sender<Message>, message: &str) {
    warn!(message, "erro de protocolo em /ws/match");
    send_frame(out, &ServerFrame::Error { message: message.to_string() }).await;
}

/// Serializa e manda um frame; devolve `false` se o canal de saída já
/// fechou (cliente desconectado) para o chamador poder parar de trabalhar.
async fn send_frame(out: &mpsc::Sender<Message>, frame: &ServerFrame) -> bool {
    let text = match serde_json::to_string(frame) {
        Ok(t) => t,
        Err(err) => {
            error!(%err, "falha ao serializar ServerFrame");
            return true; // não é motivo para derrubar a conexão
        }
    };
    out.send(Message::Text(text)).await.is_ok()
}
