//! Catálogo de cartas e decks embutido no binário.
//!
//! `mtg-cards` (parser de coleção) e `mtg-db` (armazenamento em SQLite) ainda
//! estão vazios — outro agente os está escrevendo em paralelo. Em vez de
//! bloquear o servidor nisso, este módulo fornece um catálogo mínimo mas
//! válido, o bastante para exercitar `/api/cards`, `/api/decks` e uma
//! partida completa fim-a-fim. Trocar pela carga real é substituir a chamada
//! a `load()` por algo como `mtg_db::load_catalog(path)` — o resto do
//! servidor não muda.
use mtg_core::card::{Ability, CardDatabase, CardDef, ManaAbility, ManaProduction};
use mtg_core::ids::CardDefId;
use mtg_core::ir::{Condition, Duration, ObjRef, TargetKind};
use mtg_core::mana::{Color, ManaCost, ManaSymbol};
use mtg_core::types::{CardType, Rarity, Supertype, TypeLine};
use mtg_core::{Effect, Keyword, Selector, TargetSpec, Value};

/// Deck jogável: id estável (usado pelo cliente em `start`), metadados para
/// a UI e a lista de cartas na proporção em que entram na biblioteca.
pub struct DeckInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub colors: Vec<String>,
    pub cards: Vec<CardDefId>,
}

/// Carrega o catálogo embutido e os decks de exemplo.
pub fn load() -> (CardDatabase, Vec<DeckInfo>) {
    let mut db = CardDatabase { cards: Vec::new() };

    let mountain = push(&mut db, mountain());
    let fagulheiro = push(&mut db, fagulheiro_veloz());
    let investida = push(&mut db, investida_ignea());
    let colosso = push(&mut db, colosso_flamejante());
    let lanca_chamas = push(&mut db, lanca_chamas());

    let forest = push(&mut db, forest());
    let elfo = push(&mut db, elfo_batedor());
    let fera = push(&mut db, fera_da_espessura());
    let forca = push(&mut db, forca_da_natureza());
    let ancestral = push(&mut db, ancestral_verdejante());

    // `reindex` garante que `CardDefId` case com a posição no vetor mesmo
    // que a ordem de inserção mude no futuro.
    db.reindex();

    let burn = DeckInfo {
        id: "burn".to_string(),
        name: "Fogo Selvagem".to_string(),
        description: "Vermelho agressivo: criaturas rápidas e dano direto à cara.".to_string(),
        colors: vec!["R".to_string()],
        cards: deck_list(&[
            (mountain, 16),
            (fagulheiro, 6),
            (investida, 8),
            (colosso, 5),
            (lanca_chamas, 5),
        ]),
    };

    let elves = DeckInfo {
        id: "elves".to_string(),
        name: "Chamado da Floresta".to_string(),
        description: "Verde de criaturas grandes, aceleração de mana e força bruta.".to_string(),
        colors: vec!["G".to_string()],
        cards: deck_list(&[
            (forest, 16),
            (elfo, 8),
            (fera, 6),
            (forca, 6),
            (ancestral, 4),
        ]),
    };

    (db, vec![burn, elves])
}

fn push(db: &mut CardDatabase, card: CardDef) -> CardDefId {
    let id = card.id;
    db.cards.push(card);
    id
}

