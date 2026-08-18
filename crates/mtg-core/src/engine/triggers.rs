use super::Game;
use crate::card::TriggerCondition;
use crate::event::GameEvent;
use crate::ids::ObjectId;
use crate::state::TriggerContext;
pub fn collect(_game: &mut Game) {}
pub fn matches(_game: &Game, _cond: &TriggerCondition, _ev: &GameEvent, _source: ObjectId) -> Option<TriggerContext> { None }
pub fn fire_step_triggers(_game: &mut Game) {}
