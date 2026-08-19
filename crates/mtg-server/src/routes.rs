//! Rotas HTTP/WS, estado compartilhado e o laço de reprodução do WebSocket.
//!
//! Pausa/retomada/passo controlam só a *reprodução* — a simulação em si
//! (`sim::run_match_blocking`) roda inteira, sem parar, numa thread
//! bloqueante à parte; o que este módulo pausa é a entrega dos frames já
//! computados ao cliente. Ver `docs/ENGINE_CONTRACT.md`.
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use mtg_core::card::{CardDatabase, CardDef};
use mtg_core::mana::Color;
use mtg_core::types::CardType;
use mtg_db::{CardPage, CardQuery, CardStore, CatalogStats};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Notify};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::{error, warn};

use mtg_cards::Format;
use mtg_format::{validate_with, CatalogLegality, LegalitySource, ScryfallLegality, Violation};

use crate::catalog::DeckInfo;
use crate::protocol::{
    ClientMessage, DeckLegality, DeckSummary, FormatSummary, HealthResponse, SeatRequest,
    ServerFrame,
};
use crate::sim::{self, MatchRequest, Seat, TableRequest, MAX_SEATS, MIN_SEATS};

/// Teto de itens por página. Existe para que um `limit=999999` de cliente
/// distraído não vire uma resposta de dezenas de MB — que é exatamente o que
/// o antigo `/api/cards` fazia com o catálogo inteiro.
const MAX_PAGE_LIMIT: usize = 200;
const DEFAULT_PAGE_LIMIT: usize = 60;

pub struct AppState {
    pub db: Arc<CardDatabase>,
    pub decks: Vec<DeckInfo>,
    /// Banimento e rotação de verdade, vindos do Scryfall. `None` quando o
    /// banco não tem esse dado — ver `AppState::violations`.
    pub legality: Option<ScryfallLegality>,
}

impl AppState {
    pub fn new(db: Arc<CardDatabase>, decks: Vec<DeckInfo>) -> AppState {
        AppState { legality: crate::catalog::load_legality(&db), db, decks }
    }

    pub fn deck(&self, id: &str) -> Option<&DeckInfo> {
        self.decks.iter().find(|d| d.id == id)
    }

    /// Tudo que impede esta lista de ser jogada neste formato, de uma vez só —
    /// `mtg-format` devolve o conjunto inteiro, e quem monta deck quer a lista
    /// inteira, não um problema por rodada.
    ///
    /// Sem índice do Scryfall a checagem cai em `CatalogLegality`, que valida
    /// tamanho, cópias, singleton, identidade de cor e raridade mas **não**
    /// banimento. Degradar assim é melhor que o alternativo: o índice vazio
    /// responde "ilegal" para tudo, e aí nenhuma partida começaria.
    pub fn violations(&self, deck: &DeckInfo, format: Format) -> Vec<Violation> {
        let result = match &self.legality {
            Some(index) => self.validate(deck, format, index),
            None => self.validate(deck, format, &CatalogLegality::new(&self.db)),
        };
        result.err().unwrap_or_default()
    }

    fn validate<L: LegalitySource + ?Sized>(
        &self,
        deck: &DeckInfo,
        format: Format,
        legality: &L,
    ) -> Result<(), Vec<Violation>> {
        validate_with(&deck.list, &self.db, format, legality)
    }

    /// Quantos jogadores este formato aceita na mesa.
    ///
    /// Só Commander é multiplayer aqui: CR 903 é a variante desenhada para
    /// free-for-all, e Standard/Modern/Pauper são formatos de duelo. O motor
    /// roda quatro assentos em qualquer formato — o que segura em dois é esta
    /// regra, não uma limitação técnica.
    fn seat_range(format: Format) -> (usize, usize) {
        if format.requires_commander() {
            (MIN_SEATS, MAX_SEATS)
        } else {
            (MIN_SEATS, MIN_SEATS)
        }
    }
}

/// Estado das rotas de catálogo. `CardStore` guarda uma `rusqlite::Connection`,
/// que é `Send` mas não `Sync` — o `Mutex` é o que permite compartilhá-la
/// entre requisições. Toda consulta roda em `spawn_blocking`, então o lock
/// nunca é segurado através de um `await`.
struct CatalogState {
    store: Option<Mutex<CardStore>>,
    /// Catálogo curado em Lua, servido inteiro pela rota legada.
    curated: Arc<CardDatabase>,
}

