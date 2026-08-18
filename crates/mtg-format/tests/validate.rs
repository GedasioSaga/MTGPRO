//! Um teste por linha da tabela de formatos, mais os três casos que a tabela
//! não diz em voz alta: terreno básico isento do limite de cópias, identidade
//! de cor em Commander, e o fato de `validate` devolver a lista inteira de
//! problemas de uma vez.
//!
//! O catálogo aqui é sintético de propósito. Se estes testes lessem
//! `cards/*.lua`, passariam a falhar quando alguém editasse uma carta — e o
//! que está sendo testado é a regra de formato, não o conteúdo do catálogo.

use mtg_core::card::{Ability, CardDatabase, CardDef, ManaAbility, ManaProduction};
use mtg_core::ir::{Condition, Cost};
use mtg_core::mana::{Color, ManaCost, ManaSymbol};
use mtg_core::types::{CardType, Rarity, Supertype, TypeLine};

use mtg_format::deck::DeckList;
use mtg_format::format::Format;
use mtg_format::legality::InMemoryLegality;
use mtg_format::validate::{validate_with, Violation};

// ---------------------------------------------------------------------------
// Catálogo sintético
// ---------------------------------------------------------------------------

fn cost(symbols: &[ManaSymbol]) -> ManaCost {
    ManaCost { symbols: symbols.to_vec() }
}

fn g(n: u8) -> ManaSymbol {
    ManaSymbol::Generic(n)
}

fn c(color: Color) -> ManaSymbol {
    ManaSymbol::Colored(color)
}

fn base(name: &str, rarity: Rarity, type_line: TypeLine, mana_cost: ManaCost) -> CardDef {
    CardDef {
        id: mtg_core::ids::CardDefId(0),
        name: name.to_string(),
        mana_cost,
        type_line,
        color_override: None,
        power: None,
        toughness: None,
        loyalty: None,
        abilities: Vec::new(),
        spell_effect: None,
        spell_targets: Vec::new(),
        oracle_text: String::new(),
        flavor_text: None,
        rarity,
        set_code: "TST".to_string(),
        collector_number: String::new(),
        artist: None,
        art_key: Some(name.to_string()),
    }
}

fn line(supertypes: &[Supertype], types: &[CardType], subtypes: &[&str]) -> TypeLine {
    TypeLine {
        supertypes: supertypes.to_vec(),
        types: types.to_vec(),
        subtypes: subtypes.iter().map(|s| s.to_string()).collect(),
    }
}

fn creature(name: &str, rarity: Rarity, symbols: &[ManaSymbol]) -> CardDef {
    let mut card = base(name, rarity, line(&[], &[CardType::Creature], &["Bear"]), cost(symbols));
    card.power = Some(2);
    card.toughness = Some(2);
    card
}

fn legend(name: &str, rarity: Rarity, symbols: &[ManaSymbol]) -> CardDef {
    let mut card = base(
        name,
        rarity,
        line(&[Supertype::Legendary], &[CardType::Creature], &["Elf"]),
        cost(symbols),
    );
    card.power = Some(2);
    card.toughness = Some(2);
    card
}

fn instant(name: &str, rarity: Rarity, symbols: &[ManaSymbol]) -> CardDef {
    base(name, rarity, line(&[], &[CardType::Instant], &[]), cost(symbols))
}

fn basic(name: &str, subtype: &str, color: Color) -> CardDef {
    let mut card = base(
        name,
        Rarity::Common,
        line(&[Supertype::Basic], &[CardType::Land], &[subtype]),
        ManaCost::FREE,
    );
    card.abilities.push(Ability::Mana(ManaAbility {
        cost: Cost::Tap,
        production: ManaProduction::Fixed(vec![ManaSymbol::Colored(color)]),
        restriction: Condition::Always,
        text: format!("{{T}}: Add {{{}}}.", color.letter()),
    }));
    card
}

