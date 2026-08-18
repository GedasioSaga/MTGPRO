//! Heurísticas de decisão para bots.
//!
//! Os três módulos formam uma pilha: `eval` monta o `Snapshot` e dá a nota
//! estática, `cards` lê o IR da carta para classificar papel e valor, e `sim`
//! faz a busca rasa sobre o snapshot. Nada aqui muta `Game` — decisão de bot
//! é leitura pura, e o motor continua sendo o único dono do estado.
#![forbid(unsafe_code)]

pub mod cards;
pub mod eval;
pub mod sim;
