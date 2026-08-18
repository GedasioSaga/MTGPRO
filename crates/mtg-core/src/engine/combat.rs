use super::Game;
use crate::action::Action;
use crate::event::Defender;
use crate::ids::{ObjectId, PlayerId};

pub fn attack_options(_game: &Game, _player: PlayerId, _eligible: &[ObjectId]) -> Vec<Action> { Vec::new() }
pub fn block_options(_game: &Game, _player: PlayerId, _eligible: &[ObjectId], _attackers: &[ObjectId]) -> Vec<Action> { Vec::new() }
pub fn order_options(_attacker: ObjectId, _blockers: &[ObjectId]) -> Vec<Action> { Vec::new() }
pub fn damage_assignment_options(_game: &Game, _attacker: ObjectId, _blockers: &[ObjectId], _total: i32) -> Vec<Action> { Vec::new() }
pub fn eligible_attackers(_game: &Game, _player: PlayerId) -> Vec<ObjectId> { Vec::new() }
pub fn eligible_blockers(_game: &Game, _player: PlayerId) -> Vec<ObjectId> { Vec::new() }
pub fn can_block(_game: &Game, _blocker: ObjectId, _attacker: ObjectId) -> bool { false }
pub fn declare_attackers(_game: &mut Game, _assignments: &[(ObjectId, Defender)]) {}
pub fn declare_blockers(_game: &mut Game, _assignments: &[(ObjectId, ObjectId)]) {}
pub fn combat_damage_step(_game: &mut Game, _first_strike: bool) {}
pub fn end_combat(_game: &mut Game) {}
pub fn has_first_strike_creatures(_game: &Game) -> bool { false }
