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

impl Rarity {
    /// Nome usado pelo Scryfall e gravado no banco. Ida e volta com
    /// `from_slug`; há teste garantindo.
    pub fn slug(self) -> &'static str {
        match self {
            Rarity::Common => "common",
            Rarity::Uncommon => "uncommon",
            Rarity::Rare => "rare",
            Rarity::Mythic => "mythic",
            Rarity::Special => "special",
        }
    }

    /// Converte o texto do Scryfall. `bonus` é a raridade das folhas de bônus
    /// e não tem variante própria: cai em `Special`, que é o que ela é na
    /// prática para qualquer regra de construção.
    pub fn from_slug(s: &str) -> Option<Rarity> {
        match s.trim().to_ascii_lowercase().as_str() {
            "common" => Some(Rarity::Common),
            "uncommon" => Some(Rarity::Uncommon),
            "rare" => Some(Rarity::Rare),
            "mythic" => Some(Rarity::Mythic),
            "special" | "bonus" => Some(Rarity::Special),
            _ => None,
        }
    }
}

#[cfg(test)]
mod rarity_tests {
    use super::Rarity;

    const ALL: [Rarity; 5] = [
        Rarity::Common,
        Rarity::Uncommon,
        Rarity::Rare,
        Rarity::Mythic,
        Rarity::Special,
    ];

    #[test]
    fn raridade_faz_ida_e_volta_pelo_slug() {
        for r in ALL {
            let Some(voltou) = Rarity::from_slug(r.slug()) else {
                panic!("slug '{}' de {r:?} nao volta a ser raridade", r.slug());
            };
            assert_eq!(voltou, r);
        }
        assert_eq!(Rarity::from_slug("MYTHIC"), Some(Rarity::Mythic));
        assert_eq!(Rarity::from_slug("bonus"), Some(Rarity::Special));
        assert_eq!(Rarity::from_slug("lendaria"), None);
    }
}