fn deck_list(counts: &[(CardDefId, u32)]) -> Vec<CardDefId> {
    let mut out = Vec::new();
    for (id, n) in counts {
        for _ in 0..*n {
            out.push(*id);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Construtores de carta
// ---------------------------------------------------------------------------

fn type_line(supertypes: Vec<Supertype>, types: Vec<CardType>, subtypes: &[&str]) -> TypeLine {
    TypeLine {
        supertypes,
        types,
        subtypes: subtypes.iter().map(|s| s.to_string()).collect(),
    }
}

fn cost(symbols: Vec<ManaSymbol>) -> ManaCost {
    ManaCost { symbols }
}

#[allow(clippy::too_many_arguments)]
fn card(
    name: &str,
    mana_cost: ManaCost,
    tl: TypeLine,
    power: Option<i32>,
    toughness: Option<i32>,
    abilities: Vec<Ability>,
    spell_effect: Option<Effect>,
    spell_targets: Vec<TargetSpec>,
    oracle_text: &str,
    rarity: Rarity,
) -> CardDef {
    CardDef {
        // Placeholder: `CardDatabase::reindex` reatribui o id real depois
        // que todas as cartas estiverem no vetor.
        id: CardDefId(0),
        name: name.to_string(),
        mana_cost,
        type_line: tl,
        color_override: None,
        power,
        toughness,
        loyalty: None,
        abilities,
        spell_effect,
        spell_targets,
        oracle_text: oracle_text.to_string(),
        flavor_text: None,
        rarity,
        set_code: "SIM".to_string(),
        collector_number: String::new(),
        artist: None,
        art_key: None,
    }
}

fn mana_ability(color: Color, text: &str) -> Ability {
    Ability::Mana(ManaAbility {
        cost: mtg_core::Cost::Tap,
        production: ManaProduction::Fixed(vec![ManaSymbol::Colored(color)]),
        restriction: Condition::Always,
        text: text.to_string(),
    })
}

fn creature_target_spec(description: &str) -> TargetSpec {
    TargetSpec {
        kind: TargetKind::Object(Selector::creatures()),
        description: description.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Vermelho — "burn"
// ---------------------------------------------------------------------------

fn mountain() -> CardDef {
    card(
        "Mountain",
        cost(vec![]),
        type_line(vec![Supertype::Basic], vec![CardType::Land], &["Mountain"]),
        None,
        None,
        vec![mana_ability(Color::Red, "{T}: Adicione {R}.")],
        None,
        vec![],
        "{T}: Adicione {R}.",
        Rarity::Common,
    )
}

fn fagulheiro_veloz() -> CardDef {
    card(
        "Fagulheiro Veloz",
        cost(vec![ManaSymbol::Colored(Color::Red)]),
        type_line(vec![], vec![CardType::Creature], &["Goblin"]),
        Some(1),
        Some(1),
        vec![Ability::Keyword(Keyword::Haste)],
        None,
        vec![],
        "Ímpeto.",
        Rarity::Common,
    )
}

fn investida_ignea() -> CardDef {
    card(
        "Investida Ígnea",
        cost(vec![ManaSymbol::Colored(Color::Red)]),
        type_line(vec![], vec![CardType::Instant], &[]),
        None,
        None,
        vec![],
        Some(Effect::DealDamage { amount: Value::Const(3), target: ObjRef::Target(0) }),
        vec![creature_target_spec("criatura alvo")],
        "Investida Ígnea causa 3 pontos de dano à criatura alvo.",
        Rarity::Common,
    )
}

fn colosso_flamejante() -> CardDef {
    card(
        "Colosso Flamejante",
        cost(vec![ManaSymbol::Generic(3), ManaSymbol::Colored(Color::Red)]),
        type_line(vec![], vec![CardType::Creature], &["Elemental"]),
        Some(4),
        Some(4),
        vec![],
        None,
        vec![],
        "",
        Rarity::Uncommon,
    )
}

fn lanca_chamas() -> CardDef {
    card(
        "Lança-Chamas",
        cost(vec![ManaSymbol::Generic(1), ManaSymbol::Colored(Color::Red)]),
        type_line(vec![], vec![CardType::Creature], &["Human", "Soldier"]),
        Some(2),
        Some(2),
        vec![Ability::Keyword(Keyword::FirstStrike)],
        None,
        vec![],
        "Iniciativa.",
        Rarity::Common,
    )
}

// ---------------------------------------------------------------------------
// Verde — "elves"
// ---------------------------------------------------------------------------

fn forest() -> CardDef {
    card(
        "Forest",
        cost(vec![]),
        type_line(vec![Supertype::Basic], vec![CardType::Land], &["Forest"]),
        None,
        None,
        vec![mana_ability(Color::Green, "{T}: Adicione {G}.")],
        None,
        vec![],
        "{T}: Adicione {G}.",
        Rarity::Common,
    )
}

fn elfo_batedor() -> CardDef {
    card(
        "Elfo Batedor",
        cost(vec![ManaSymbol::Colored(Color::Green)]),
        type_line(vec![], vec![CardType::Creature], &["Elf", "Druid"]),
        Some(1),
        Some(1),
        vec![mana_ability(Color::Green, "{T}: Adicione {G}.")],
        None,
        vec![],
        "{T}: Adicione {G}.",
        Rarity::Common,
    )
}

fn fera_da_espessura() -> CardDef {
    card(
        "Fera da Espessura",
        cost(vec![ManaSymbol::Generic(2), ManaSymbol::Colored(Color::Green)]),
        type_line(vec![], vec![CardType::Creature], &["Beast"]),
        Some(3),
        Some(3),
        vec![],
        None,
        vec![],
        "",
        Rarity::Common,
    )
}

fn forca_da_natureza() -> CardDef {
    card(
        "Força da Natureza",
        cost(vec![ManaSymbol::Colored(Color::Green)]),
        type_line(vec![], vec![CardType::Instant], &[]),
        None,
        None,
        vec![],
        Some(Effect::ModifyPT {
            target: ObjRef::Target(0),
            power: Value::Const(3),
            toughness: Value::Const(3),
            duration: Duration::EndOfTurn,
        }),
        vec![creature_target_spec("criatura alvo")],
        "A criatura alvo recebe +3/+3 até o final do turno.",
        Rarity::Common,
    )
}

fn ancestral_verdejante() -> CardDef {
    card(
        "Ancestral Verdejante",
        cost(vec![ManaSymbol::Generic(4), ManaSymbol::Colored(Color::Green)]),
        type_line(vec![], vec![CardType::Creature], &["Treefolk"]),
        Some(5),
        Some(5),
        vec![Ability::Keyword(Keyword::Trample)],
        None,
        vec![],
        "Atropelar.",
        Rarity::Rare,
    )
}
