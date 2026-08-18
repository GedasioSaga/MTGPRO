//! Leitura dos campos estruturados do Scryfall: custo, linha de tipo, raridade.
//!
//! Duplica de propósito o que `mtg-script` já faz: aquele crate arrasta um
//! interpretador Lua inteiro junto, e o compilador de oracle precisa rodar
//! (e ser testado) sem essa dependência.

use mtg_core::mana::{Color, ManaCost, ManaSymbol};
use mtg_core::types::{CardType, Rarity, Supertype, TypeLine};

/// Aceita a forma do Scryfall: `{2}{W}{W}`, `{X}{R}`, `{W/U}`, `{2/W}`, `{W/P}`.
/// Custo vazio é custo vazio, não erro — terreno não tem custo.
pub fn parse_mana_cost(spec: &str) -> Option<ManaCost> {
    let mut symbols = Vec::new();
    let mut rest = spec.trim();
    while !rest.is_empty() {
        let inner = rest.strip_prefix('{')?;
        let (token, tail) = inner.split_once('}')?;
        symbols.push(parse_symbol(token.trim())?);
        rest = tail.trim_start();
    }
    Some(ManaCost { symbols })
}

/// Um único símbolo entre chaves, como aparece em "Add {G}".
pub fn parse_braced_symbol(spec: &str) -> Option<ManaSymbol> {
    let inner = spec.trim().strip_prefix('{')?;
    let (token, tail) = inner.split_once('}')?;
    if !tail.trim().is_empty() {
        return None;
    }
    parse_symbol(token.trim())
}

fn parse_symbol(token: &str) -> Option<ManaSymbol> {
    if token.is_empty() {
        return None;
    }
    if token.chars().all(|c| c.is_ascii_digit()) {
        return token.parse::<u8>().ok().map(ManaSymbol::Generic);
    }
    if let Some((a, b)) = token.split_once('/') {
        if a == "2" {
            return single_color(b).map(ManaSymbol::MonoHybrid);
        }
        if b.eq_ignore_ascii_case("p") {
            return single_color(a).map(ManaSymbol::Phyrexian);
        }
        return Some(ManaSymbol::Hybrid(single_color(a)?, single_color(b)?));
    }
    match token.to_ascii_uppercase().as_str() {
        "X" => Some(ManaSymbol::X),
        "C" => Some(ManaSymbol::Colorless),
        "S" => Some(ManaSymbol::Snow),
        other => single_color(other).map(ManaSymbol::Colored),
    }
}

fn single_color(token: &str) -> Option<Color> {
    let t = token.trim();
    if t.chars().count() != 1 {
        return None;
    }
    t.chars().next().and_then(Color::from_letter)
}

/// Renderiza um símbolo de volta para o texto impresso.
pub fn render_symbol(s: ManaSymbol) -> String {
    match s {
        ManaSymbol::Generic(n) => format!("{{{n}}}"),
        ManaSymbol::Colored(c) => format!("{{{}}}", c.letter()),
        ManaSymbol::Colorless => "{C}".to_string(),
        ManaSymbol::Snow => "{S}".to_string(),
        ManaSymbol::X => "{X}".to_string(),
        ManaSymbol::Hybrid(a, b) => format!("{{{}/{}}}", a.letter(), b.letter()),
        ManaSymbol::MonoHybrid(c) => format!("{{2/{}}}", c.letter()),
        ManaSymbol::Phyrexian(c) => format!("{{{}/P}}", c.letter()),
    }
}

/// `"Legendary Creature — Human Soldier"`. Recusa linha com `//` (carta de
/// duas metades) e qualquer tipo desconhecido.
pub fn parse_type_line(spec: &str) -> Option<TypeLine> {
    if spec.contains("//") {
        return None;
    }
    let normalized = spec.replace('\u{2014}', "|").replace(" - ", "|");
    let (left, right) = match normalized.split_once('|') {
        Some((l, r)) => (l.trim().to_string(), r.trim().to_string()),
        None => (normalized.trim().to_string(), String::new()),
    };

    let mut tl = TypeLine::default();
    for word in left.split_whitespace() {
        if let Some(sup) = parse_supertype(word) {
            tl.supertypes.push(sup);
            continue;
        }
        tl.types.push(parse_card_type(word)?);
    }
    if tl.types.is_empty() {
        return None;
    }
    tl.subtypes = right.split_whitespace().map(|s| s.to_string()).collect();
    Some(tl)
}

fn parse_supertype(word: &str) -> Option<Supertype> {
    match word.to_ascii_lowercase().as_str() {
        "basic" => Some(Supertype::Basic),
        "legendary" => Some(Supertype::Legendary),
        "snow" => Some(Supertype::Snow),
        "world" => Some(Supertype::World),
        _ => None,
    }
}

fn parse_card_type(word: &str) -> Option<CardType> {
    match word.to_ascii_lowercase().as_str() {
        "artifact" => Some(CardType::Artifact),
        "battle" => Some(CardType::Battle),
        "creature" => Some(CardType::Creature),
        "enchantment" => Some(CardType::Enchantment),
        "instant" => Some(CardType::Instant),
        "land" => Some(CardType::Land),
        "planeswalker" => Some(CardType::Planeswalker),
        "sorcery" => Some(CardType::Sorcery),
        "kindred" | "tribal" => Some(CardType::Kindred),
        _ => None,
    }
}

/// Raridade desconhecida não invalida a carta: vira `Special`.
pub fn parse_rarity(spec: &str) -> Rarity {
    match spec.trim().to_ascii_lowercase().as_str() {
        "common" => Rarity::Common,
        "uncommon" => Rarity::Uncommon,
        "rare" => Rarity::Rare,
        "mythic" => Rarity::Mythic,
        _ => Rarity::Special,
    }
}

/// Mana intrínseco de um subtipo de terreno (CR 305.6). Vale para qualquer
/// terreno com o subtipo, não só para o básico: é assim que Tropical Island
/// produz duas cores sem ter uma linha de texto sequer.
pub fn land_subtype_mana(subtype: &str) -> Option<ManaSymbol> {
    match subtype.to_ascii_lowercase().as_str() {
        "plains" => Some(ManaSymbol::Colored(Color::White)),
        "island" => Some(ManaSymbol::Colored(Color::Blue)),
        "swamp" => Some(ManaSymbol::Colored(Color::Black)),
        "mountain" => Some(ManaSymbol::Colored(Color::Red)),
        "forest" => Some(ManaSymbol::Colored(Color::Green)),
        "wastes" => Some(ManaSymbol::Colorless),
        _ => None,
    }
}
