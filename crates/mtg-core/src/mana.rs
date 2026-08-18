//! Cor, custo e pool de mana.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Color {
    White,
    Blue,
    Black,
    Red,
    Green,
}

impl Color {
    pub const ALL: [Color; 5] = [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn letter(self) -> char {
        match self {
            Color::White => 'W',
            Color::Blue => 'U',
            Color::Black => 'B',
            Color::Red => 'R',
            Color::Green => 'G',
        }
    }
    pub fn from_letter(c: char) -> Option<Color> {
        match c.to_ascii_uppercase() {
            'W' => Some(Color::White),
            'U' => Some(Color::Blue),
            'B' => Some(Color::Black),
            'R' => Some(Color::Red),
            'G' => Some(Color::Green),
            _ => None,
        }
    }
}

/// Conjunto de cores como bitmask. Identidade de cor de um objeto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ColorSet(pub u8);

impl ColorSet {
    pub const COLORLESS: ColorSet = ColorSet(0);
    pub fn single(c: Color) -> Self {
        ColorSet(1 << c.index())
    }
    pub fn contains(self, c: Color) -> bool {
        self.0 & (1 << c.index()) != 0
    }
    pub fn insert(&mut self, c: Color) {
        self.0 |= 1 << c.index();
    }
    pub fn union(self, other: ColorSet) -> ColorSet {
        ColorSet(self.0 | other.0)
    }
    pub fn intersects(self, other: ColorSet) -> bool {
        self.0 & other.0 != 0
    }
    pub fn is_colorless(self) -> bool {
        self.0 == 0
    }
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }
    pub fn iter(self) -> impl Iterator<Item = Color> {
        Color::ALL.into_iter().filter(move |c| self.contains(*c))
    }
    pub fn is_monocolored(self) -> bool {
        self.count() == 1
    }
    pub fn is_multicolored(self) -> bool {
        self.count() >= 2
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ManaSymbol {
    Generic(u8),
    Colored(Color),
    Colorless,
    /// {W/U} — paga com qualquer uma das duas.
    Hybrid(Color, Color),
    /// {2/W} — paga 2 genérico ou a cor.
    MonoHybrid(Color),
    /// {W/P} — paga a cor ou 2 vidas.
    Phyrexian(Color),
    Snow,
    X,
}

impl ManaSymbol {
    /// Contribuição para o valor de mana (CR 202.3). X conta 0 fora da pilha.
    pub fn mana_value(self) -> u32 {
        match self {
            ManaSymbol::Generic(n) => n as u32,
            ManaSymbol::X => 0,
            ManaSymbol::MonoHybrid(_) => 2,
            _ => 1,
        }
    }
    pub fn colors(self) -> ColorSet {
        let mut s = ColorSet::COLORLESS;
        match self {
            ManaSymbol::Colored(c)
            | ManaSymbol::MonoHybrid(c)
            | ManaSymbol::Phyrexian(c) => s.insert(c),
            ManaSymbol::Hybrid(a, b) => {
                s.insert(a);
                s.insert(b);
            }
            _ => {}
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ManaCost {
    pub symbols: Vec<ManaSymbol>,
}

impl ManaCost {
    pub const FREE: ManaCost = ManaCost { symbols: Vec::new() };

    pub fn mana_value(&self) -> u32 {
        self.symbols.iter().map(|s| s.mana_value()).sum()
    }
    pub fn colors(&self) -> ColorSet {
        self.symbols
            .iter()
            .fold(ColorSet::COLORLESS, |acc, s| acc.union(s.colors()))
    }
    pub fn has_x(&self) -> bool {
        self.symbols.iter().any(|s| matches!(s, ManaSymbol::X))
    }
    pub fn is_free(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// Pool de mana flutuante de um jogador. Esvazia no fim de cada passo (CR 500.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ManaPool {
    /// WUBRG na ordem de `Color::ALL`.
    pub colored: [u16; 5],
    pub colorless: u16,
}

impl ManaPool {
    pub fn total(&self) -> u32 {
        self.colored.iter().map(|n| *n as u32).sum::<u32>() + self.colorless as u32
    }
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
    pub fn add(&mut self, c: Option<Color>, amount: u16) {
        match c {
            Some(c) => self.colored[c.index()] += amount,
            None => self.colorless += amount,
        }
    }
    pub fn get(&self, c: Option<Color>) -> u16 {
        match c {
            Some(c) => self.colored[c.index()],
            None => self.colorless,
        }
    }
    pub fn take(&mut self, c: Option<Color>, amount: u16) -> bool {
        let slot = match c {
            Some(c) => &mut self.colored[c.index()],
            None => &mut self.colorless,
        };
        if *slot < amount {
            return false;
        }
        *slot -= amount;
        true
    }
    pub fn clear(&mut self) {
        *self = ManaPool::default();
    }
}
