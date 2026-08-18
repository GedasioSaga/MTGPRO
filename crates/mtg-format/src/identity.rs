//! Identidade de cor (CR 903.4).
//!
//! Identidade não é a cor da carta: é a cor do custo, mais a do indicador de
//! cor, mais **todo símbolo de mana colorido que apareça no texto de regras** —
//! e, em terreno, mais a cor dos tipos básicos de terreno que ele tenha
//! (CR 903.4d). Por isso um Llanowar Elves e um Terreno que produz {G} têm a
//! mesma identidade, ainda que o terreno seja incolor.
use mtg_core::card::{Ability, CardDef, ManaProduction};
use mtg_core::ir::Cost;
use mtg_core::mana::{Color, ColorSet, ManaSymbol};

/// CR 903.4 — identidade de cor da carta impressa.
pub fn color_identity(card: &CardDef) -> ColorSet {
    // CR 903.4 — custo de mana e indicador de cor.
    let mut set = card.colors();

    // CR 903.4d — tipo básico de terreno carrega a cor do mana que ele produz.
    for (subtype, color) in BASIC_LAND_TYPES {
        if card.type_line.has_subtype(subtype) {
            set.insert(color);
        }
    }

    // CR 903.4 — símbolos de mana no texto de regras.
    for ability in &card.abilities {
        match ability {
            Ability::Activated(a) => absorb_cost(&a.cost, &mut set),
            Ability::Mana(a) => {
                absorb_cost(&a.cost, &mut set);
                absorb_production(&a.production, &mut set);
            }
            // Palavra-chave, estático, disparado e substituição não carregam
            // custo de mana no IR: o que eles têm de colorido já veio do custo
            // da carta. Ficam de fora de propósito — se um dia passarem a ter
            // `Cost`, este `match` para de compilar e o autor é obrigado a
            // decidir o que fazer.
            Ability::Keyword(_)
            | Ability::Static(_)
            | Ability::Triggered(_)
            | Ability::Replacement(_) => {}
        }
    }
    set
}

const BASIC_LAND_TYPES: [(&str, Color); 5] = [
    ("Plains", Color::White),
    ("Island", Color::Blue),
    ("Swamp", Color::Black),
    ("Mountain", Color::Red),
    ("Forest", Color::Green),
];

fn absorb_cost(cost: &Cost, set: &mut ColorSet) {
    match cost {
        Cost::Mana(symbols) => {
            for s in symbols {
                absorb_symbol(*s, set);
            }
        }
        Cost::Composite(parts) => {
            for p in parts {
                absorb_cost(p, set);
            }
        }
        _ => {}
    }
}

fn absorb_production(production: &ManaProduction, set: &mut ColorSet) {
    match production {
        ManaProduction::Fixed(symbols) | ManaProduction::OneOf(symbols) => {
            for s in symbols {
                absorb_symbol(*s, set);
            }
        }
        ManaProduction::Dynamic { symbol, .. } => absorb_symbol(*symbol, set),
        // "Adicione um mana de qualquer cor" não é símbolo colorido: é por isso
        // que o Prophetic Prism continua incolor e o Birds of Paradise continua
        // só verde.
        ManaProduction::AnyColor(_) => {}
    }
}

fn absorb_symbol(symbol: ManaSymbol, set: &mut ColorSet) {
    for c in symbol.colors().iter() {
        set.insert(c);
    }
}

/// Cores de `set` em ordem WUBRG — a forma legível usada nas violações.
pub fn colors_of(set: ColorSet) -> Vec<Color> {
    set.iter().collect()
}
