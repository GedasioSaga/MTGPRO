//! Frase de efeito -> IR.
//!
//! Regra que vale para todo reconhecedor daqui: casamento é da frase INTEIRA.
//! Prefixo que casa e sobra texto devolve `None`. Uma carta ausente do catálogo
//! custa uma carta; uma carta que joga diferente do que está escrito quebra a
//! partida em silêncio.

use mtg_core::ir::{
    Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind, TargetSpec, Value,
};
use mtg_core::types::CardType;

use crate::keywords::parse_keyword;
use crate::text::{parse_count, parse_signed};

/// Efeito reconhecido junto com os alvos que ele exige.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub effect: Effect,
    pub targets: Vec<TargetSpec>,
}

impl Parsed {
    fn new(effect: Effect, targets: Vec<TargetSpec>) -> Parsed {
        Parsed { effect, targets }
    }
}

const FIRST_TARGET: ObjRef = ObjRef::Target(0);

fn spec(kind: TargetKind, description: &str) -> TargetSpec {
    TargetSpec { kind, description: description.to_string() }
}

fn t_creature() -> TargetSpec {
    spec(TargetKind::Object(Selector::creatures()), "target creature")
}

/// "any target" = criatura, planeswalker ou jogador.
fn t_any() -> TargetSpec {
    let objects = Selector::battlefield(Filter::Or(vec![
        Filter::HasType(CardType::Creature),
        Filter::HasType(CardType::Planeswalker),
    ]));
    spec(TargetKind::ObjectOrPlayer(objects, PlayerRef::Each), "any target")
}

fn t_creature_or_player() -> TargetSpec {
    spec(
        TargetKind::ObjectOrPlayer(Selector::creatures(), PlayerRef::Each),
        "target creature or player",
    )
}

fn t_player() -> TargetSpec {
    spec(TargetKind::Player(PlayerRef::Each), "target player")
}

/// Ponto de entrada: uma frase normalizada vira efeito, ou nada.
pub fn parse_effect(text: &str) -> Option<Parsed> {
    let body = text.trim();
    let body = body.strip_suffix('.').unwrap_or(body).trim();
    if body.is_empty() {
        return None;
    }
    // "Destroy target creature. It can't be regenerated." é uma frase só,
    // partida em duas por convenção de template.
    if let Some(head) = body.strip_suffix(". it can't be regenerated") {
        return parse_destroy(head.trim(), true);
    }
    parse_damage(body)
        .or_else(|| parse_destroy(body, false))
        .or_else(|| parse_draw(body))
        .or_else(|| parse_pump(body))
        .or_else(|| parse_gain_life(body))
        .or_else(|| parse_counter(body))
}

// ---------------------------------------------------------------------------
// Dano direto
// ---------------------------------------------------------------------------

fn parse_damage(body: &str) -> Option<Parsed> {
    let rest = body.strip_prefix("~ deals ")?;
    let (amount, victim) = rest.split_once(" damage to ")?;
    let n = parse_count(amount)?;
    let value = Value::Const(n);
    let parsed = match victim.trim() {
        "any target" => Parsed::new(
            Effect::DealDamage { amount: value, target: FIRST_TARGET },
            vec![t_any()],
        ),
        "target creature" => Parsed::new(
            Effect::DealDamage { amount: value, target: FIRST_TARGET },
            vec![t_creature()],
        ),
        "target creature or player" => Parsed::new(
            Effect::DealDamage { amount: value, target: FIRST_TARGET },
            vec![t_creature_or_player()],
        ),
        "target player" => Parsed::new(
            Effect::DealDamageToPlayer { amount: value, player: PlayerRef::Target(0) },
            vec![t_player()],
        ),
        _ => return None,
    };
    Some(parsed)
}

// ---------------------------------------------------------------------------
// Remoção
// ---------------------------------------------------------------------------

fn parse_destroy(body: &str, no_regeneration: bool) -> Option<Parsed> {
    let what = body.strip_prefix("destroy target ")?.trim();
    let filter = permanent_filter(what)?;
    let description = format!("target {what}");
    Some(Parsed::new(
        Effect::Destroy { target: FIRST_TARGET, no_regeneration },
        vec![spec(TargetKind::Object(Selector::battlefield(filter)), &description)],
    ))
}

