use super::Game;
use super::query::EvalCtx;
use crate::action::Action;
use crate::ids::ObjectId;
use crate::ir::Effect;

pub fn resolve_effect(_game: &mut Game, _effect: &Effect, _ctx: &mut EvalCtx) {}
pub fn mode_options(_count: usize, _choose: u8) -> Vec<Action> { Vec::new() }
pub fn selection_options(_candidates: &[ObjectId], _min: u8, _max: u8) -> Vec<Action> { Vec::new() }
pub fn arrange_options(_cards: &[ObjectId]) -> Vec<Action> { Vec::new() }