/// Catálogo mínimo, o mesmo para todos os testes.
fn db() -> CardDatabase {
    let mut db = CardDatabase {
        cards: vec![
            basic("Plains", "Plains", Color::White),
            basic("Forest", "Forest", Color::Green),
            basic("Island", "Island", Color::Blue),
            creature("Grizzly Bears", Rarity::Common, &[g(1), c(Color::Green)]),
            creature("Serra Angel", Rarity::Uncommon, &[g(3), c(Color::White), c(Color::White)]),
            instant("Lightning Bolt", Rarity::Common, &[c(Color::Red)]),
            instant("Counterspell", Rarity::Common, &[c(Color::Blue), c(Color::Blue)]),
            base("Sol Ring", Rarity::Uncommon, line(&[], &[CardType::Artifact], &[]), cost(&[g(1)])),
            legend("Emmara", Rarity::Rare, &[g(1), c(Color::Green), c(Color::White)]),
        ],
    };
    db.reindex();
    assert!(db.len() >= 9, "catálogo de teste incompleto: {}", db.len());
    db
}

/// Fonte de legalidade em que tudo do catálogo é legal em todo formato, para
/// que um teste de tamanho não falhe por causa de legalidade.
fn tudo_legal(db: &CardDatabase) -> InMemoryLegality {
    let mut src = InMemoryLegality::new();
    for card in &db.cards {
        src.insert_everywhere(&card.name, card.rarity);
    }
    src
}

// ---------------------------------------------------------------------------
// Montagem de lista
// ---------------------------------------------------------------------------

fn lista(format: Format, commander: Option<&str>, cards: &[(&str, u8)]) -> DeckList {
    DeckList {
        name: "Teste".to_string(),
        description: String::new(),
        colors: Vec::new(),
        format,
        commander: commander.map(|s| s.to_string()),
        cards: cards.iter().map(|(n, q)| (n.to_string(), *q)).collect(),
    }
}

/// Violações de uma lista. `Ok` vira vetor vazio, para o teste poder comparar
/// sem `if let` — teste que só afirma dentro de um `if` não afirma nada.
fn violacoes(deck: &DeckList, db: &CardDatabase, format: Format) -> Vec<Violation> {
    match validate_with(deck, db, format, &tudo_legal(db)) {
        Ok(()) => Vec::new(),
        Err(v) => v,
    }
}

fn tem<F: Fn(&Violation) -> bool>(violations: &[Violation], pred: F) -> bool {
    violations.iter().any(pred)
}

// ---------------------------------------------------------------------------
// Uma linha da tabela por teste
// ---------------------------------------------------------------------------

#[test]
fn standard_exige_sessenta_cartas_quatro_copias_e_carta_legal() {
    let db = db();

    let ok = lista(Format::Standard, None, &[("Forest", 36), ("Grizzly Bears", 4), ("Plains", 20)]);
    assert_eq!(ok.size(), 60);
    assert_eq!(violacoes(&ok, &db, Format::Standard), Vec::new());

    let pequeno = lista(Format::Standard, None, &[("Forest", 55), ("Grizzly Bears", 4)]);
    assert!(
        tem(&violacoes(&pequeno, &db, Format::Standard), |v| matches!(
            v,
            Violation::TooFewCards { found: 59, required: 60 }
        )),
        "59 cartas passaram em Standard"
    );

    let copias = lista(Format::Standard, None, &[("Forest", 55), ("Grizzly Bears", 5)]);
    assert!(
        tem(&violacoes(&copias, &db, Format::Standard), |v| matches!(
            v,
            Violation::TooManyCopies { count: 5, max: 4, .. }
        )),
        "cinco cópias passaram em Standard"
    );

    // Legalidade: `Lightning Bolt` é retirado da lista de Standard e a mesma
    // lista, que antes passava, deixa de passar.
    let mut src = tudo_legal(&db);
    src.insert("Lightning Bolt", Rarity::Common, &[Format::Modern, Format::Commander]);
    let com_bolt =
        lista(Format::Standard, None, &[("Forest", 52), ("Plains", 4), ("Lightning Bolt", 4)]);
    let Err(violations) = validate_with(&com_bolt, &db, Format::Standard, &src) else {
        panic!("carta fora de Standard passou na validação");
    };
    assert!(
        tem(&violations, |v| matches!(v, Violation::NotLegalInFormat {
            card,
            format: Format::Standard
        } if card == "Lightning Bolt")),
        "esperada NotLegalInFormat, veio {violations:?}"
    );
}

