//! Heurísticas de decisão para bots.
//!
//! A base forma uma pilha: `eval` monta o `Snapshot` e dá a nota estática,
//! `cards` lê o IR da carta para classificar papel e valor, `table` guarda o
//! retrato dos oponentes que não estão em foco e o relógio de comandante,
//! `politics` decide contra quem jogar numa mesa de três ou quatro, e `sim` dá
//! o passo à frente sobre o snapshot. Em cima dela vivem os agentes — `bots`
//! (`RandomBot` e a fábrica `bot_by_name`), `heuristic` e `greedy`.
//!
//! Nada aqui muta `Game` — decisão de bot é leitura pura, e o motor continua
//! sendo o único dono do estado.
#![forbid(unsafe_code)]

pub mod bots;
pub mod cards;
pub mod eval;
pub mod greedy;
pub mod heuristic;
pub mod politics;
pub mod sim;
pub mod table;

pub use bots::{bot_by_name, GreedyBot, HeuristicBot, RandomBot, BOT_NAMES, DEFAULT_BOT};
pub use table::COMMANDER_LETHAL;