pub fn build_router(state: Arc<AppState>) -> Router {
    let catalog = Arc::new(CatalogState {
        store: crate::catalog::open_store(&state.db).map(Mutex::new),
        curated: state.db.clone(),
    });

    let catalog_api = Router::new()
        .route("/api/cards", get(cards))
        .route("/api/cards/:oracle_id", get(card_by_oracle_id))
        .route("/api/catalog", get(curated_catalog))
        .route("/api/stats", get(stats))
        .with_state(catalog);

    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/decks", get(decks))
        .route("/api/formats", get(formats))
        .route("/ws/match", get(ws_match))
        .with_state(state)
        .merge(catalog_api);

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

// ---------------------------------------------------------------------------
// Catálogo
// ---------------------------------------------------------------------------

/// Parâmetros de `GET /api/cards`. Tudo chega como texto e é validado aqui —
/// query string é entrada hostil, então valor malformado vira 400 com
/// mensagem, nunca `unwrap` nem filtro silenciosamente ignorado.
#[derive(Debug, Deserialize)]
struct CardsParams {
    q: Option<String>,
    colors: Option<String>,
    #[serde(rename = "type")]
    card_type: Option<String>,
    mv: Option<String>,
    set: Option<String>,
    playable: Option<String>,
    limit: Option<String>,
    offset: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

#[derive(Debug)]
enum ApiFailure {
    BadRequest(String),
    NotFound(String),
    Unavailable(String),
    Internal(String),
}

impl IntoResponse for ApiFailure {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiFailure::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiFailure::NotFound(m) => (StatusCode::NOT_FOUND, m),
            ApiFailure::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
            ApiFailure::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, Json(ApiError { error: message })).into_response()
    }
}

/// `GET /api/cards` — busca paginada. Substituiu a resposta que despejava o
/// catálogo inteiro num array; com 35 mil cartas aquilo eram dezenas de MB
/// por requisição. Quem precisa do array antigo usa `/api/catalog`.
async fn cards(
    State(state): State<Arc<CatalogState>>,
    Query(params): Query<CardsParams>,
) -> Result<Json<CardPage>, ApiFailure> {
    let query = build_query(&params)?;
    let page = blocking_query(state, move |store| store.search_page(&query)).await?;
    Ok(Json(page))
}

/// `GET /api/cards/:oracle_id` — uma carta, com o `CardDef` completo.
async fn card_by_oracle_id(
    State(state): State<Arc<CatalogState>>,
    UrlPath(oracle_id): UrlPath<String>,
) -> Result<impl IntoResponse, ApiFailure> {
    let wanted = oracle_id.clone();
    let found = blocking_query(state, move |store| store.find_by_oracle_id(&wanted)).await?;
    match found {
        Some(detail) => Ok(Json(detail)),
        None => Err(ApiFailure::NotFound(format!("carta não encontrada: {oracle_id}"))),
    }
}

/// `GET /api/stats` — total, jogáveis, não jogáveis e o recorte por coleção.
async fn stats(State(state): State<Arc<CatalogState>>) -> Result<Json<CatalogStats>, ApiFailure> {
    let stats = blocking_query(state, |store| store.stats()).await?;
    Ok(Json(stats))
}

/// Rota legada, explícita: o catálogo curado em Lua inteiro, como array.
/// É o formato que `GET /api/cards` tinha antes da paginação, e é seguro
/// porque essas cartas são poucas (centenas) e todas jogáveis — o catálogo do
/// Scryfall nunca sai por aqui.
async fn curated_catalog(State(state): State<Arc<CatalogState>>) -> Json<Vec<CardDef>> {
    Json(state.curated.cards.clone())
}

