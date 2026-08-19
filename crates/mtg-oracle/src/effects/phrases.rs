//! Sintagmas nominais: quem é o alvo, que conjunto o filtro descreve.
//!
//! Vocabulário separado do verbo de propósito: "destrua", "exile" e "vire"
//! aceitam exatamente os mesmos sintagmas, e é isso que faz um reconhecedor
//! novo custar cinco linhas em vez de um padrão inteiro.
//!
//! Toda função daqui casa o sintagma INTEIRO. Palavra sobrando devolve `None`,
//! porque "criatura alvo que um oponente controla" e "criatura alvo" são alvos
//! diferentes, e escolher o errado é carta que joga diferente do que está
//! escrito.

use mtg_core::ir::{Filter, ObjRef, PlayerRef, Selector, ZoneScope};
use mtg_core::mana::Color;
use mtg_core::types::{CardType, Supertype};

use super::Ctx;
use crate::keywords::parse_keyword;
use crate::text::parse_count;

/// Sintagma na posição de objeto: "target creature you control", "~", "it".
pub(super) fn object_phrase(ctx: &mut Ctx, phrase: &str) -> Option<ObjRef> {
    let p = phrase.trim();
    match p {
        "~" | "itself" => return Some(ObjRef::SelfObject),
        // "it" sem alvo anterior só pode ser a própria fonte — é a leitura de
        // "When ~ enters, it deals 2 damage...". Com alvo anterior, "it" é o
        // alvo, como em "Untap target creature. It gets +2/+4...".
        "it" => return Some(ctx.last_object().unwrap_or(ObjRef::SelfObject)),
        "that creature" | "that permanent" | "that artifact" | "that enchantment"
        | "that land" | "that token" => return ctx.last_object(),
        _ => {}
    }
    let rest = p.strip_prefix("target ")?;
    let (filter, owner) = filter_phrase(rest)?;
    let mut selector = Selector::battlefield(filter);
    selector.owner_scope = owner;
    ctx.object(selector, p)
}

/// Sintagma de objeto no cemitério: "target creature card from your graveyard".
pub(super) fn graveyard_phrase(ctx: &mut Ctx, phrase: &str) -> Option<ObjRef> {
    let p = phrase.trim();
    let head = p.strip_suffix(" from your graveyard")?;
    let rest = head.strip_prefix("target ")?.strip_suffix(" card")?;
    let (filter, owner) = filter_phrase(rest)?;
    // "de um cemitério" e "do cemitério de um oponente" são outros conjuntos.
    if owner.is_some() {
        return None;
    }
    let selector = Selector {
        zone: ZoneScope::Graveyard,
        filter,
        owner_scope: Some(PlayerRef::You),
        max: None,
    };
    ctx.object(selector, p)
}

/// Sintagma de jogador. `None` quando o sintagma não fala de jogador.
pub(super) fn player_phrase(ctx: &mut Ctx, phrase: &str) -> Option<PlayerRef> {
    let p = phrase.trim();
    let who = match p {
        "you" | "yourself" => PlayerRef::You,
        "each opponent" | "each of your opponents" => PlayerRef::Opponents,
        "each player" => PlayerRef::Each,
        "~'s controller" => PlayerRef::ControllerOf(Box::new(ObjRef::SelfObject)),
        "target player" => return ctx.player(PlayerRef::Each, p),
        "target opponent" => return ctx.player(PlayerRef::Opponents, p),
        _ => return None,
    };
    Some(who)
}

/// "any target" — criatura, planeswalker ou jogador (CR 115.4).
pub(super) fn any_target(ctx: &mut Ctx) -> Option<ObjRef> {
    let objects = Selector::battlefield(Filter::Or(vec![
        Filter::HasType(CardType::Creature),
        Filter::HasType(CardType::Planeswalker),
    ]));
    ctx.object_or_player(objects, PlayerRef::Each, "any target")
}

/// Redação antiga de dano: "target creature or player".
pub(super) fn creature_or_player(ctx: &mut Ctx) -> Option<ObjRef> {
    ctx.object_or_player(
        Selector::creatures(),
        PlayerRef::Each,
        "target creature or player",
    )
}

/// O filtro de um sintagma e, quando dito, de quem é o permanente.
pub(super) fn filter_phrase(phrase: &str) -> Option<(Filter, Option<PlayerRef>)> {
    let mut rest = phrase.trim();
    let mut owner = None;
    for (suffix, who) in [
        (" you control", PlayerRef::You),
        (" you don't control", PlayerRef::Opponents),
        (" an opponent controls", PlayerRef::Opponents),
        (" your opponents control", PlayerRef::Opponents),
    ] {
        if let Some(head) = rest.strip_suffix(suffix) {
            owner = Some(who);
            rest = head.trim();
            break;
        }
    }

    let mut extra: Vec<Filter> = Vec::new();
    if let Some((head, tail)) = rest.split_once(" with ") {
        extra.push(with_qualifier(tail)?);
        rest = head.trim();
    }

    let base = disjunction(rest)?;
    Some((combine(base, extra), owner))
}

/// Quantidade na frente do sintagma: "two creatures" -> (2, "creatures").
/// Sem número explícito a quantidade é um, como em "sacrifice a creature".
pub(super) fn split_count(phrase: &str) -> (i32, &str) {
    let p = phrase.trim();
    match p.split_once(' ') {
        Some((head, tail)) => match parse_count(head) {
            Some(n) => (n, tail.trim()),
            None => (1, p),
        },
        None => (1, p),
    }
}

// ---------------------------------------------------------------------------
// Interior do sintagma
// ---------------------------------------------------------------------------

