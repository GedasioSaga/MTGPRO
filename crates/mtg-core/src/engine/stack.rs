use super::Game;
use crate::action::{Action, TargetChoice};
use crate::ids::{ObjectId, PlayerId};
use crate::state::StackItem;
pub fn put_triggers_on_stack(_game: &mut Game) {}
pub fn resolve_top(_game: &mut Game) {}
pub fn counter_item(_game: &mut Game, _stack_id: ObjectId) {}
pub fn push_spell(_game: &mut Game, _object: ObjectId, _controller: PlayerId, _targets: Vec<TargetChoice>, _x: u32, _modes: Vec<u8>) {}
pub fn push_activated(_game: &mut Game, _source: ObjectId, _index: u16, _controller: PlayerId, _targets: Vec<TargetChoice>, _x: u32) {}
pub fn trigger_order_options(_triggers: &[ObjectId]) -> Vec<Action> { Vec::new() }
pub fn peek(_game: &Game) -> Option<&StackItem> { None }