fn permanent_filter(what: &str) -> Option<Filter> {
    let f = match what {
        "creature" => Filter::HasType(CardType::Creature),
        "artifact" => Filter::HasType(CardType::Artifact),
        "enchantment" => Filter::HasType(CardType::Enchantment),
        "land" => Filter::HasType(CardType::Land),
        "planeswalker" => Filter::HasType(CardType::Planeswalker),
        "permanent" => Filter::Any,
        "nonland permanent" => Filter::Not(Box::new(Filter::HasType(CardType::Land))),
        "artifact or enchantment" => Filter::Or(vec![
            Filter::HasType(CardType::Artifact),
            Filter::HasType(CardType::Enchantment),
        ]),
        "creature or planeswalker" => Filter::Or(vec![
            Filter::HasType(CardType::Creature),
            Filter::HasType(CardType::Planeswalker),
        ]),
        _ => return None,
    };
    Some(f)
}

// ---------------------------------------------------------------------------
// Compra
// ---------------------------------------------------------------------------

fn parse_draw(body: &str) -> Option<Parsed> {
    let rest = body.strip_prefix("draw ")?;
    let amount = rest
        .strip_suffix(" cards")
        .or_else(|| rest.strip_suffix(" card"))?;
    let n = parse_count(amount)?;
    Some(Parsed::new(
        Effect::DrawCards { count: Value::Const(n), player: PlayerRef::You },
        Vec::new(),
    ))
}

// ---------------------------------------------------------------------------
// Pump
// ---------------------------------------------------------------------------

fn parse_pump(body: &str) -> Option<Parsed> {
    let rest = body.strip_prefix("target creature ")?;
    let rest = rest.strip_suffix(" until end of turn")?;

    // "gets +N/+N", opcionalmente "and gains <kw>"; ou só "gains <kw>".
    let (pt, keyword_part) = match rest.strip_prefix("gets ") {
        Some(after_gets) => match after_gets.split_once(" and gains ") {
            Some((pt, kws)) => (Some(pt), Some(kws)),
            None => (Some(after_gets), None),
        },
        None => (None, Some(rest.strip_prefix("gains ")?)),
    };

    let mut effects = Vec::new();
    if let Some(pt) = pt {
        let (p, t) = pt.trim().split_once('/')?;
        effects.push(Effect::ModifyPT {
            target: FIRST_TARGET,
            power: Value::Const(parse_signed(p)?),
            toughness: Value::Const(parse_signed(t)?),
            duration: Duration::EndOfTurn,
        });
    }
    if let Some(kws) = keyword_part {
        effects.push(Effect::GrantKeywords {
            target: FIRST_TARGET,
            keywords: parse_granted_keywords(kws)?,
            duration: Duration::EndOfTurn,
        });
    }

    let effect = if effects.len() == 1 {
        effects.remove(0)
    } else {
        Effect::Sequence(effects)
    };
    Some(Parsed::new(effect, vec![t_creature()]))
}

fn parse_granted_keywords(list: &str) -> Option<Vec<Keyword>> {
    let mut out = Vec::new();
    for token in list.split(" and ") {
        out.push(parse_keyword(token.trim())?);
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Vida
// ---------------------------------------------------------------------------

fn parse_gain_life(body: &str) -> Option<Parsed> {
    let amount = body.strip_prefix("you gain ")?.strip_suffix(" life")?;
    let n = parse_count(amount)?;
    Some(Parsed::new(
        Effect::GainLife { amount: Value::Const(n), player: PlayerRef::You },
        Vec::new(),
    ))
}

// ---------------------------------------------------------------------------
// Contra-magia
// ---------------------------------------------------------------------------

fn parse_counter(body: &str) -> Option<Parsed> {
    let what = body.strip_prefix("counter target ")?.strip_suffix("spell")?.trim();
    let filter = match what {
        "" => Filter::Any,
        "creature" => Filter::HasType(CardType::Creature),
        "noncreature" => Filter::Not(Box::new(Filter::HasType(CardType::Creature))),
        "artifact" => Filter::HasType(CardType::Artifact),
        "instant or sorcery" => Filter::Or(vec![
            Filter::HasType(CardType::Instant),
            Filter::HasType(CardType::Sorcery),
        ]),
        _ => return None,
    };
    let description = if what.is_empty() {
        "target spell".to_string()
    } else {
        format!("target {what} spell")
    };
    Some(Parsed::new(
        Effect::CounterSpell { target: FIRST_TARGET, unless_pays: None },
        vec![spec(TargetKind::SpellOnStack(filter), &description)],
    ))
}
