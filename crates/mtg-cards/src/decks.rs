//! Decks pré-montados.
//!
//! Duas famílias, com contratos diferentes:
//!
//! * `decks()` — os quatro construídos de 60 cartas. Quatro identidades
//!   distintas, pensadas para se baterem: o agressivo pune a curva alta, o
//!   controle pune o agressivo, o valor pune o controle e a rampa pune quem
//!   não fecha o jogo cedo.
//! * `commander_decks()` — os de 100 cartas, singleton, com comandante.
//!
//! São funções separadas de propósito. Quem monta uma partida de duelo chama
//! `decks()` e sabe que toda lista tem 60 cartas; quem quer tudo chama
//! `all_decks()`. Misturar as duas numa função só faria um deck de 100 cartas
//! aparecer onde 60 era o contrato.
//!
//! A lista é declarada por *nome* de carta, não por id: id é posição no vetor
//! do catálogo e muda quando alguém insere uma carta nova no meio de um `.lua`.

use mtg_core::card::CardDatabase;
use mtg_core::ids::CardDefId;
use mtg_core::mana::Color;

pub use mtg_format::{DeckList, Format};

fn deck(
    name: &str,
    description: &str,
    colors: &[Color],
    format: Format,
    commander: Option<&str>,
    cards: &[(&str, u8)],
) -> DeckList {
    DeckList {
        name: name.to_string(),
        description: description.to_string(),
        colors: colors.to_vec(),
        format,
        commander: commander.map(|c| c.to_string()),
        cards: cards.iter().map(|(n, c)| (n.to_string(), *c)).collect(),
    }
}

/// Os decks de 60 cartas, na ordem em que a interface deve listá-los.
///
/// **Contrato:** toda lista devolvida aqui tem exatamente 60 cartas e nenhum
/// comandante. Há teste garantindo, e o servidor depende disso.
pub fn decks() -> Vec<DeckList> {
    vec![goblin_onslaught(), azorius_control(), selesnya_valor(), gruul_stampede()]
}

/// Os decks de Commander: 100 cartas, singleton, comandante lendário.
pub fn commander_decks() -> Vec<DeckList> {
    vec![conclave_of_emmara(), storm_of_adeliz()]
}

/// Tudo que é jogável, em ordem estável.
pub fn all_decks() -> Vec<DeckList> {
    let mut out = decks();
    out.extend(commander_decks());
    out
}

/// Procura um deck pelo nome (sem diferenciar maiúsculas) e já expande em ids.
///
/// O que volta é a **biblioteca**: num deck de Commander são 99 cartas, porque
/// o comandante começa na zona de comando (CR 903.6). Use `commander_id` da
/// lista para saber quem é.
pub fn deck_by_name(db: &CardDatabase, name: &str) -> Option<Vec<CardDefId>> {
    deck_list_by_name(name).and_then(|d| d.expand(db))
}

/// A lista completa, com formato e comandante, procurada pelo nome.
pub fn deck_list_by_name(name: &str) -> Option<DeckList> {
    all_decks().into_iter().find(|d| d.name.eq_ignore_ascii_case(name))
}

// ---------------------------------------------------------------------------
// Construídos — 60 cartas
// ---------------------------------------------------------------------------

/// Mono-vermelho: curva termina no 3, 22 terrenos porque quase nada custa mais
/// que isso, e 11 mágicas de dano que servem tanto para limpar bloqueador
/// quanto para os últimos pontos de vida.
///
/// Marcado `Modern`: a lista inteira é de coleção pós-8ª edição, que é o corte
/// do formato.
fn goblin_onslaught() -> DeckList {
    deck(
        "Goblin Onslaught",
        "Agressivo mono-vermelho: criaturas baratas, pressa e dano direto para fechar antes do turno seis.",
        &[Color::Red],
        Format::Modern,
        None,
        &[
            ("Mountain", 22),
            // Criaturas (20)
            ("Raging Goblin", 4),
            ("Mogg Fanatic", 3),
            ("Goblin Piker", 4),
            ("Ember Hauler", 4),
            ("Goblin Chieftain", 3),
            ("Furnace Whelp", 2),
            // Mágicas (18)
            ("Lightning Bolt", 4),
            ("Shock", 4),
            ("Searing Spear", 4),
            ("Dragon Fodder", 3),
            ("Krenko's Command", 3),
        ],
    )
}

