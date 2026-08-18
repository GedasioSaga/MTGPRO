//! Motor de regras de Magic: The Gathering.
//!
//! Camadas, de baixo para cima:
//!   `ids` `mana` `types` `zone`  — vocabulário
//!   `ir` `card`                  — como uma carta é descrita
//!   `event` `action` `state`     — o que acontece e o que se decide
//!   `engine`                     — as regras
//!   `view`                       — o que a interface enxerga
//!
//! Nada aqui faz I/O. Banco vive em `mtg-db`, rede em `mtg-server`.
#![forbid(unsafe_code)]

pub mod action;
pub mod card;
pub mod engine;
pub mod event;
pub mod ids;
pub mod ir;
pub mod mana;
pub mod state;
pub mod types;
pub mod view;
pub mod zone;

pub use action::{Action, ActionError, ManaSourceChoice, Request, TargetChoice};
pub use card::{Ability, CardDatabase, CardDef, TriggerCondition};
pub use engine::{Agent, Characteristics, Game, GameConfig, PlayerConfig};
pub use event::{Defender, GameEvent, LossReason, Phase, Step};
pub use ids::{CardDefId, ObjectId, PlayerId};
pub use ir::{Cost, Effect, Filter, Keyword, Selector, TargetSpec, Value};
pub use mana::{Color, ColorSet, ManaCost, ManaPool, ManaSymbol};
pub use state::{GameOutcome, GameState, ObjectState, PlayerState};
pub use types::{CardType, CounterKind, Rarity, Supertype, TypeLine};
pub use view::{CardView, GameView, MatchEvent, Observer};
pub use zone::{Zone, ZoneId, ZoneKind};
