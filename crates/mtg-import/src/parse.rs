//! Léxico: custo de mana, linha de tipo, P/T, raridade.
//!
//! Tudo aqui é função pura de `&str` para tipo de `mtg-core`, e todo erro é
//! `Unsupported` com o texto exato que não deu para ler — esse texto vira o
//! motivo de "não jogável" e é o que diz onde vale a pena investir depois.
use mtg_core::mana::{Color, ColorSet, ManaCost, ManaSymbol};
use mtg_core::types::{CardType, Rarity, Supertype, TypeLine};

/// Um pedaço de texto que o compilador não sabe representar no IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    /// Motivo curto e estável, usado para agrupar falhas no resumo.
    pub reason: String,
    /// Texto de oráculo inteiro que não compilou, já com o nome próprio
    /// trocado por `~` e em minúsculas. Só existe quando a falha foi textual:
    /// custo de mana malformado não é padrão de oráculo e não deve entrar no
    /// relatório de cobertura.
    pub snippet: Option<String>,
}

impl Unsupported {
    pub fn new(what: impl Into<String>) -> Unsupported {
        Unsupported { reason: what.into(), snippet: None }
    }

    /// Falha textual: guarda o trecho exato, que vira padrão no relatório.
    pub fn text(reason: impl Into<String>, snippet: impl Into<String>) -> Unsupported {
        Unsupported { reason: reason.into(), snippet: Some(snippet.into()) }
    }
}

pub type Parsed<T> = Result<T, Unsupported>;

// ---------------------------------------------------------------------------
// Mana
// ---------------------------------------------------------------------------

/// Converte `{2}{W}{U}` em `ManaCost`. Custo vazio é custo zero (terreno).
pub fn parse_mana_cost(text: &str) -> Parsed<ManaCost> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(ManaCost::FREE);
    }
    if text.contains("//") {
        return Err(Unsupported::new("custo de mana de carta dividida"));
    }
    let mut symbols = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let open = rest.find('{').ok_or_else(|| Unsupported::new(format!("custo de mana '{text}'")))?;
        if rest[..open].trim() != "" {
            return Err(Unsupported::new(format!("custo de mana '{text}'")));
        }
        let close = rest.find('}').ok_or_else(|| Unsupported::new(format!("custo de mana '{text}'")))?;
        if close < open {
            return Err(Unsupported::new(format!("custo de mana '{text}'")));
        }
        symbols.push(parse_mana_symbol(&rest[open + 1..close])?);
        rest = &rest[close + 1..];
    }
    Ok(ManaCost { symbols })
}

/// Um símbolo, já sem as chaves.
pub fn parse_mana_symbol(sym: &str) -> Parsed<ManaSymbol> {
    let s = sym.trim().to_ascii_uppercase();
    if s.is_empty() {
        return Err(Unsupported::new("símbolo de mana vazio"));
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        let n: u32 = s.parse().map_err(|_| Unsupported::new(format!("símbolo {{{sym}}}")))?;
        let n = u8::try_from(n).map_err(|_| Unsupported::new(format!("símbolo {{{sym}}}")))?;
        return Ok(ManaSymbol::Generic(n));
    }
    if let Some((left, right)) = s.split_once('/') {
        return parse_hybrid(left, right, sym);
    }
    match s.as_str() {
        "X" | "Y" | "Z" => Ok(ManaSymbol::X),
        "C" => Ok(ManaSymbol::Colorless),
        "S" => Ok(ManaSymbol::Snow),
        _ => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Color::from_letter(c)
                    .map(ManaSymbol::Colored)
                    .ok_or_else(|| Unsupported::new(format!("símbolo {{{sym}}}"))),
                _ => Err(Unsupported::new(format!("símbolo {{{sym}}}"))),
            }
        }
    }
}