/// Adjetivos e substantivos de um sintagma, ainda separados.
#[derive(Debug, Default)]
struct Group {
    adjectives: Vec<Filter>,
    nouns: Vec<Filter>,
}

/// "artifact or enchantment" e "attacking or blocking creature".
///
/// A segunda forma divide o substantivo entre as alternativas: só a última
/// parte o traz escrito. Ler as duas do mesmo jeito daria "atacando" (qualquer
/// permanente atacando) em vez de "criatura atacando".
fn disjunction(phrase: &str) -> Option<Filter> {
    if !phrase.contains(" or ") {
        return group_filter(&noun_group(phrase)?, None);
    }
    let mut groups: Vec<Group> = Vec::new();
    for part in phrase.split(" or ") {
        groups.push(noun_group(part)?);
    }
    let shared: Option<Vec<Filter>> = groups
        .iter()
        .rev()
        .find(|g| !g.nouns.is_empty())
        .map(|g| g.nouns.clone());
    let mut alts = Vec::new();
    for g in &groups {
        alts.push(group_filter(g, shared.as_deref())?);
    }
    Some(Filter::Or(alts))
}

fn group_filter(group: &Group, shared: Option<&[Filter]>) -> Option<Filter> {
    let mut parts = group.adjectives.clone();
    if group.nouns.is_empty() {
        parts.extend(shared?.iter().cloned());
    } else {
        parts.extend(group.nouns.iter().cloned());
    }
    match parts.len() {
        0 => None,
        1 => parts.pop(),
        _ => Some(Filter::And(parts)),
    }
}

fn noun_group(phrase: &str) -> Option<Group> {
    let mut group = Group::default();
    for word in phrase.split_whitespace() {
        // Artigo não filtra nada: "a creature you control" é o mesmo conjunto
        // que "creature you control".
        if matches!(word, "a" | "an" | "the") {
            continue;
        }
        if let Some(f) = adjective(word) {
            group.adjectives.push(f);
            continue;
        }
        group.nouns.push(noun(word)?);
    }
    Some(group)
}

fn adjective(word: &str) -> Option<Filter> {
    let f = match word {
        "another" | "other" => Filter::IsOther,
        "attacking" => Filter::Attacking,
        "blocking" => Filter::Blocking,
        "blocked" => Filter::Blocked,
        "unblocked" => Filter::Unblocked,
        "tapped" => Filter::Tapped,
        "untapped" => Filter::Untapped,
        "token" => Filter::Token,
        "nontoken" => Filter::NonToken,
        "legendary" => Filter::HasSupertype(Supertype::Legendary),
        "basic" => Filter::HasSupertype(Supertype::Basic),
        "snow" => Filter::HasSupertype(Supertype::Snow),
        "colorless" => Filter::Colorless,
        "multicolored" => Filter::Multicolored,
        "nonland" => Filter::Not(Box::new(Filter::HasType(CardType::Land))),
        "noncreature" => Filter::Not(Box::new(Filter::HasType(CardType::Creature))),
        "nonartifact" => Filter::Not(Box::new(Filter::HasType(CardType::Artifact))),
        _ => {
            if let Some(color) = word.strip_prefix("non").and_then(color_by_name) {
                return Some(Filter::Not(Box::new(Filter::HasColor(color))));
            }
            return color_by_name(word).map(Filter::HasColor);
        }
    };
    Some(f)
}

fn noun(word: &str) -> Option<Filter> {
    let singular = word.strip_suffix('s').unwrap_or(word);
    let f = match singular {
        "permanent" => Filter::Any,
        "creature" => Filter::HasType(CardType::Creature),
        "artifact" => Filter::HasType(CardType::Artifact),
        "enchantment" => Filter::HasType(CardType::Enchantment),
        "land" => Filter::HasType(CardType::Land),
        "planeswalker" => Filter::HasType(CardType::Planeswalker),
        "battle" => Filter::HasType(CardType::Battle),
        _ => return None,
    };
    Some(f)
}

/// "with flying", "with power 2 or less", "with mana value 3 or less".
fn with_qualifier(tail: &str) -> Option<Filter> {
    let t = tail.trim();
    if let Some(k) = parse_keyword(t) {
        return Some(Filter::HasKeyword(k));
    }
    if let Some(rest) = t.strip_prefix("power ") {
        return bound(rest, Filter::PowerAtMost, Filter::PowerAtLeast);
    }
    if let Some(rest) = t.strip_prefix("toughness ") {
        return bound(rest, Filter::ToughnessAtMost, Filter::ToughnessAtLeast);
    }
    if let Some(rest) = t.strip_prefix("mana value ") {
        let n = rest.strip_suffix(" or less").and_then(parse_count)?;
        return u32::try_from(n).ok().map(Filter::ManaValueAtMost);
    }
    None
}

fn bound(
    rest: &str,
    at_most: fn(i32) -> Filter,
    at_least: fn(i32) -> Filter,
) -> Option<Filter> {
    if let Some(n) = rest.strip_suffix(" or less").and_then(parse_count) {
        return Some(at_most(n));
    }
    if let Some(n) = rest.strip_suffix(" or greater").and_then(parse_count) {
        return Some(at_least(n));
    }
    None
}

fn combine(base: Filter, extra: Vec<Filter>) -> Filter {
    if extra.is_empty() {
        return base;
    }
    let mut all = vec![base];
    all.extend(extra);
    Filter::And(all)
}

pub(super) fn color_by_name(name: &str) -> Option<Color> {
    match name.trim() {
        "white" => Some(Color::White),
        "blue" => Some(Color::Blue),
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        _ => None,
    }
}
