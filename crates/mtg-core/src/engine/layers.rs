use super::{Characteristics, Game};
use crate::ids::ObjectId;
pub fn characteristics(_game: &Game, _id: ObjectId) -> Option<Characteristics> { None }
pub fn base_characteristics(_game: &Game, _id: ObjectId) -> Option<Characteristics> { None }
pub fn expire_continuous_effects(_game: &mut Game) {}