/// Roda uma consulta do SQLite fora da runtime async. O `Mutex` é travado
/// dentro do `spawn_blocking` e solto antes do retorno, então nenhum guard
/// atravessa `await`.
async fn blocking_query<T, F>(state: Arc<CatalogState>, work: F) -> Result<T, ApiFailure>
where
    T: Send + 'static,
    F: FnOnce(&CardStore) -> Result<T, mtg_db::DbError> + Send + 'static,
{
    let result = tokio::task::spawn_blocking(move || {
        let Some(store) = state.store.as_ref() else {
            return Err(ApiFailure::Unavailable("catálogo indisponível".to_string()));
        };
        // `PoisonError` só acontece se outra requisição entrou em pânico
        // segurando o lock; responder 503 é melhor que propagar o pânico.
        let guard = store
            .lock()
            .map_err(|_| ApiFailure::Unavailable("catálogo em estado inconsistente".to_string()))?;
        work(&guard).map_err(|err| {
            error!(%err, "consulta ao catálogo falhou");
            ApiFailure::Internal("falha ao consultar o catálogo".to_string())
        })
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(err) => {
            error!(%err, "tarefa de catálogo não completou");
            Err(ApiFailure::Internal("falha ao consultar o catálogo".to_string()))
        }
    }
}

fn build_query(params: &CardsParams) -> Result<CardQuery, ApiFailure> {
    let limit = parse_usize(params.limit.as_deref(), "limit")?
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);
    let offset = parse_usize(params.offset.as_deref(), "offset")?.unwrap_or(0);

    Ok(CardQuery {
        text: params.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        colors: parse_colors(params.colors.as_deref())?,
        types: parse_types(params.card_type.as_deref())?,
        mana_value_max: None,
        mana_value: parse_u32(params.mv.as_deref(), "mv")?,
        set_code: params.set.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        playable: parse_bool(params.playable.as_deref())?,
        limit,
        offset,
    })
}

/// Aceita `WU`, `w,u` ou `white,blue` — o cliente não precisa saber qual.
fn parse_colors(raw: Option<&str>) -> Result<Option<Vec<Color>>, ApiFailure> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else { return Ok(None) };
    let mut out: Vec<Color> = Vec::new();
    let push = |c: Color, out: &mut Vec<Color>| {
        if !out.contains(&c) {
            out.push(c);
        }
    };
    for part in raw.split([',', ' ']).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "white" => push(Color::White, &mut out),
            "blue" => push(Color::Blue, &mut out),
            "black" => push(Color::Black, &mut out),
            "red" => push(Color::Red, &mut out),
            "green" => push(Color::Green, &mut out),
            // Forma compacta: `WU`, `bg`, `wubrg`. Cada letra é uma cor.
            letters => {
                for ch in letters.chars() {
                    match Color::from_letter(ch.to_ascii_uppercase()) {
                        Some(c) => push(c, &mut out),
                        None => {
                            return Err(ApiFailure::BadRequest(format!("cor desconhecida: {part}")))
                        }
                    }
                }
            }
        }
    }
    Ok(if out.is_empty() { None } else { Some(out) })
}

fn parse_types(raw: Option<&str>) -> Result<Option<Vec<CardType>>, ApiFailure> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else { return Ok(None) };
    let mut out = Vec::new();
    for part in raw.split([',', ' ']).filter(|p| !p.is_empty()) {
        let parsed = match part.to_ascii_lowercase().as_str() {
            "artifact" => Some(CardType::Artifact),
            "battle" => Some(CardType::Battle),
            "creature" => Some(CardType::Creature),
            "enchantment" => Some(CardType::Enchantment),
            "instant" => Some(CardType::Instant),
            "land" => Some(CardType::Land),
            "planeswalker" => Some(CardType::Planeswalker),
            "sorcery" => Some(CardType::Sorcery),
            "kindred" | "tribal" => Some(CardType::Kindred),
            _ => None,
        };
        match parsed {
            Some(t) if !out.contains(&t) => out.push(t),
            Some(_) => {}
            None => return Err(ApiFailure::BadRequest(format!("tipo desconhecido: {part}"))),
        }
    }
    Ok(if out.is_empty() { None } else { Some(out) })
}

fn parse_bool(raw: Option<&str>) -> Result<Option<bool>, ApiFailure> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else { return Ok(None) };
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Ok(Some(true)),
        "0" | "false" | "no" => Ok(Some(false)),
        other => Err(ApiFailure::BadRequest(format!("playable inválido: {other}"))),
    }
}

fn parse_usize(raw: Option<&str>, field: &str) -> Result<Option<usize>, ApiFailure> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else { return Ok(None) };
    raw.parse::<usize>()
        .map(Some)
        .map_err(|_| ApiFailure::BadRequest(format!("{field} inválido: {raw}")))
}

