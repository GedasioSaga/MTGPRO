//! Identificadores estáveis. Tudo no motor é referenciado por id, nunca por
//! referência Rust — grafos de objetos (aura -> criatura -> controlador) são
//! impossíveis de expressar com &mut sem arena.
use serde::{Deserialize, Serialize};

macro_rules! newtype_id {
    ($name:ident, $inner:ty) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub $inner);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    };
}

newtype_id!(PlayerId, u8);
newtype_id!(ObjectId, u32);
newtype_id!(CardDefId, u32);
newtype_id!(EffectId, u32);

impl PlayerId {
    pub const P0: PlayerId = PlayerId(0);
    pub const P1: PlayerId = PlayerId(1);
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl ObjectId {
    pub const NONE: ObjectId = ObjectId(u32::MAX);
    pub fn is_none(self) -> bool {
        self == Self::NONE
    }
}

/// Referência a uma habilidade específica de um objeto específico.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AbilityRef {
    pub object: ObjectId,
    pub index: u16,
}

/// Gerador monotônico. Nunca reutiliza id — objeto que muda de zona vira
/// objeto novo pelas regras (CR 400.7), e id novo torna isso explícito.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdGen {
    next: u32,
}

impl IdGen {
    pub fn next_object(&mut self) -> ObjectId {
        let id = ObjectId(self.next);
        self.next += 1;
        id
    }
    pub fn next_effect(&mut self) -> EffectId {
        let id = EffectId(self.next);
        self.next += 1;
        id
    }
}

impl Default for PlayerId {
    fn default() -> Self {
        PlayerId(0)
    }
}

impl Default for ObjectId {
    fn default() -> Self {
        ObjectId::NONE
    }
}
