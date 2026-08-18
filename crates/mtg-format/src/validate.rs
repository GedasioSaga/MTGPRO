//! Validação de lista de deck.
//!
//! Regra da casa: **devolver todas as violações de uma vez**. Quem está
//! montando deck quer a lista inteira do que está errado, não descobrir um
//! problema por rodada de validação.
use mtg_core::card::{CardDatabase, CardDef};
use mtg_core::mana::{Color, ColorSet};
use mtg_core::types::{CardType, Rarity, Supertype};

use crate::deck::DeckList;
use crate::format::Format;
use crate::identity::{color_identity, colors_of};
use crate::legality::{CatalogLegality, LegalitySource};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Violation {
    #[error("deck tem {found} cartas, mínimo do formato é {required}")]
    TooFewCards { found: u32, required: u32 },

    #[error("deck tem {found} cartas, máximo do formato é {allowed}")]
    TooManyCards { found: u32, allowed: u32 },

    #[error("{card}: {count} cópias, máximo é {max}")]
    TooManyCopies { card: String, count: u8, max: u8 },

    #[error("{card}: {count} cópias num formato singleton")]
    NotSingleton { card: String, count: u8 },

    #[error("{card} não é legal em {format}")]
    NotLegalInFormat { card: String, format: Format },

    #[error("{card} é {rarity:?}, e o formato só aceita comum")]
    NotCommon { card: String, rarity: Rarity },

    #[error("{card} é {} e o comandante é {}", label_of(.identity), label_of(.commander_identity))]
    OutsideColorIdentity {
        card: String,
        identity: Vec<Color>,
        commander_identity: Vec<Color>,
    },

    #[error("{card} não é uma criatura lendária e não pode ser comandante")]
    CommanderNotLegendary { card: String },

    #[error("formato exige comandante e a lista não declara nenhum")]
    MissingCommander,

    #[error("{card} não existe no catálogo")]
    UnknownCard { card: String },
}

/// Rótulo WUBRG de uma lista de cores, para as mensagens acima.
fn label_of(colors: &[Color]) -> String {
    if colors.is_empty() {
        return "C".to_string();
    }
    colors.iter().map(|c| c.letter()).collect()
}

/// Valida a lista usando o próprio catálogo como fonte de legalidade.
///
/// Ver `CatalogLegality`: o catálogo em Lua tem raridade mas não tem lista de
/// banidos, então esta forma checa tamanho, cópias, singleton, identidade de
/// cor, comandante e raridade — não checa banimento. Para valer legalidade de
/// verdade, use `validate_with` com um provedor alimentado pelo Scryfall.
pub fn validate(
    deck: &DeckList,
    db: &CardDatabase,
    format: Format,
) -> Result<(), Vec<Violation>> {
    validate_with(deck, db, format, &CatalogLegality::new(db))
}

/// Valida a lista contra uma fonte de legalidade explícita.
pub fn validate_with<L: LegalitySource + ?Sized>(
    deck: &DeckList,
    db: &CardDatabase,
    format: Format,
    legality: &L,
) -> Result<(), Vec<Violation>> {
    let mut out = Vec::new();

    // ---- comandante (CR 903.3) ------------------------------------------
    // Vem primeiro porque é ele que define a identidade de cor contra a qual
    // o resto do deck é medido.
    let mut identity: Option<ColorSet> = None;
    if format.requires_commander() {
        match deck.commander.as_deref() {
            None => out.push(Violation::MissingCommander),
            Some(name) => match db.by_name(name) {
                None => out.push(Violation::UnknownCard { card: name.to_string() }),
                Some(card) => {
                    if !is_legendary_creature(card) {
                        out.push(Violation::CommanderNotLegendary { card: card.name.clone() });
                    }
                    identity = Some(color_identity(card));
                    check_legality(card, format, legality, &mut out);
                }
            },
        }
    }

    // ---- tamanho (CR 100.2a / CR 903.5a) --------------------------------
    let found = deck.size();
    match format.exact_deck_size() {
        Some(exact) => {
            if found < exact {
                out.push(Violation::TooFewCards { found, required: exact });
            } else if found > exact {
                out.push(Violation::TooManyCards { found, allowed: exact });
            }
        }
        None => {
            let required = format.min_deck_size();
            if found < required {
                out.push(Violation::TooFewCards { found, required });
            }
        }
    }

    // ---- carta a carta ---------------------------------------------------
    // Na ordem declarada na lista: a saída de `validate` tem de ser a mesma
    // toda vez, para a mesma lista.
    for (name, count) in &deck.cards {
        let Some(card) = db.by_name(name) else {
            out.push(Violation::UnknownCard { card: name.clone() });
            continue;
        };

        // CR 100.2a e CR 903.5b — terreno básico não tem limite de cópias, em
        // formato nenhum.
        if !is_basic_land(card) {
            match format.max_copies() {
                Some(1) if *count > 1 => {
                    out.push(Violation::NotSingleton { card: card.name.clone(), count: *count })
                }
                Some(max) if *count > max => out.push(Violation::TooManyCopies {
                    card: card.name.clone(),
                    count: *count,
                    max,
                }),
                _ => {}
            }
        }

        check_legality(card, format, legality, &mut out);

        if format.commons_only() {
            let rarity = legality.rarity(&card.name).unwrap_or(card.rarity);
            if rarity != Rarity::Common {
                out.push(Violation::NotCommon { card: card.name.clone(), rarity });
            }
        }

        // CR 903.5c — inclusive terreno básico: um Plains não entra num deck
        // cuja identidade não tem branco.
        if let Some(identity) = identity {
            let card_identity = color_identity(card);
            if !within(card_identity, identity) {
                out.push(Violation::OutsideColorIdentity {
                    card: card.name.clone(),
                    identity: colors_of(card_identity),
                    commander_identity: colors_of(identity),
                });
            }
        }
    }

    if out.is_empty() {
        Ok(())
    } else {
        Err(out)
    }
}

fn check_legality<L: LegalitySource + ?Sized>(
    card: &CardDef,
    format: Format,
    legality: &L,
    out: &mut Vec<Violation>,
) {
    if format.checks_legality() && !legality.legal_in(&card.name, format) {
        out.push(Violation::NotLegalInFormat { card: card.name.clone(), format });
    }
}

/// CR 903.3 — só criatura lendária pode ser comandante. (As exceções por texto
/// próprio, como "pode ser seu comandante", não existem no catálogo curado.)
fn is_legendary_creature(card: &CardDef) -> bool {
    card.type_line.has_supertype(Supertype::Legendary) && card.type_line.has_type(CardType::Creature)
}

fn is_basic_land(card: &CardDef) -> bool {
    card.type_line.has_supertype(Supertype::Basic) && card.type_line.is_land()
}

/// `inner` cabe inteiro em `outer`?
fn within(inner: ColorSet, outer: ColorSet) -> bool {
    inner.0 & !outer.0 == 0
}