/// Azul-branco: 25 terrenos porque quer chegar vivo ao turno cinco, oito
/// criaturas que só existem para bloquear e comprar, e 27 respostas.
///
/// Marcado `Casual`, e não `Modern`, por causa do `Swords to Plowshares`: a
/// carta nunca foi impressa numa coleção legal em Modern. Chamar esta lista de
/// Modern seria uma afirmação falsa, e a validação não teria como pegá-la
/// enquanto a legalidade por carta não vier do Scryfall.
fn azorius_control() -> DeckList {
    deck(
        "Azorius Control",
        "Controle azul-branco: contra-magia para o que importa, remoção incondicional para o resto e compra para não ficar sem gás.",
        &[Color::White, Color::Blue],
        Format::Casual,
        None,
        &[
            ("Island", 12),
            ("Plains", 9),
            ("Tranquil Cove", 4),
            // Criaturas (8)
            ("Wall of Omens", 4),
            ("Aven Fisher", 2),
            ("Serra Angel", 2),
            // Mágicas (27)
            ("Counterspell", 4),
            ("Cancel", 3),
            ("Essence Scatter", 3),
            ("Negate", 3),
            ("Mana Leak", 2),
            ("Dismiss", 2),
            ("Divination", 3),
            ("Jace's Ingenuity", 2),
            ("Swords to Plowshares", 3),
            ("Day of Judgment", 2),
        ],
    )
}

/// Verde-branco: cada criatura paga o próprio custo com uma carta, um ponto de
/// vida ou uma ficha, então trocas em combate nunca são neutras.
fn selesnya_valor() -> DeckList {
    deck(
        "Selesnya Valor",
        "Meio-de-curva verde-branco: cada criatura traz valor ao entrar, e o anthem transforma as fichas em ameaça real.",
        &[Color::White, Color::Green],
        Format::Modern,
        None,
        &[
            ("Forest", 10),
            ("Plains", 10),
            ("Blossoming Sands", 4),
            // Criaturas (24)
            ("Elvish Visionary", 4),
            ("Sylvan Ranger", 4),
            ("Veteran Armorer", 3),
            ("Wall of Blossoms", 3),
            ("Centaur Courser", 3),
            ("Attended Knight", 3),
            ("Acidic Slime", 2),
            ("Thragtusk", 2),
            // Mágicas (12)
            ("Divine Verdict", 3),
            ("Oblivion Ring", 3),
            ("Glorious Anthem", 2),
            ("Hunt the Weak", 2),
            ("Naturalize", 2),
        ],
    )
}

/// Verde-vermelho: oito aceleradores de um mana e dois artefatos de quatro
/// sustentam uma curva que termina em 6, o que seria impossível com 23 terrenos
/// sozinhos.
fn gruul_stampede() -> DeckList {
    deck(
        "Gruul Stampede",
        "Rampa verde-vermelha: elfos no turno um pagam criaturas gigantes no turno quatro, e o dano direto abre caminho.",
        &[Color::Red, Color::Green],
        Format::Modern,
        None,
        &[
            ("Forest", 10),
            ("Mountain", 9),
            ("Rugged Highlands", 4),
            // Criaturas (26)
            ("Llanowar Elves", 4),
            ("Elvish Mystic", 4),
            ("Kalonian Tusker", 3),
            ("Leatherback Baloth", 3),
            ("Giant Spider", 3),
            ("Palladium Myr", 2),
            ("Craw Wurm", 3),
            ("Vastwood Gorger", 2),
            ("Shivan Dragon", 2),
            // Mágicas (11)
            ("Lightning Bolt", 3),
            ("Giant Growth", 3),
            ("Rabid Bite", 3),
            ("Overrun", 2),
        ],
    )
}

// ---------------------------------------------------------------------------
// Commander — 100 cartas, singleton
// ---------------------------------------------------------------------------
//
// As duas listas seguem a mesma forma: 62 cartas singleton que não são
// terreno, 2 terrenos não-básicos e 35 básicos, mais o comandante — 100 no
// total, com 37 terrenos, que é a base de mana normal do formato.
//
// `Day of Judgment` e `Runeclaw Bear` ficaram de fora do deck da Emmara de
// propósito: o primeiro destrói as próprias fichas que o comandante fabrica, e
// o segundo é cópia funcional do `Grizzly Bears`, que já está na lista — num
// deck singleton, redundância pura só ocupa espaço.

