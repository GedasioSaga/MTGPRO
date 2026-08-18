//! Formatos de jogo e validação de lista de deck.
//!
//! Este crate responde a uma pergunta só: *esta lista pode ser jogada neste
//! formato?* Ele conhece as regras de construção (CR 100.2 e CR 903) e nada
//! além disso — não sabe simular partida, não sabe ler carta de arquivo e não
//! sabe o que é banimento. Legalidade por carta é dado externo e entra por
//! `LegalitySource`, cuja fonte real é o campo `legalities` do Scryfall.
//!
//! Camadas:
//!   `format`   — o que cada formato exige
//!   `identity` — identidade de cor (CR 903.4)
//!   `legality` — o contrato de consulta e dois provedores
//!   `deck`     — a lista declarada pelo jogador
//!   `validate` — a checagem, que devolve **todas** as violações de uma vez
#![forbid(unsafe_code)]

pub mod deck;
pub mod format;
pub mod identity;
pub mod legality;
pub mod validate;

pub use deck::DeckList;
pub use format::Format;
pub use identity::color_identity;
pub use legality::{CardLegality, CatalogLegality, InMemoryLegality, LegalitySource};
pub use validate::{validate, validate_with, Violation};