fn parse_u32(raw: Option<&str>, field: &str) -> Result<Option<u32>, ApiFailure> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else { return Ok(None) };
    raw.parse::<u32>()
        .map(Some)
        .map_err(|_| ApiFailure::BadRequest(format!("{field} inválido: {raw}")))
}

/// Quantas violações de um mesmo deck vão para a tela. Uma lista de 60 cartas
/// fora do formato produz uma violação por carta, e despejar as 60 num painel
/// não informa mais que as primeiras.
const MAX_VIOLATIONS_SHOWN: usize = 8;

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
            format: d.format.slug().to_string(),
            commander: d.commander.clone(),
            legality: Format::ALL
                .into_iter()
                .map(|format| {
                    let found = state.violations(d, format);
                    DeckLegality {
                        format: format.slug().to_string(),
                        legal: found.is_empty(),
                        violations: summarize(&found),
                    }
                })
                .collect(),
        })
        .collect();
    Json(list)
}

/// Violações em texto, truncadas com um resumo do que sobrou — nunca cortadas
/// em silêncio, porque "3 problemas" e "31 problemas" pedem decisões
/// diferentes de quem está montando a mesa.
fn summarize(found: &[Violation]) -> Vec<String> {
    let mut out: Vec<String> =
        found.iter().take(MAX_VIOLATIONS_SHOWN).map(|v| v.to_string()).collect();
    if found.len() > MAX_VIOLATIONS_SHOWN {
        out.push(format!("… e mais {} problema(s)", found.len() - MAX_VIOLATIONS_SHOWN));
    }
    out
}

