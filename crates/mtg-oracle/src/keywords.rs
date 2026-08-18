//! Linha de palavras-chave: "Flying", "Trample, haste", "Ward {2}".

use mtg_core::ir::{Cost, Filter, Keyword};
use mtg_core::mana::Color;
use mtg_core::types::CardType;

use crate::parse::parse_mana_cost;

/// Uma linha inteira só de palavras-chave separadas por vírgula.
///
/// Tudo ou nada: se um dos termos não for palavra-chave conhecida, a linha
/// não é linha de palavra-chave e cabe a outro reconhecedor (ou a ninguém).
pub fn parse_keyword_line(norm: &str) -> Option<Vec<Keyword>> {
    let body = norm.trim().trim_end_matches('.').trim();
    if body.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for token in body.split(',') {
        out.push(parse_keyword(token.trim())?);
    }
    Some(out)
}

/// Um termo isolado, já normalizado (minúsculo, sem ponto final).
pub fn parse_keyword(token: &str) -> Option<Keyword> {
    let t = token.trim();
    if let Some(k) = simple_keyword(t) {
        return Some(k);
    }
    if let Some(color) = t.strip_prefix("protection from ") {
        return color_by_name(color).map(Keyword::Protection);
    }
    if let Some(rest) = t.strip_prefix("ward ") {
        return mana_cost(rest).map(|c| Keyword::Ward(Box::new(c)));
    }
    if let Some(rest) = t.strip_prefix("flashback ") {
        return mana_cost(rest).map(|c| Keyword::Flashback(Box::new(c)));
    }
    if let Some(rest) = t.strip_prefix("kicker ") {
        return mana_cost(rest).map(|c| Keyword::Kicker(Box::new(c)));
    }
    if let Some(rest) = t.strip_prefix("cycling ") {
        return mana_cost(rest).map(|c| Keyword::Cycling(Box::new(c)));
    }
    if let Some(rest) = t.strip_prefix("equip ") {
        return mana_cost(rest).map(|c| Keyword::Equip(Box::new(c)));
    }
    if let Some(rest) = t.strip_prefix("enchant ") {
        return enchant_filter(rest).map(|f| Keyword::Enchant(Box::new(f)));
    }
    if let Some(rest) = t.strip_prefix("annihilator ") {
        return rest.trim().parse::<u8>().ok().map(Keyword::Annihilator);
    }
    if let Some(rest) = t.strip_prefix("afflict ") {
        return rest.trim().parse::<u8>().ok().map(Keyword::Afflict);
    }
    if let Some(land) = t.strip_suffix("walk") {
        return landwalk_subtype(land).map(Keyword::Landwalk);
    }
    None
}

fn simple_keyword(t: &str) -> Option<Keyword> {
    // `Keyword::Split` fica de fora: nenhuma palavra-chave impressa mapeia
    // para ela sem ambiguidade, e chutar geraria carta com comportamento
    // inventado.
    let k = match t {
        "flying" => Keyword::Flying,
        "reach" => Keyword::Reach,
        "trample" => Keyword::Trample,
        "first strike" => Keyword::FirstStrike,
        "double strike" => Keyword::DoubleStrike,
        "deathtouch" => Keyword::Deathtouch,
        "lifelink" => Keyword::Lifelink,
        "vigilance" => Keyword::Vigilance,
        "haste" => Keyword::Haste,
        "menace" => Keyword::Menace,
        "defender" => Keyword::Defender,
        "flash" => Keyword::Flash,
        "hexproof" => Keyword::Hexproof,
        "shroud" => Keyword::Shroud,
        "indestructible" => Keyword::Indestructible,
        "prowess" => Keyword::Prowess,
        "intimidate" => Keyword::Intimidate,
        "fear" => Keyword::Fear,
        "skulk" => Keyword::Skulk,
        "exalted" => Keyword::Exalted,
        "riot" => Keyword::Riot,
        "convoke" => Keyword::Convoke,
        "delve" => Keyword::Delve,
        "cascade" => Keyword::Cascade,
        "storm" => Keyword::Storm,
        _ => return None,
    };
    Some(k)
}

fn mana_cost(spec: &str) -> Option<Cost> {
    let cost = parse_mana_cost(spec.trim())?;
    if cost.symbols.is_empty() {
        return None;
    }
    Some(Cost::Mana(cost.symbols))
}

fn color_by_name(name: &str) -> Option<Color> {
    match name.trim() {
        "white" => Some(Color::White),
        "blue" => Some(Color::Blue),
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        _ => None,
    }
}

fn enchant_filter(what: &str) -> Option<Filter> {
    match what.trim() {
        "creature" => Some(Filter::HasType(CardType::Creature)),
        "artifact" => Some(Filter::HasType(CardType::Artifact)),
        "enchantment" => Some(Filter::HasType(CardType::Enchantment)),
        "land" => Some(Filter::HasType(CardType::Land)),
        "permanent" => Some(Filter::Any),
        _ => None,
    }
}

fn landwalk_subtype(land: &str) -> Option<String> {
    match land.trim() {
        "plains" => Some("Plains".to_string()),
        "island" => Some("Island".to_string()),
        "swamp" => Some("Swamp".to_string()),
        "mountain" => Some("Mountain".to_string()),
        "forest" => Some("Forest".to_string()),
        _ => None,
    }
}