/// Verde-branco, indo largo: Emmara vira todo turno e paga uma ficha por vez,
/// e os anthems transformam essas fichas em relógio de verdade.
fn conclave_of_emmara() -> DeckList {
    deck(
        "Conclave of Emmara",
        "Commander verde-branco: fichas, anthems e criaturas que trazem valor ao entrar — a vantagem vem de largura, não de uma carta grande.",
        &[Color::White, Color::Green],
        Format::Commander,
        Some("Emmara, Soul of the Accord"),
        &[
            // Terrenos (37)
            ("Plains", 17),
            ("Forest", 18),
            ("Blossoming Sands", 1),
            ("Radiant Fountain", 1),
            // Branco (27)
            ("Elite Vanguard", 1),
            ("Savannah Lions", 1),
            ("Suntail Hawk", 1),
            ("Healer's Hawk", 1),
            ("Soul Warden", 1),
            ("Gideon's Lawkeeper", 1),
            ("Youthful Knight", 1),
            ("Leonin Skyhunter", 1),
            ("Fencing Ace", 1),
            ("Veteran Armorer", 1),
            ("Angelic Wall", 1),
            ("Wall of Omens", 1),
            ("Ajani's Pridemate", 1),
            ("Squadron Hawk", 1),
            ("Attended Knight", 1),
            ("Serra Angel", 1),
            ("Angel of Mercy", 1),
            ("Captain of the Watch", 1),
            ("Swords to Plowshares", 1),
            ("Disenchant", 1),
            ("Divine Verdict", 1),
            ("Sunlance", 1),
            ("Raise the Alarm", 1),
            ("Mighty Leap", 1),
            ("Oblivion Ring", 1),
            ("Glorious Anthem", 1),
            ("Honor of the Pure", 1),
            // Verde (28)
            ("Llanowar Elves", 1),
            ("Elvish Mystic", 1),
            ("Arbor Elf", 1),
            ("Grizzly Bears", 1),
            ("Elvish Visionary", 1),
            ("Sylvan Ranger", 1),
            ("Ambush Viper", 1),
            ("Thornweald Archer", 1),
            ("Wall of Blossoms", 1),
            ("Garruk's Companion", 1),
            ("Kalonian Tusker", 1),
            ("Centaur Courser", 1),
            ("Giant Spider", 1),
            ("Leatherback Baloth", 1),
            ("Acidic Slime", 1),
            ("Craw Wurm", 1),
            ("Vastwood Gorger", 1),
            ("Thragtusk", 1),
            ("Giant Growth", 1),
            ("Titanic Growth", 1),
            ("Aggressive Urge", 1),
            ("Prey Upon", 1),
            ("Rabid Bite", 1),
            ("Hunt the Weak", 1),
            ("Plummet", 1),
            ("Naturalize", 1),
            ("Lay of the Land", 1),
            ("Overrun", 1),
            // Artefatos (7)
            ("Sol Ring", 1),
            ("Mind Stone", 1),
            ("Manalith", 1),
            ("Darksteel Ingot", 1),
            ("Prophetic Prism", 1),
            ("Palladium Myr", 1),
            ("Skyscanner", 1),
        ],
    )
}

