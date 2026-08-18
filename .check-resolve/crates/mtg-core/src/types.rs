//! Linha de tipos, supertipos, subtipos, marcadores.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CardType {
    Artifact,
    Battle,
    Creature,
    Enchantment,
    Instant,
    Land,
    Planeswalker,
    Sorcery,
    Kindred,
}

impl CardType {
    /// CR 110.4a — tipos que produzem permanentes.
    pub fn is_permanent(self) -> bool {
        matches!(
            self,
            CardType::Artifact
                | CardType::Battle
                | CardType::Creature
                | CardType::Enchantment
                | CardType::Land
                | CardType::Planeswalker
        )
    }
    pub fn is_spell_only(self) -> bool {
        matches!(self, CardType::Instant | CardType::Sorcery)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Supertype {
    Basic,
    Legendary,
    Snow,
    World,
}

/// Subtipo é string livre: o conjunto muda a cada set e não cabe em enum fechado.
pub type Subtype = String;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TypeLine {
    pub supertypes: Vec<Supertype>,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
}

impl TypeLine {
    pub fn has_type(&self, t: CardType) -> bool {
        self.types.contains(&t)
    }
    pub fn has_supertype(&self, t: Supertype) -> bool {
        self.supertypes.contains(&t)
    }
    pub fn has_subtype(&self, s: &str) -> bool {
        self.subtypes.iter().any(|x| x.eq_ignore_ascii_case(s))
    }
    pub fn is_permanent(&self) -> bool {
        self.types.iter().any(|t| t.is_permanent())
    }
    pub fn is_creature(&self) -> bool {
        self.has_type(CardType::Creature)
    }
    pub fn is_land(&self) -> bool {
        self.has_type(CardType::Land)
    }
    pub fn render(&self) -> String {
        let mut s = String::new();
        for sup in &self.supertypes {
            s.push_str(&format!("{sup:?} "));
        }
        let types: Vec<String> = self.types.iter().map(|t| format!("{t:?}")).collect();
        s.push_str(&types.join(" "));
        if !self.subtypes.is_empty() {
            s.push_str(" — ");
            s.push_str(&self.subtypes.join(" "));
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CounterKind {
    PlusOnePlusOne,
    MinusOneMinusOne,
    Loyalty,
    Charge,
    Poison,
    Stun,
    Shield,
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Mythic,
    Special,
}