#[test]
fn modern_exige_sessenta_cartas_quatro_copias_e_carta_legal() {
    let db = db();

    let ok = lista(Format::Modern, None, &[("Forest", 56), ("Lightning Bolt", 4)]);
    assert_eq!(ok.size(), 60);
    assert_eq!(violacoes(&ok, &db, Format::Modern), Vec::new());

    let copias = lista(Format::Modern, None, &[("Forest", 54), ("Lightning Bolt", 6)]);
    assert!(
        tem(&violacoes(&copias, &db, Format::Modern), |v| matches!(
            v,
            Violation::TooManyCopies { count: 6, max: 4, .. }
        )),
        "seis cópias passaram em Modern"
    );

    // Uma carta legal em Standard mas não em Modern não existe na prática, mas
    // o inverso sim: aqui `Counterspell` sai da lista de Modern.
    let mut src = tudo_legal(&db);
    src.insert("Counterspell", Rarity::Common, &[Format::Commander]);
    let com_counter = lista(Format::Modern, None, &[("Island", 56), ("Counterspell", 4)]);
    let Err(violations) = validate_with(&com_counter, &db, Format::Modern, &src) else {
        panic!("carta fora de Modern passou na validação");
    };
    assert!(
        tem(&violations, |v| matches!(v, Violation::NotLegalInFormat {
            card,
            format: Format::Modern
        } if card == "Counterspell")),
        "esperada NotLegalInFormat, veio {violations:?}"
    );
}

#[test]
fn pauper_so_aceita_carta_de_raridade_comum() {
    let db = db();

    let ok = lista(Format::Pauper, None, &[("Forest", 56), ("Grizzly Bears", 4)]);
    assert_eq!(ok.size(), 60);
    assert_eq!(violacoes(&ok, &db, Format::Pauper), Vec::new());

    // `Serra Angel` é incomum: passa em Standard e em Modern, não em Pauper.
    let com_incomum = lista(Format::Pauper, None, &[("Plains", 56), ("Serra Angel", 4)]);
    let violations = violacoes(&com_incomum, &db, Format::Pauper);
    assert!(
        tem(&violations, |v| matches!(v, Violation::NotCommon {
            card,
            rarity: Rarity::Uncommon
        } if card == "Serra Angel")),
        "carta incomum passou em Pauper: {violations:?}"
    );
    assert_eq!(
        violacoes(&com_incomum, &db, Format::Modern),
        Vec::new(),
        "a mesma lista tinha de ser legal em Modern — senão o teste mediu outra coisa"
    );
}

#[test]
fn commander_exige_cem_cartas_singleton_e_comandante_lendario() {
    let db = db();

    let ok = lista(
        Format::Commander,
        Some("Emmara"),
        &[("Forest", 49), ("Plains", 49), ("Grizzly Bears", 1)],
    );
    assert_eq!(ok.size(), 100);
    assert_eq!(violacoes(&ok, &db, Format::Commander), Vec::new());

    // 99 + comandante = 100. Uma carta a menos e a lista morre.
    let pequeno = lista(Format::Commander, Some("Emmara"), &[("Forest", 49), ("Plains", 49)]);
    assert!(
        tem(&violacoes(&pequeno, &db, Format::Commander), |v| matches!(
            v,
            Violation::TooFewCards { found: 99, required: 100 }
        )),
        "deck de 99 passou em Commander"
    );

    // Uma a mais também.
    let grande = lista(
        Format::Commander,
        Some("Emmara"),
        &[("Forest", 50), ("Plains", 49), ("Grizzly Bears", 1)],
    );
    assert!(
        tem(&violacoes(&grande, &db, Format::Commander), |v| matches!(
            v,
            Violation::TooManyCards { found: 101, allowed: 100 }
        )),
        "deck de 101 passou em Commander"
    );

    // Singleton: duas cópias de algo que não é terreno básico.
    let repetido = lista(
        Format::Commander,
        Some("Emmara"),
        &[("Forest", 49), ("Plains", 48), ("Grizzly Bears", 2)],
    );
    assert!(
        tem(&violacoes(&repetido, &db, Format::Commander), |v| matches!(
            v,
            Violation::NotSingleton { count: 2, .. }
        )),
        "duas cópias passaram num formato singleton"
    );

    // Comandante que não é criatura lendária.
    let sem_lenda = lista(
        Format::Commander,
        Some("Grizzly Bears"),
        &[("Forest", 49), ("Plains", 49), ("Serra Angel", 1)],
    );
    assert!(
        tem(&violacoes(&sem_lenda, &db, Format::Commander), |v| matches!(
            v,
            Violation::CommanderNotLegendary { card } if card == "Grizzly Bears"
        )),
        "criatura não lendária virou comandante"
    );

    // Nenhum comandante declarado.
    let sem_comandante =
        lista(Format::Commander, None, &[("Forest", 50), ("Plains", 49), ("Grizzly Bears", 1)]);
    assert!(
        tem(&violacoes(&sem_comandante, &db, Format::Commander), |v| matches!(
            v,
            Violation::MissingCommander
        )),
        "deck de Commander sem comandante passou"
    );
}

