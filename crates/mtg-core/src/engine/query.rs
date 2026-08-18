use super::Game;
use crate::action::TargetChoice;
use crate::ids::{ObjectId, PlayerId};
use crate::ir::{Condition, Filter, ObjRef, PlayerRef, Selector, TargetSpec, Value};
use crate::state::TriggerContext;

#[derive(Debug, Clone, Default)]
pub struct EvalCtx {
    pub source: Option<ObjectId>,
    pub controller: PlayerId,
    pub targets: Vec<TargetChoice>,
    pub x: u32,
    pub selected: Option<ObjectId>,
    pub trigger: TriggerContext,
    pub remembered: Vec<ObjectId>,
    pub chosen_number: i32,
}
impl EvalCtx {
    pub fn for_source(source: ObjectId, controller: PlayerId) -> Self {
        EvalCtx { source: Some(source), controller, ..Default::default() }
    }
}

pub fn matches_filter(_game: &Game, _obj: ObjectId, _filter: &Filter, _ctx: &EvalCtx) -> bool { false }
pub fn select(_game: &Game, _sel: &Selector, _ctx: &EvalCtx) -> Vec<ObjectId> { Vec::new() }
pub fn eval_value(_game: &Game, _v: &Value, _ctx: &EvalCtx) -> i32 { 0 }
pub fn eval_condition(_game: &Game, _c: &Condition, _ctx: &EvalCtx) -> bool { false }
pub fn resolve_players(_game: &Game, _r: &PlayerRef, _ctx: &EvalCtx) -> Vec<PlayerId> { Vec::new() }
pub fn resolve_objects(_game: &Game, _r: &ObjRef, _ctx: &EvalCtx) -> Vec<ObjectId> { Vec::new() }
pub fn legal_targets(_game: &Game, _spec: &TargetSpec, _ctx: &EvalCtx) -> Vec<TargetChoice> { Vec::new() }
pub fn can_be_targeted(_game: &Game, _obj: ObjectId, _source: Option<ObjectId>, _by: PlayerId) -> bool { false }
pub fn target_still_legal(_game: &Game, _t: TargetChoice, _spec: &TargetSpec, _ctx: &EvalCtx) -> bool { false }
