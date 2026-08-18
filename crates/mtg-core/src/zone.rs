//! Zonas do jogo (CR 400).
use crate::ids::{ObjectId, PlayerId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ZoneKind {
    Library,
    Hand,
    Battlefield,
    Graveyard,
    Stack,
    Exile,
    Command,
}

impl ZoneKind {
    /// Zonas ocultas: conteúdo não é informação pública.
    pub fn is_hidden(self) -> bool {
        matches!(self, ZoneKind::Library | ZoneKind::Hand)
    }
    /// Zonas compartilhadas não têm dono (CR 400.1).
    pub fn is_shared(self) -> bool {
        matches!(self, ZoneKind::Battlefield | ZoneKind::Stack | ZoneKind::Exile)
    }
    /// Ordem importa (biblioteca, cemitério, pilha).
    pub fn is_ordered(self) -> bool {
        matches!(self, ZoneKind::Library | ZoneKind::Graveyard | ZoneKind::Stack)
    }
}

/// Endereço completo de uma zona. `owner` é `None` para zonas compartilhadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZoneId {
    pub kind: ZoneKind,
    pub owner: Option<PlayerId>,
}

impl ZoneId {
    pub const BATTLEFIELD: ZoneId = ZoneId { kind: ZoneKind::Battlefield, owner: None };
    pub const STACK: ZoneId = ZoneId { kind: ZoneKind::Stack, owner: None };
    pub const EXILE: ZoneId = ZoneId { kind: ZoneKind::Exile, owner: None };
    pub fn library(p: PlayerId) -> Self { ZoneId { kind: ZoneKind::Library, owner: Some(p) } }
    pub fn hand(p: PlayerId) -> Self { ZoneId { kind: ZoneKind::Hand, owner: Some(p) } }
    pub fn graveyard(p: PlayerId) -> Self { ZoneId { kind: ZoneKind::Graveyard, owner: Some(p) } }
}

/// Sequência ordenada de objetos. Índice 0 = topo para biblioteca e pilha.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zone {
    pub id_kind: Option<ZoneKind>,
    pub objects: Vec<ObjectId>,
}

impl Zone {
    pub fn new(kind: ZoneKind) -> Self {
        Zone { id_kind: Some(kind), objects: Vec::new() }
    }
    pub fn len(&self) -> usize { self.objects.len() }
    pub fn is_empty(&self) -> bool { self.objects.is_empty() }
    pub fn contains(&self, id: ObjectId) -> bool { self.objects.contains(&id) }
    pub fn push_top(&mut self, id: ObjectId) { self.objects.insert(0, id); }
    pub fn push_bottom(&mut self, id: ObjectId) { self.objects.push(id); }
    pub fn pop_top(&mut self) -> Option<ObjectId> {
        if self.objects.is_empty() { None } else { Some(self.objects.remove(0)) }
    }
    pub fn peek_top(&self) -> Option<ObjectId> { self.objects.first().copied() }
    pub fn remove(&mut self, id: ObjectId) -> bool {
        if let Some(pos) = self.objects.iter().position(|x| *x == id) {
            self.objects.remove(pos);
            true
        } else {
            false
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = ObjectId> + '_ { self.objects.iter().copied() }
}
