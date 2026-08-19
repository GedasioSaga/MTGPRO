//! `mtg-server` — roda a simulação bot-vs-bot e transmite para a UI web.
//!
//! Nada de regra de jogo mora aqui: este binário só monta o catálogo,
//! sobe o axum e faz a ponte entre `mtg-core::engine::Game` (síncrono) e o
//! WebSocket (assíncrono). Ver `docs/ENGINE_CONTRACT.md`, seção
//! "Protocolo de rede", para o contrato de fio com o frontend.
mod bot;
mod catalog;
mod protocol;
mod routes;
mod sim;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use crate::routes::{build_router, AppState};

const DEFAULT_PORT: u16 = 8787;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let (db, decks) = match catalog::load() {
        Ok(v) => v,
        Err(err) => {
            // Sem catálogo não há partida: falhar aqui aponta o erro para o
            // script de carta, em vez de virar 500 no primeiro `start`.
            tracing::error!(%err, "não foi possível carregar o catálogo de cartas");
            std::process::exit(1);
        }
    };
    // Só o catálogo curado em Lua. O total do banco (curadas + importadas do
    // Scryfall) é logado por `catalog::open_store`, na montagem do router.
    tracing::info!(cards = db.cards.len(), decks = decks.len(), "catálogo curado (Lua) carregado");
    let state = Arc::new(AppState::new(Arc::new(db), decks));

    let port = std::env::var("MTG_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    let app = build_router(state);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            tracing::error!(%err, %addr, "não foi possível abrir a porta — MTG_PORT já em uso?");
            std::process::exit(1);
        }
    };
    tracing::info!(%addr, "mtg-server no ar");

    if let Err(err) = axum::serve(listener, app).await {
        tracing::error!(%err, "servidor encerrou com erro");
    }
}