/// Azul-vermelho: Adeliz cresce a cada mágica lançada, então a lista é feita de
/// mágicas baratas — o dano direto limpa o bloqueador e o contra-magia segura
/// a resposta.
fn storm_of_adeliz() -> DeckList {
    deck(
        "Storm of Adeliz",
        "Commander azul-vermelho: enxame de mágicas baratas que engorda o comandante e passa dano pelo ar.",
        &[Color::Blue, Color::Red],
        Format::Commander,
        Some("Adeliz, the Cinder Wind"),
        &[
            // Terrenos (37)
            ("Island", 18),
            ("Mountain", 17),
            ("Swiftwater Cliffs", 1),
            ("Radiant Fountain", 1),
            // Azul (27)
            ("Merfolk of the Pearl Trident", 1),
            ("Merfolk Looter", 1),
            ("Wind Drake", 1),
            ("Cloudkin Seer", 1),
            ("Snapping Drake", 1),
            ("Air Elemental", 1),
            ("Man-o'-War", 1),
            ("Aether Adept", 1),
            ("Aven Fisher", 1),
            ("Frost Lynx", 1),
            ("Thieving Magpie", 1),
            ("Sower of Temptation", 1),
            ("Counterspell", 1),
            ("Cancel", 1),
            ("Essence Scatter", 1),
            ("Negate", 1),
            ("Mana Leak", 1),
            ("Dismiss", 1),
            ("Unsummon", 1),
            ("Boomerang", 1),
            ("Griptide", 1),
            ("Opt", 1),
            ("Divination", 1),
            ("Jace's Ingenuity", 1),
            ("Tome Scour", 1),
            ("Sleep", 1),
            ("Talrand's Invocation", 1),
            // Vermelho (28)
            ("Raging Goblin", 1),
            ("Mogg Fanatic", 1),
            ("Goblin Piker", 1),
            ("Ember Hauler", 1),
            ("Goblin Chieftain", 1),
            ("Prodigal Pyromancer", 1),
            ("Manic Vandal", 1),
            ("Furnace Whelp", 1),
            ("Fire Elemental", 1),
            ("Shivan Dragon", 1),
            ("Lightning Bolt", 1),
            ("Shock", 1),
            ("Searing Spear", 1),
            ("Incinerate", 1),
            ("Volcanic Hammer", 1),
            ("Flame Slash", 1),
            ("Seismic Strike", 1),
            ("Chandra's Outrage", 1),
            ("Lava Axe", 1),
            ("Pyroclasm", 1),
            ("Shatter", 1),
            ("Smelt", 1),
            ("Stone Rain", 1),
            ("Act of Treason", 1),
            ("Titan's Strength", 1),
            ("Trumpet Blast", 1),
            ("Dragon Fodder", 1),
            ("Krenko's Command", 1),
            // Artefatos (7)
            ("Sol Ring", 1),
            ("Mind Stone", 1),
            ("Manalith", 1),
            ("Darksteel Ingot", 1),
            ("Prophetic Prism", 1),
            ("Palladium Myr", 1),
            ("Skyscanner", 1),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_database;
    use mtg_core::types::{CardType, Supertype};
    use mtg_format::identity::color_identity;
    use mtg_format::validate;

    /// Lista de decks com garantia de não estar vazia. Sem isto, um `all_decks()`
    /// que devolvesse `vec![]` faria todo teste de laço abaixo passar sem
    /// afirmar coisa nenhuma.
    fn todos() -> Vec<DeckList> {
        let d = all_decks();
        assert!(d.len() >= 6, "all_decks() devolveu só {} listas", d.len());
        d
    }

    /// Carta do catálogo pelo nome. Nome que não resolve é erro de deck, não
    /// motivo para pular a verificação.
    fn card<'a>(db: &'a CardDatabase, name: &str) -> &'a mtg_core::card::CardDef {
        db.by_name(name)
            .unwrap_or_else(|| panic!("carta '{name}' citada num deck não existe no catálogo"))
    }

    #[test]
    fn deck_construido_tem_sessenta_cartas_e_o_de_commander_tem_cem() {
        for d in decks() {
            assert_eq!(d.size(), 60, "deck construído {} tem {} cartas", d.name, d.size());
            assert!(d.commander.is_none(), "deck construído {} declara comandante", d.name);
        }
        for d in commander_decks() {
            // CR 903.5a — o comandante conta dentro das 100.
            assert_eq!(d.size(), 100, "deck de commander {} tem {} cartas", d.name, d.size());
            assert_eq!(d.library_size(), 99, "biblioteca de {} não tem 99", d.name);
        }
    }

    #[test]
    fn todo_deck_expande_para_ids_validos() {
        let db = build_database().expect("catálogo carrega");
        for d in todos() {
            let Some(ids) = d.expand(&db) else {
                panic!("deck {} cita carta inexistente", d.name);
            };
            assert_eq!(ids.len(), d.library_size() as usize, "deck {}", d.name);
            for id in ids {
                assert!(db.get(id).is_some(), "id inválido no deck {}", d.name);
            }
        }
    }

    /// A checagem de verdade: cada lista contra as regras do formato que ela
    /// própria declara. Cobre tamanho, cópias, singleton, comandante lendário
    /// e identidade de cor de uma vez só.
    #[test]
    fn todo_deck_passa_na_validacao_do_proprio_formato() {
        let db = build_database().expect("catálogo carrega");
        for d in todos() {
            if let Err(violations) = validate(&d, &db, d.format) {
                panic!("deck {} não é legal em {}: {violations:?}", d.name, d.format);
            }
        }
    }

    #[test]
    fn nenhum_deck_passa_do_limite_de_copias_fora_de_terreno_basico() {
        let db = build_database().expect("catálogo carrega");
        let mut verificadas = 0usize;
        for d in todos() {
            let Some(max) = d.format.max_copies() else { continue };
            for (name, count) in &d.cards {
                if card(&db, name).type_line.has_supertype(Supertype::Basic) {
                    continue;
                }
                verificadas += 1;
                // CR 100.2a — limite de cópias; CR 903.5b — singleton.
                assert!(*count <= max, "deck {}: {name} aparece {count} vezes", d.name);
            }
        }
        assert!(verificadas > 0, "nenhuma carta foi verificada: o teste não afirmou nada");
    }

    #[test]
    fn todo_deck_tem_a_base_de_mana_do_formato() {
        let db = build_database().expect("catálogo carrega");
        for d in todos() {
            let terrenos: u32 = d
                .cards
                .iter()
                .filter(|(name, _)| card(&db, name).type_line.is_land())
                .map(|(_, n)| u32::from(*n))
                .sum();
            // 60 cartas pedem 22–25 terrenos; as 100 de Commander pedem 35–40.
            // Fora dessas faixas o deck trava ou afoga.
            let faixa = if d.format == Format::Commander { 35..=40 } else { 22..=25 };
            assert!(
                faixa.contains(&terrenos),
                "deck {} tem {terrenos} terrenos, fora de {faixa:?}",
                d.name
            );
        }
    }

    #[test]
    fn todo_deck_pode_pagar_as_proprias_cores() {
        // Um deck que declara cor sem fonte dela trava na primeira mão.
        let db = build_database().expect("catálogo carrega");
        let mut verificadas = 0usize;
        for d in todos() {
            let nomes: Vec<String> =
                d.cards.iter().map(|(n, _)| n.clone()).chain(d.commander.clone()).collect();
            for name in nomes {
                for cor in card(&db, &name).colors().iter() {
                    verificadas += 1;
                    assert!(
                        d.colors.contains(&cor),
                        "deck {} joga {name}, que é {cor:?}, mas não declara essa cor",
                        d.name
                    );
                }
            }
        }
        assert!(
            verificadas > 0,
            "nenhuma carta colorida foi verificada: o teste não afirmou nada"
        );
    }

    #[test]
    fn todo_deck_tem_criatura_suficiente_para_jogar_o_jogo() {
        let db = build_database().expect("catálogo carrega");
        for d in todos() {
            let criaturas: u32 = d
                .cards
                .iter()
                .filter(|(name, _)| card(&db, name).type_line.has_type(CardType::Creature))
                .map(|(_, n)| u32::from(*n))
                .sum();
            assert!(criaturas >= 8, "deck {} só tem {criaturas} criaturas", d.name);
        }
    }

    /// CR 903.3 e CR 903.5c: o comandante existe, é criatura lendária, e a
    /// identidade dele é exatamente a que a lista declara.
    #[test]
    fn deck_de_commander_tem_comandante_lendario_e_identidade_coerente() {
        let db = build_database().expect("catálogo carrega");
        let listas = commander_decks();
        assert_eq!(listas.len(), 2, "esperados dois decks de Commander");
        for d in listas {
            let Some(nome) = d.commander.clone() else {
                panic!("deck de commander {} não declara comandante", d.name);
            };
            let c = card(&db, &nome);
            assert!(c.type_line.has_supertype(Supertype::Legendary), "{nome} não é lendária");
            assert!(c.type_line.has_type(CardType::Creature), "{nome} não é criatura");

            let identidade = color_identity(c);
            assert_eq!(
                identidade.count() as usize,
                d.colors.len(),
                "deck {} declara {:?}, comandante tem identidade {identidade:?}",
                d.name,
                d.colors
            );
            for cor in &d.colors {
                assert!(
                    identidade.contains(*cor),
                    "deck {} declara {cor:?}, fora da identidade de {nome}",
                    d.name
                );
            }
            let Some(id) = d.commander_id(&db) else {
                panic!("commander_id de {} não resolveu", d.name);
            };
            assert_eq!(db.get(id).map(|c| c.name.as_str()), Some(nome.as_str()));
        }
    }

    #[test]
    fn deck_by_name_ignora_caixa_e_rejeita_desconhecido() {
        let db = build_database().expect("catálogo carrega");
        assert_eq!(deck_by_name(&db, "goblin onslaught").map(|v| v.len()), Some(60));
        // Biblioteca de Commander: 99, porque o comandante começa na zona de
        // comando (CR 903.6).
        assert_eq!(deck_by_name(&db, "conclave of emmara").map(|v| v.len()), Some(99));
        assert!(deck_by_name(&db, "não existe").is_none());
    }

    /// `decks()` é o que o servidor consome esperando 60 cartas. Se um dia
    /// alguém acrescentar um deck de Commander ali, este teste cai antes de a
    /// partida começar.
    #[test]
    fn decks_nao_contem_lista_de_commander() {
        for d in decks() {
            assert_ne!(d.format, Format::Commander, "deck {} não devia estar em decks()", d.name);
        }
        for d in commander_decks() {
            assert_eq!(d.format, Format::Commander, "deck {} devia ser Commander", d.name);
        }
    }
}
