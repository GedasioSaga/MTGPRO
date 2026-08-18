use super::Game;
use crate::action::{Action, ActionError, ManaSourceChoice};
use crate::ids::PlayerId;
use crate::ir::Cost;
#[derive(Debug, Clone, Default)]
pub struct ManaAvailability { pub by_color: [u16;5], pub any_color: u16, pub colorless: u16, pub total: u16 }
pub fn priority_actions(_g: &Game, _p: PlayerId) -> Vec<Action> { Vec::new() }
pub fn execute(_g: &mut Game, _p: PlayerId, _a: Action) -> Result<(), ActionError> { Ok(()) }
pub fn can_pay(_g: &Game, _p: PlayerId, _c: &Cost) -> bool { false }
pub fn pay_cost(_g: &mut Game, _p: PlayerId, _c: &Cost, _pl: &[ManaSourceChoice]) -> Result<(), ActionError> { Ok(()) }
pub fn available_mana(_g: &Game) -> ManaAvailability { ManaAvailability::default() }