/// `GET /api/formats` — o que a tela de abertura precisa para montar a mesa:
/// os formatos, o tamanho de deck de cada um e quantos decks servem a cada um.
async fn formats(State(state): State<Arc<AppState>>) -> Json<Vec<FormatSummary>> {
    let list = Format::ALL
        .into_iter()
        .map(|format| {
            let (min_players, max_players) = AppState::seat_range(format);
            let deck_count =
                state.decks.iter().filter(|d| state.violations(d, format).is_empty()).count();
            FormatSummary {
                slug: format.slug().to_string(),
                name: format.to_string(),
                min_deck_size: format.min_deck_size(),
                exact_deck_size: format.exact_deck_size(),
                max_copies: format.max_copies(),
                requires_commander: format.requires_commander(),
                min_players,
                max_players,
                deck_count,
            }
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
            Ok(ClientMessage::Start { deck_a, deck_b, format, seats, seed, speed, perspective }) => {
                if playback.as_ref().is_some_and(|h| !h.is_finished()) {
                    send_error(&out_tx, "partida já em andamento nesta conexão").await;
                    continue;
                }
                let observer = perspective.observer();
                let db = state.db.clone();
                let (sim_tx, sim_rx) = mpsc::channel::<ServerFrame>(64);

                // Thread bloqueante dedicada nos dois caminhos: o motor roda a
                // partida inteira numa chamada só e não pode compartilhar a
                // runtime async.
                if seats.is_empty() {
                    // Forma antiga do frame `start`: duelo por `deckA`/`deckB`.
                    let (Some(deck_a), Some(deck_b)) = (deck_a, deck_b) else {
                        send_error(&out_tx, "start precisa de seats[] ou de deckA e deckB").await;
                        continue;
                    };
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
                        observer,
                        deck_a: cfg_a.cards.clone(),
                        deck_b: cfg_b.cards.clone(),
                        seed,
                    };
                    tokio::task::spawn_blocking(move || sim::run_match_blocking(db, req, sim_tx));
                } else {
                    let req = match build_table(&state, &seats, format.as_deref(), observer, seed) {
                        Ok(req) => req,
                        Err(message) => {
                            send_error(&out_tx, &message).await;
                            continue;
                        }
                    };
                    tokio::task::spawn_blocking(move || sim::run_table_blocking(db, req, sim_tx));
                }

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

/// Teto de linhas na mensagem de erro de legalidade. Quatro assentos com deck
/// errado geram dezenas de violações; a caixa da tela precisa continuar legível.
const MAX_PROBLEMS_REPORTED: usize = 12;

/// Monta a mesa pedida, ou devolve, numa mensagem só, TUDO que impede montá-la.
///
/// Nada aqui pode entrar em pânico: `seats` vem do cliente, que é entrada
/// hostil. Formato desconhecido, deck inexistente, contagem de assentos fora da
/// faixa e deck ilegal viram texto para o usuário ler.
fn build_table(
    state: &AppState,
    seats: &[SeatRequest],
    format: Option<&str>,
    observer: mtg_core::view::Observer,
    seed: u64,
) -> Result<TableRequest, String> {
    let slug = format.unwrap_or_else(|| Format::Casual.slug());
    let Some(format) = Format::from_slug(slug) else {
        return Err(format!("formato desconhecido: {slug}"));
    };

    let (min, max) = AppState::seat_range(format);
    let found = seats.len();
    if !(min..=max).contains(&found) {
        return Err(if min == max {
            format!("{format} é formato de duelo: pede {min} jogadores, e vieram {found}")
        } else {
            format!("{format} aceita de {min} a {max} jogadores, e vieram {found}")
        });
    }

    let mut problems: Vec<String> = Vec::new();
    let mut built: Vec<Seat> = Vec::new();
    let mut taken: Vec<String> = Vec::new();

    for (index, request) in seats.iter().enumerate() {
        let position = index + 1;
        let Some(deck) = state.deck(&request.deck) else {
            problems.push(format!("Assento {position}: deck desconhecido: {}", request.deck));
            continue;
        };
        for violation in state.violations(deck, format) {
            problems.push(format!("Assento {position} ({}): {violation}", deck.name));
        }

        // Comandante pedido pelo cliente ganha do declarado pela lista — é o
        // que permite trocar o comandante sem editar `decks.rs`.
        let commander = match request.commander.as_deref() {
            Some(name) => match state.db.id_by_name(name) {
                Some(id) => Some(id),
                None => {
                    problems.push(format!("Assento {position}: comandante desconhecido: {name}"));
                    None
                }
            },
            None => deck.commander_id,
        };

        let bot = request.bot.clone().unwrap_or_else(|| mtg_ai::DEFAULT_BOT.to_string());
        let mut seat =
            Seat::new(unique_name(&deck.name, &mut taken), deck.cards.clone()).with_bot(bot);
        if let Some(id) = commander {
            seat = seat.with_commander(id);
        }
        built.push(seat);
    }

    if !problems.is_empty() {
        let extra = problems.len().saturating_sub(MAX_PROBLEMS_REPORTED);
        problems.truncate(MAX_PROBLEMS_REPORTED);
        if extra > 0 {
            problems.push(format!("\u{2026} e mais {extra} problema(s)"));
        }
        return Err(problems.join("\n"));
    }

    let table = TableRequest { seats: built, format, seed, observer };
    // Cinto e suspensório: `run_table_blocking` revalida, mas responder aqui
    // dá a mensagem antes de gastar uma thread bloqueante.
    table.validate().map_err(|err| err.to_string())?;
    Ok(table)
}

/// Nome de assento único. Dois assentos com o mesmo deck teriam o mesmo nome, e
/// aí o placar da mesa mostra a mesma etiqueta duas vezes sem dizer quem é quem.
fn unique_name(base: &str, taken: &mut Vec<String>) -> String {
    let mut name = base.to_string();
    let mut n = 2usize;
    while taken.iter().any(|t| t == &name) {
        name = format!("{base} #{n}");
        n += 1;
    }
    taken.push(name.clone());
    name
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

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::view::Observer;

    fn params() -> CardsParams {
        CardsParams {
            q: None,
            colors: None,
            card_type: None,
            mv: None,
            set: None,
            playable: None,
            limit: None,
            offset: None,
        }
    }

    #[test]
    fn limit_is_capped_and_defaulted() {
        let q = build_query(&params()).expect("query padrão");
        assert_eq!(q.limit, DEFAULT_PAGE_LIMIT);
        assert_eq!(q.offset, 0);

        let mut p = params();
        p.limit = Some("999999".into());
        assert_eq!(build_query(&p).expect("limit alto").limit, MAX_PAGE_LIMIT);

        // limit=0 devolveria página vazia para sempre — sobe para 1.
        p.limit = Some("0".into());
        assert_eq!(build_query(&p).expect("limit zero").limit, 1);
    }

    #[test]
    fn malformed_numbers_are_rejected_not_ignored() {
        let mut p = params();
        p.limit = Some("abc".into());
        assert!(matches!(build_query(&p), Err(ApiFailure::BadRequest(_))));

        let mut p = params();
        p.mv = Some("-1".into());
        assert!(matches!(build_query(&p), Err(ApiFailure::BadRequest(_))));

        let mut p = params();
        p.offset = Some("1e9".into());
        assert!(matches!(build_query(&p), Err(ApiFailure::BadRequest(_))));
    }

    #[test]
    fn colors_accept_letters_commas_and_names() {
        assert_eq!(parse_colors(Some("WU")).expect("letras"), Some(vec![Color::White, Color::Blue]));
        assert_eq!(parse_colors(Some("w,u")).expect("vírgula"), Some(vec![Color::White, Color::Blue]));
        assert_eq!(
            parse_colors(Some("white blue")).expect("nomes"),
            Some(vec![Color::White, Color::Blue])
        );
        // Repetição não duplica o filtro.
        assert_eq!(parse_colors(Some("GG")).expect("repetido"), Some(vec![Color::Green]));
        assert_eq!(parse_colors(Some("  ")).expect("vazio"), None);
        assert!(parse_colors(Some("roxo")).is_err());
    }

    #[test]
    fn types_and_playable_parse_or_fail_loudly() {
        assert_eq!(
            parse_types(Some("creature,land")).expect("tipos"),
            Some(vec![CardType::Creature, CardType::Land])
        );
        assert!(parse_types(Some("planeswalkerr")).is_err());

        assert_eq!(parse_bool(Some("true")).expect("bool"), Some(true));
        assert_eq!(parse_bool(Some("0")).expect("bool"), Some(false));
        assert_eq!(parse_bool(None).expect("bool"), None);
        assert!(parse_bool(Some("talvez")).is_err());
    }

    #[test]
    fn blank_text_filter_is_dropped() {
        let mut p = params();
        p.q = Some("   ".into());
        assert!(build_query(&p).expect("query").text.is_none());

        p.q = Some("  bolt  ".into());
        assert_eq!(build_query(&p).expect("query").text.as_deref(), Some("bolt"));
    }

    // -----------------------------------------------------------------
    // Frame `start`: mesa de dois a quatro
    // -----------------------------------------------------------------

    /// Estado com os decks reais do catálogo. Sem banco em disco no diretório
    /// do crate, `legality` fica `None` e a validação cai na estrutural — que é
    /// o que estes testes exercitam (tamanho, singleton, comandante).
    fn table_state() -> Arc<AppState> {
        let (db, decks) = match crate::catalog::load() {
            Ok(v) => v,
            Err(err) => panic!("catálogo não carregou: {err}"),
        };
        assert!(decks.len() >= 6, "catálogo com {} decks: pouco para a mesa", decks.len());
        Arc::new(AppState::new(Arc::new(db), decks))
    }

    fn seat(deck: &str) -> SeatRequest {
        SeatRequest { deck: deck.to_string(), bot: None, commander: None }
    }

    fn commander_table(state: &AppState, count: usize) -> Result<TableRequest, String> {
        let seats: Vec<SeatRequest> = (0..count).map(|_| seat("conclave-of-emmara")).collect();
        build_table(state, &seats, Some("commander"), Observer::Spectator, 7)
    }

    /// Cliente antigo manda `deckA`/`deckB` e mais nada. Se isto quebrar, a
    /// versão publicada da UI para de iniciar partida sem aviso de compilador.
    #[test]
    fn start_antigo_continua_desserializando() {
        let raw = r#"{"type":"start","deckA":"goblin-onslaught","deckB":"azorius-control","seed":9}"#;
        let msg = match serde_json::from_str::<ClientMessage>(raw) {
            Ok(m) => m,
            Err(err) => panic!("frame antigo deixou de desserializar: {err}"),
        };
        let ClientMessage::Start { deck_a, deck_b, format, seats, seed, speed, .. } = msg else {
            panic!("frame antigo virou outra variante");
        };
        assert_eq!(deck_a.as_deref(), Some("goblin-onslaught"));
        assert_eq!(deck_b.as_deref(), Some("azorius-control"));
        assert_eq!(seed, 9);
        assert_eq!(speed, 1.0, "speed omitido tem de cair no padrão");
        assert!(format.is_none());
        assert!(seats.is_empty(), "frame antigo não declara assento");
    }

    #[test]
    fn start_novo_le_assentos_formato_e_comandante() {
        let raw = r#"{"type":"start","format":"commander","seed":3,"speed":2.0,
            "seats":[{"deck":"a","bot":"greedy","commander":"Emmara, Soul of the Accord"},
                     {"deck":"b"}]}"#;
        let msg = match serde_json::from_str::<ClientMessage>(raw) {
            Ok(m) => m,
            Err(err) => panic!("frame novo não desserializa: {err}"),
        };
        let ClientMessage::Start { format, seats, deck_a, speed, .. } = msg else {
            panic!("frame novo virou outra variante");
        };
        assert_eq!(format.as_deref(), Some("commander"));
        assert_eq!(speed, 2.0);
        assert!(deck_a.is_none());
        assert_eq!(seats.len(), 2);
        assert_eq!(seats[0].bot.as_deref(), Some("greedy"));
        assert_eq!(seats[0].commander.as_deref(), Some("Emmara, Soul of the Accord"));
        // Assento sem bot nem comandante é legal: os dois têm padrão.
        assert!(seats[1].bot.is_none() && seats[1].commander.is_none());
    }

    #[test]
    fn commander_monta_de_dois_a_quatro_assentos() {
        let state = table_state();
        for count in MIN_SEATS..=MAX_SEATS {
            let table = match commander_table(&state, count) {
                Ok(t) => t,
                Err(err) => panic!("mesa de {count} devia montar: {err}"),
            };
            assert_eq!(table.seats.len(), count);
            assert_eq!(table.format, Format::Commander);
            // CR 903.6 — o comandante não está na biblioteca.
            for s in &table.seats {
                assert_eq!(s.deck.len(), 99, "biblioteca de {} não tem 99", s.name);
                assert!(s.commander.is_some(), "{} sem comandante", s.name);
            }
            // Mesmo deck em todos os assentos não pode virar nomes iguais.
            let mut names = table.player_names();
            names.sort();
            let total = names.len();
            names.dedup();
            assert_eq!(names.len(), total, "assentos com nome repetido");
        }
    }

    #[test]
    fn mesa_fora_da_faixa_responde_erro_e_nao_panico() {
        let state = table_state();
        for count in [0usize, 1, 5, 9] {
            let Err(message) = commander_table(&state, count) else {
                panic!("mesa de {count} passou e não devia");
            };
            assert!(
                message.contains("de 2 a 4"),
                "mensagem de {count} assentos não diz a faixa: {message}"
            );
        }
    }

    #[test]
    fn formato_de_duelo_recusa_tres_assentos() {
        let state = table_state();
        let seats: Vec<SeatRequest> = (0..3).map(|_| seat("goblin-onslaught")).collect();
        let Err(message) = build_table(&state, &seats, Some("modern"), Observer::Spectator, 1)
        else {
            panic!("Modern aceitou três jogadores");
        };
        assert!(message.contains("duelo"), "mensagem não explica o limite: {message}");
    }

    #[test]
    fn formato_desconhecido_vira_mensagem_e_nao_padrao_silencioso() {
        let state = table_state();
        let seats = vec![seat("goblin-onslaught"), seat("azorius-control")];
        let Err(message) = build_table(&state, &seats, Some("vintage"), Observer::Spectator, 1)
        else {
            panic!("formato inventado foi aceito");
        };
        assert!(message.contains("vintage"), "mensagem não cita o formato: {message}");
    }

    /// Formato omitido é o duelo casual — é o que a forma antiga do frame
    /// significava, e trocar esse padrão mudaria o resultado de cliente antigo.
    #[test]
    fn formato_omitido_cai_em_casual() {
        let state = table_state();
        let seats = vec![seat("goblin-onslaught"), seat("azorius-control")];
        let table = match build_table(&state, &seats, None, Observer::Spectator, 1) {
            Ok(t) => t,
            Err(err) => panic!("duelo casual devia montar: {err}"),
        };
        assert_eq!(table.format, Format::Casual);
    }

    #[test]
    fn deck_de_sessenta_em_commander_devolve_as_violacoes_de_uma_vez() {
        let state = table_state();
        let seats = vec![seat("goblin-onslaught"), seat("conclave-of-emmara")];
        let Err(message) = build_table(&state, &seats, Some("commander"), Observer::Spectator, 1)
        else {
            panic!("deck de 60 passou em Commander");
        };
        assert!(message.contains("Assento 1"), "erro não diz o assento: {message}");
        // CR 903.5a (tamanho) e CR 903.3 (comandante) falham juntas, e as duas
        // têm de chegar na mesma resposta.
        assert!(message.contains("100"), "erro não cita o tamanho: {message}");
        assert!(message.contains("comandante"), "erro não cita o comandante: {message}");
        assert!(!message.contains("Assento 2"), "assento válido virou problema: {message}");
    }

    #[test]
    fn deck_desconhecido_no_assento_vira_mensagem() {
        let state = table_state();
        let seats = vec![seat("conclave-of-emmara"), seat("deck-que-nao-existe")];
        let Err(message) = build_table(&state, &seats, Some("commander"), Observer::Spectator, 1)
        else {
            panic!("deck inexistente foi aceito");
        };
        assert!(message.contains("deck-que-nao-existe"), "mensagem inútil: {message}");
    }

    #[test]
    fn comandante_pedido_pelo_cliente_ganha_do_declarado_na_lista() {
        let state = table_state();
        let mut seats = vec![seat("conclave-of-emmara"), seat("conclave-of-emmara")];
        seats[0].commander = Some("Adeliz, the Cinder Wind".to_string());
        let table = match build_table(&state, &seats, Some("commander"), Observer::Spectator, 1) {
            Ok(t) => t,
            Err(err) => panic!("troca de comandante devia montar: {err}"),
        };
        let esperado = state.db.id_by_name("Adeliz, the Cinder Wind");
        assert!(esperado.is_some(), "catálogo sem a carta: o teste não afirmaria nada");
        assert_eq!(table.seats[0].commander, esperado);
        assert_ne!(table.seats[1].commander, esperado, "assento 2 devia manter o da lista");

        seats[0].commander = Some("Ornitorrinco Lendário".to_string());
        let Err(message) = build_table(&state, &seats, Some("commander"), Observer::Spectator, 1)
        else {
            panic!("comandante inexistente foi aceito");
        };
        assert!(message.contains("Ornitorrinco"), "mensagem não cita a carta: {message}");
    }

    #[test]
    fn bot_pedido_chega_ao_assento_e_ausente_cai_no_padrao() {
        let state = table_state();
        let mut seats = vec![seat("conclave-of-emmara"), seat("storm-of-adeliz")];
        seats[0].bot = Some("greedy".to_string());
        let table = match build_table(&state, &seats, Some("commander"), Observer::Spectator, 1) {
            Ok(t) => t,
            Err(err) => panic!("mesa devia montar: {err}"),
        };
        assert_eq!(table.seats[0].bot, "greedy");
        assert_eq!(table.seats[1].bot, mtg_ai::DEFAULT_BOT);
    }

    #[test]
    fn nome_de_assento_repetido_ganha_sufixo() {
        let mut taken: Vec<String> = Vec::new();
        assert_eq!(unique_name("Emmara", &mut taken), "Emmara");
        assert_eq!(unique_name("Emmara", &mut taken), "Emmara #2");
        assert_eq!(unique_name("Emmara", &mut taken), "Emmara #3");
        assert_eq!(unique_name("Adeliz", &mut taken), "Adeliz");
    }

    /// Único teste que mexe em `MTG_DB_PATH` — o valor `:memory:` evita que
    /// rodar a suíte crie arquivo de banco na árvore do repositório.
    #[test]
    fn router_builds_with_all_catalog_routes() {
        std::env::set_var("MTG_DB_PATH", ":memory:");
        let db = match mtg_cards::build_database() {
            Ok(db) => db,
            Err(err) => panic!("catálogo não carregou: {err}"),
        };
        let state = Arc::new(AppState::new(Arc::new(db), Vec::new()));
        // Rota duplicada ou conflito de padrão viraria pânico aqui, não 404
        // em produção.
        let _router = build_router(state);
    }
}