#[test]
fn casual_aceita_quarenta_cartas_e_qualquer_numero_de_copias() {
    let db = db();

    let ok = lista(Format::Casual, None, &[("Forest", 20), ("Grizzly Bears", 20)]);
    assert_eq!(ok.size(), 40);
    assert_eq!(violacoes(&ok, &db, Format::Casual), Vec::new());

    // As mesmas 20 cópias seriam quatro violações em Modern — é isso que faz
    // este teste dizer alguma coisa sobre `Casual`, e não sobre nada.
    assert!(
        tem(&violacoes(&ok, &db, Format::Modern), |v| matches!(
            v,
            Violation::TooManyCopies { .. }
        )),
        "20 cópias passaram até em Modern: o teste não mediu Casual"
    );

    let pequeno = lista(Format::Casual, None, &[("Forest", 20), ("Grizzly Bears", 19)]);
    assert!(
        tem(&violacoes(&pequeno, &db, Format::Casual), |v| matches!(
            v,
            Violation::TooFewCards { found: 39, required: 40 }
        )),
        "39 cartas passaram em Casual"
    );

    // `Casual` não tem lista de banidos: mesmo com a fonte negando tudo, nada
    // vira NotLegalInFormat.
    let vazia = InMemoryLegality::new();
    assert_eq!(validate_with(&ok, &db, Format::Casual, &vazia), Ok(()));
}

// ---------------------------------------------------------------------------
// Os três casos fora da tabela
// ---------------------------------------------------------------------------

#[test]
fn terreno_basico_isento_do_limite_de_copias() {
    let db = db();

    // CR 100.2a e CR 903.5b — em todo formato, e inclusive no singleton.
    for (format, terrenos, outras) in [
        (Format::Standard, 56u8, 4u8),
        (Format::Modern, 56, 4),
        (Format::Pauper, 56, 4),
        (Format::Casual, 36, 4),
    ] {
        let deck = lista(format, None, &[("Forest", terrenos), ("Grizzly Bears", outras)]);
        assert_eq!(
            violacoes(&deck, &db, format),
            Vec::new(),
            "{format}: {terrenos} florestas viraram violação"
        );
    }

    let commander =
        lista(Format::Commander, Some("Emmara"), &[("Forest", 49), ("Plains", 49), ("Grizzly Bears", 1)]);
    assert_eq!(
        violacoes(&commander, &db, Format::Commander),
        Vec::new(),
        "49 florestas viraram violação de singleton"
    );

    // O contraste que prova que a isenção é por ser básico, e não por ser
    // terreno ou por nada: a mesma quantidade de uma carta não-básica cai.
    let nao_basico = lista(Format::Modern, None, &[("Forest", 4), ("Grizzly Bears", 56)]);
    assert!(
        tem(&violacoes(&nao_basico, &db, Format::Modern), |v| matches!(
            v,
            Violation::TooManyCopies { count: 56, max: 4, .. }
        )),
        "56 cópias de uma carta não-básica passaram"
    );
}

