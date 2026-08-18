use super::Game;
use crate::card::TriggerCondition;
use crate::event::GameEvent;
use crate::ids::ObjectId;
use crate::state::TriggerContext;
pub fn collect(_g: &mut Game) {}
pub fn matches(_g: &Game, _c: &TriggerCondition, _e: &GameEvent, _s: ObjectId) -> Option<TriggerContext> { None }
pub fn fire_step_triggers(_g: &mut Game) {}
