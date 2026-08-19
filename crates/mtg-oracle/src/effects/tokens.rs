//! "create two 1/1 red Goblin creature tokens with haste".
//!
//! Só a forma completa vira ficha: quantidade, P/T, cor, subtipo e o
//! substantivo "creature". Ficha predefinida — "create a Treasure token" — tem
//! comportamento próprio que não está escrito na carta, e inventá-la daria uma
//! ficha 0/0 sem habilidade nenhuma no lugar de um artefato que produz mana.

use mtg_core::ir::{Effect, Keyword, PlayerRef, TokenSpec, Value};
use mtg_core::mana::Color;
use mtg_core::types::{CardType, TypeLine};

use super::phrases::color_by_name;
use crate::keywords::parse_keyword;
use crate::text::parse_count;

/// Palavras que mudam a ficha e que este reconhecedor não sabe representar.
/// Sem esta lista elas virariam subtipo ("Legendary Goblin"), que é ficha
/// diferente da impressa.
const NOT_A_SUBTYPE: [&str; 6] = ["legendary", "tapped", "attacking", "snow", "copy", "that"];

pub(super) fn parse_create_token(sentence: &str) -> Option<Effect> {
    let rest = sentence.strip_prefix("create ")?;
    let (count_word, rest) = rest.split_once(' ')?;
    let count = parse_count(count_word)?;
    let (head, tail) = rest.split_once(" token")?;

    let tail = tail.trim_start_matches('s').trim();
    let keywords = if tail.is_empty() {
        Vec::new()
    } else {
        granted_keywords(tail.strip_prefix("with ")?)?
    };

    let (head, types) = split_types(head)?;
    let mut words = head.split_whitespace();
    let (power, toughness) = split_pt(words.next()?)?;

    let mut colors: Vec<Color> = Vec::new();
    let mut subtypes: Vec<String> = Vec::new();
    for word in words {
        if word == "and" || word == "colorless" {
            continue;
        }
        if let Some(c) = color_by_name(word) {
            colors.push(c);
            continue;
        }
        if NOT_A_SUBTYPE.contains(&word) {
            return None;
        }
        subtypes.push(capitalize(word));
    }

    let name = if subtypes.is_empty() {
        "Token".to_string()
    } else {
        subtypes.join(" ")
    };
    let spec = TokenSpec {
        name,
        type_line: TypeLine { supertypes: Vec::new(), types, subtypes },
        colors,
        power,
        toughness,
        keywords,
        art_key: None,
    };
    Some(Effect::CreateToken {
        spec,
        count: Value::Const(count),
        controller: PlayerRef::You,
    })
}

/// Separa o corpo do tipo da ficha. A ordem importa: "artifact creature" tem
/// que ser testado antes de "creature", senão "artifact" viraria subtipo.
fn split_types(head: &str) -> Option<(&str, Vec<CardType>)> {
    for (suffix, types) in [
        (" artifact creature", vec![CardType::Artifact, CardType::Creature]),
        (
            " enchantment creature",
            vec![CardType::Enchantment, CardType::Creature],
        ),
        (" creature", vec![CardType::Creature]),
    ] {
        if let Some(body) = head.strip_suffix(suffix) {
            return Some((body.trim(), types));
        }
    }
    None
}

/// "1/1" — número puro dos dois lados. "*/*" fica de fora: ficha com P/T
/// variável depende de uma característica que o `TokenSpec` não guarda.
fn split_pt(word: &str) -> Option<(i32, i32)> {
    let (p, t) = word.split_once('/')?;
    Some((p.trim().parse::<i32>().ok()?, t.trim().parse::<i32>().ok()?))
}

fn granted_keywords(list: &str) -> Option<Vec<Keyword>> {
    let flattened = list.replace(", and ", " and ").replace(", ", " and ");
    let mut out = Vec::new();
    for token in flattened.split(" and ") {
        out.push(parse_keyword(token.trim())?);
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
