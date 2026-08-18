use super::Game;
use crate::action::{Action, TargetChoice};
use crate::ids::{ObjectId, PlayerId};
use crate::state::StackItem;
pub fn put_triggers_on_stack(_g: &mut Game) {}
pub fn resolve_top(_g: &mut Game) {}
pub fn counter_item(_g: &mut Game, _s: ObjectId) {}
pub fn push_spell(_g: &mut Game, _o: ObjectId, _c: PlayerId, _t: Vec<TargetChoice>, _x: u32, _m: Vec<u8>) {}
pub fn push_activated(_g: &mut Game, _s: ObjectId, _i: u16, _c: PlayerId, _t: Vec<TargetChoice>, _x: u32) {}
pub fn trigger_order_options(_t: &[ObjectId]) -> Vec<Action> { Vec::new() }
pub fn peek(_g: &Game) -> Option<&StackItem> { None }