fn parse_hybrid(left: &str, right: &str, original: &str) -> Parsed<ManaSymbol> {
    let one = |s: &str| s.chars().next().filter(|_| s.len() == 1);
    let lc = one(left).and_then(Color::from_letter);
    let rc = one(right).and_then(Color::from_letter);
    match (left, right, lc, rc) {
        (_, _, Some(a), Some(b)) => Ok(ManaSymbol::Hybrid(a, b)),
        ("2", _, _, Some(c)) => Ok(ManaSymbol::MonoHybrid(c)),
        (_, "P", Some(c), _) => Ok(ManaSymbol::Phyrexian(c)),
        _ => Err(Unsupported::new(format!("símbolo híbrido {{{original}}}"))),
    }
}

/// Cores vindas do campo `colors` do Scryfall.
pub fn parse_color_set(letters: &[String]) -> ColorSet {
    letters
        .iter()
        .filter_map(|s| s.chars().next())
        .filter_map(Color::from_letter)
        .fold(ColorSet::COLORLESS, |acc, c| acc.union(ColorSet::single(c)))
}

/// Cores em ordem WUBRG, para gravar no banco de forma estável.
pub fn color_letters(set: ColorSet) -> String {
    set.iter().map(|c| c.letter()).collect()
}

// ---------------------------------------------------------------------------
// Linha de tipo
// ---------------------------------------------------------------------------

/// Travessão do Scryfall (U+2014). Alguns registros antigos usam hífen.
const EM_DASH: char = '\u{2014}';

pub fn parse_type_line(text: &str) -> Parsed<TypeLine> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Unsupported::new("linha de tipo vazia"));
    }
    if text.contains("//") {
        return Err(Unsupported::new("linha de tipo de carta multiface"));
    }
    let (head, tail) = match text.split_once(EM_DASH) {
        Some((h, t)) => (h, Some(t)),
        None => match text.split_once(" - ") {
            Some((h, t)) => (h, Some(t)),
            None => (text, None),
        },
    };

    let mut line = TypeLine::default();
    for word in head.split_whitespace() {
        if let Some(sup) = supertype_from(word) {
            line.supertypes.push(sup);
            continue;
        }
        match card_type_from(word) {
            Some(t) => line.types.push(t),
            None => return Err(Unsupported::new(format!("tipo '{word}'"))),
        }
    }
    if line.types.is_empty() {
        return Err(Unsupported::new(format!("linha de tipo '{text}' sem tipo de carta")));
    }
    if let Some(sub) = tail {
        for word in sub.split_whitespace() {
            line.subtypes.push(word.to_string());
        }
    }
    Ok(line)
}

fn supertype_from(word: &str) -> Option<Supertype> {
    match word {
        "Basic" => Some(Supertype::Basic),
        "Legendary" => Some(Supertype::Legendary),
        "Snow" => Some(Supertype::Snow),
        "World" => Some(Supertype::World),
        _ => None,
    }
}