#[test]
fn commander_rejeita_carta_fora_da_identidade_de_cor() {
    let db = db();

    // Emmara é {1}{G}{W}: identidade GW. `Counterspell` é azul.
    let deck = lista(
        Format::Commander,
        Some("Emmara"),
        &[("Forest", 49), ("Plains", 49), ("Counterspell", 1)],
    );
    let violations = violacoes(&deck, &db, Format::Commander);
    assert!(
        tem(&violations, |v| matches!(v, Violation::OutsideColorIdentity { card, identity, .. }
            if card == "Counterspell" && identity == &vec![Color::Blue])),
        "carta azul passou num comandante GW: {violations:?}"
    );

    // CR 903.5c vale para terreno básico também: uma Ilha é tão azul quanto o
    // Counterspell, pelo mana que produz.
    let com_ilha =
        lista(Format::Commander, Some("Emmara"), &[("Forest", 49), ("Plains", 49), ("Island", 1)]);
    let violations = violacoes(&com_ilha, &db, Format::Commander);
    assert!(
        tem(&violations, |v| matches!(v, Violation::OutsideColorIdentity { card, .. }
            if card == "Island")),
        "Ilha passou num comandante GW: {violations:?}"
    );

    // E o contraste: carta incolor entra em qualquer identidade.
    let com_artefato =
        lista(Format::Commander, Some("Emmara"), &[("Forest", 49), ("Plains", 49), ("Sol Ring", 1)]);
    assert_eq!(
        violacoes(&com_artefato, &db, Format::Commander),
        Vec::new(),
        "artefato incolor foi barrado por identidade de cor"
    );
}

#[test]
fn validate_devolve_todas_as_violacoes_de_uma_vez() {
    let db = db();

    // Uma lista com quatro problemas independentes: pequena demais, sem
    // comandante, com carta repetida e com carta fora do catálogo.
    let deck = lista(
        Format::Commander,
        None,
        &[("Forest", 10), ("Grizzly Bears", 3), ("Black Lotus", 1)],
    );
    let Err(violations) = validate_with(&deck, &db, Format::Commander, &tudo_legal(&db)) else {
        panic!("lista cheia de erros passou na validação");
    };

    assert!(tem(&violations, |v| matches!(v, Violation::MissingCommander)), "{violations:?}");
    assert!(
        tem(&violations, |v| matches!(v, Violation::TooFewCards { found: 14, required: 100 })),
        "{violations:?}"
    );
    assert!(
        tem(&violations, |v| matches!(v, Violation::NotSingleton { card, count: 3 } if card == "Grizzly Bears")),
        "{violations:?}"
    );
    assert!(
        tem(&violations, |v| matches!(v, Violation::UnknownCard { card } if card == "Black Lotus")),
        "{violations:?}"
    );
    assert!(
        violations.len() >= 4,
        "validate parou no primeiro problema: só devolveu {violations:?}"
    );

    // Determinismo: a mesma lista tem de dar exatamente a mesma sequência.
    let de_novo = validate_with(&deck, &db, Format::Commander, &tudo_legal(&db));
    assert_eq!(de_novo, Err(violations));
}

// ---------------------------------------------------------------------------
// Identidade de cor, isolada
// ---------------------------------------------------------------------------

#[test]
fn identidade_de_cor_vem_do_custo_e_do_mana_produzido() {
    let db = db();
    let casos = [
        ("Plains", vec![Color::White]),
        ("Forest", vec![Color::Green]),
        ("Grizzly Bears", vec![Color::Green]),
        ("Emmara", vec![Color::White, Color::Green]),
        ("Sol Ring", vec![]),
    ];
    for (nome, esperado) in casos {
        let Some(card) = db.by_name(nome) else { panic!("catálogo de teste sem {nome}") };
        let identidade: Vec<Color> = mtg_format::color_identity(card).iter().collect();
        assert_eq!(identidade, esperado, "identidade de {nome}");
    }
}