fn card_type_from(word: &str) -> Option<CardType> {
    match word {
        "Artifact" => Some(CardType::Artifact),
        "Battle" => Some(CardType::Battle),
        "Creature" => Some(CardType::Creature),
        "Enchantment" => Some(CardType::Enchantment),
        "Instant" => Some(CardType::Instant),
        "Land" => Some(CardType::Land),
        "Planeswalker" => Some(CardType::Planeswalker),
        "Sorcery" => Some(CardType::Sorcery),
        // "Tribal" foi renomeado para "Kindred" em 2023; o bulk tem os dois.
        "Kindred" | "Tribal" => Some(CardType::Kindred),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Números impressos
// ---------------------------------------------------------------------------

/// P/T e lealdade impressos. `*`, `1+*` e `X` são característica variável, que
/// o IR não modela em valor impresso — viram erro, não zero silencioso.
pub fn parse_printed_number(text: &str) -> Parsed<i32> {
    let t = text.trim();
    if t.is_empty() {
        return Err(Unsupported::new("número impresso vazio"));
    }
    t.parse::<i32>().map_err(|_| Unsupported::new(format!("valor variável '{t}'")))
}

pub fn parse_rarity(text: Option<&str>) -> Rarity {
    match text.unwrap_or("").to_ascii_lowercase().as_str() {
        "common" => Rarity::Common,
        "uncommon" => Rarity::Uncommon,
        "rare" => Rarity::Rare,
        "mythic" => Rarity::Mythic,
        _ => Rarity::Special,
    }
}

/// Número escrito por extenso ou em dígito, como aparece no texto de oráculo.
pub fn parse_word_number(text: &str) -> Option<i32> {
    let t = text.trim().to_ascii_lowercase();
    if let Ok(n) = t.parse::<i32>() {
        return Some(n);
    }
    let n = match t.as_str() {
        "a" | "an" | "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "twenty" => 20,
        _ => return None,
    };
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mana_cost_covers_the_usual_symbols() {
        let cost = parse_mana_cost("{2}{W}{U}").expect("custo simples");
        assert_eq!(
            cost.symbols,
            vec![
                ManaSymbol::Generic(2),
                ManaSymbol::Colored(Color::White),
                ManaSymbol::Colored(Color::Blue)
            ]
        );
        assert_eq!(cost.mana_value(), 4);

        assert_eq!(parse_mana_cost("").expect("vazio"), ManaCost::FREE);
        assert_eq!(
            parse_mana_cost("{X}{R}").expect("X").symbols,
            vec![ManaSymbol::X, ManaSymbol::Colored(Color::Red)]
        );
        assert_eq!(
            parse_mana_cost("{W/U}").expect("híbrido").symbols,
            vec![ManaSymbol::Hybrid(Color::White, Color::Blue)]
        );
        assert_eq!(
            parse_mana_cost("{2/G}").expect("mono-híbrido").symbols,
            vec![ManaSymbol::MonoHybrid(Color::Green)]
        );
        assert_eq!(
            parse_mana_cost("{B/P}").expect("phyrexiano").symbols,
            vec![ManaSymbol::Phyrexian(Color::Black)]
        );
        assert_eq!(parse_mana_cost("{C}").expect("incolor").symbols, vec![ManaSymbol::Colorless]);
        assert_eq!(parse_mana_cost("{S}").expect("neve").symbols, vec![ManaSymbol::Snow]);
    }

    #[test]
    fn mana_cost_rejects_what_it_cannot_represent() {
        assert!(parse_mana_cost("{R} // {G}").is_err(), "carta dividida não é representável");
        assert!(parse_mana_cost("{HW}").is_err(), "meio-mana não é representável");
        assert!(parse_mana_cost("R").is_err(), "símbolo sem chaves é entrada malformada");
    }

    #[test]
    fn type_line_splits_supertypes_types_and_subtypes() {
        let t = parse_type_line("Legendary Creature \u{2014} Human Wizard").expect("linha de tipo");
        assert_eq!(t.supertypes, vec![Supertype::Legendary]);
        assert_eq!(t.types, vec![CardType::Creature]);
        assert_eq!(t.subtypes, vec!["Human".to_string(), "Wizard".to_string()]);

        let land = parse_type_line("Basic Land \u{2014} Forest").expect("terreno básico");
        assert!(land.has_supertype(Supertype::Basic));
        assert!(land.is_land());

        let artifact_creature =
            parse_type_line("Artifact Creature \u{2014} Golem").expect("dois tipos");
        assert_eq!(artifact_creature.types, vec![CardType::Artifact, CardType::Creature]);
    }

    #[test]
    fn type_line_rejects_unknown_types() {
        assert!(parse_type_line("Conspiracy").is_err());
        assert!(parse_type_line("Instant // Sorcery").is_err());
        assert!(parse_type_line("").is_err());
        assert_eq!(
            parse_type_line("Tribal Instant \u{2014} Elf").expect("tribal").types,
            vec![CardType::Kindred, CardType::Instant]
        );
    }

    #[test]
    fn printed_numbers_reject_variable_values() {
        assert_eq!(parse_printed_number("3").expect("três"), 3);
        assert_eq!(parse_printed_number("-1").expect("negativo"), -1);
        assert!(parse_printed_number("*").is_err());
        assert!(parse_printed_number("1+*").is_err());
    }

    #[test]
    fn word_numbers() {
        assert_eq!(parse_word_number("a"), Some(1));
        assert_eq!(parse_word_number("three"), Some(3));
        assert_eq!(parse_word_number("7"), Some(7));
        assert_eq!(parse_word_number("many"), None);
    }
}
