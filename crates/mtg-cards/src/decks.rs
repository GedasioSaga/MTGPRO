//! Decks pré-montados.
//!
//! Quatro identidades distintas, 60 cartas cada, pensadas para se baterem: o
//! agressivo pune a curva alta, o controle pune o agressivo, o valor pune o
//! controle e a rampa pune quem não fecha o jogo cedo. Nenhum deck usa carta
//! que o outro não possa responder.
//!
//! A lista é declarada por *nome* de carta, não por id: id é posição no vetor
//! do catálogo e muda quando alguém insere uma carta nova no meio de um `.lua`.

use mtg_core::card::CardDatabase;
use mtg_core::ids::CardDefId;
use mtg_core::mana::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckList {
    pub name: String,
    pub description: String,
    pub colors: Vec<Color>,
    /// `(nome da carta, quantidade)`.
    pub cards: Vec<(String, u8)>,
}

impl DeckList {
    /// Total de cartas. Deve ser 60 — há teste garantindo.
    pub fn size(&self) -> u32 {
        self.cards.iter().map(|(_, n)| *n as u32).sum()
    }

    /// Expande para ids, repetindo cada carta pela quantidade declarada.
    /// `None` se alguma carta não existir no catálogo: um deck incompleto não
    /// é um deck, e devolver uma versão menor esconderia o erro de digitação.
    pub fn expand(&self, db: &CardDatabase) -> Option<Vec<CardDefId>> {
        let mut out = Vec::with_capacity(self.size() as usize);
        for (name, count) in &self.cards {
            let id = db.id_by_name(name)?;
            for _ in 0..*count {
                out.push(id);
            }
        }
        Some(out)
    }
}

fn deck(name: &str, description: &str, colors: &[Color], cards: &[(&str, u8)]) -> DeckList {
    DeckList {
        name: name.to_string(),
        description: description.to_string(),
        colors: colors.to_vec(),
        cards: cards.iter().map(|(n, c)| (n.to_string(), *c)).collect(),
    }
}

/// Os decks disponíveis, na ordem em que a interface deve listá-los.
pub fn decks() -> Vec<DeckList> {
    vec![goblin_onslaught(), azorius_control(), selesnya_valor(), gruul_stampede()]
}

/// Procura um deck pelo nome (sem diferenciar maiúsculas) e já expande em ids.
pub fn deck_by_name(db: &CardDatabase, name: &str) -> Option<Vec<CardDefId>> {
    decks()
        .into_iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))
        .and_then(|d| d.expand(db))
}

/// Mono-vermelho: curva termina no 3, 22 terrenos porque quase nada custa mais
/// que isso, e 11 mágicas de dano que servem tanto para limpar bloqueador
/// quanto para os últimos pontos de vida.
fn goblin_onslaught() -> DeckList {
    deck(
        "Goblin Onslaught",
        "Agressivo mono-vermelho: criaturas baratas, pressa e dano direto para fechar antes do turno seis.",
        &[Color::Red],
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
fn azorius_control() -> DeckList {
    deck(
        "Azorius Control",
        "Controle azul-branco: contra-magia para o que importa, remoção incondicional para o resto e compra para não ficar sem gás.",
        &[Color::White, Color::Blue],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_database;
    use mtg_core::types::CardType;

    /// Lista de decks com garantia de não estar vazia. Sem isto, um `decks()`
    /// que devolvesse `vec![]` faria todo teste de laço abaixo passar sem
    /// afirmar coisa nenhuma.
    fn all_decks() -> Vec<DeckList> {
        let d = decks();
        assert!(!d.is_empty(), "decks() não devolveu nenhum deck jogável");
        d
    }

    /// Carta do catálogo pelo nome. Nome que não resolve é erro de deck, não
    /// motivo para pular a verificação.
    fn card<'a>(db: &'a CardDatabase, name: &str) -> &'a mtg_core::card::CardDef {
        db.by_name(name)
            .unwrap_or_else(|| panic!("carta '{name}' citada num deck não existe no catálogo"))
    }

    #[test]
    fn todo_deck_tem_exatamente_sessenta_cartas() {
        for d in all_decks() {
            assert_eq!(d.size(), 60, "deck {} tem {} cartas", d.name, d.size());
        }
    }

    #[test]
    fn todo_deck_expande_para_sessenta_ids_validos() {
        let db = build_database().expect("catálogo carrega");
        for d in all_decks() {
            let ids = d.expand(&db).unwrap_or_else(|| panic!("deck {} cita carta inexistente", d.name));
            assert_eq!(ids.len(), 60, "deck {}", d.name);
            for id in ids {
                assert!(db.get(id).is_some(), "id inválido no deck {}", d.name);
            }
        }
    }

    #[test]
    fn nenhum_deck_passa_de_quatro_copias_fora_de_terreno_basico() {
        let db = build_database().expect("catálogo carrega");
        for d in all_decks() {
            for (name, count) in &d.cards {
                let basico =
                    card(&db, name).type_line.has_supertype(mtg_core::types::Supertype::Basic);
                if !basico {
                    // CR 100.2a — limite de quatro cópias por baralho.
                    assert!(*count <= 4, "deck {}: {name} aparece {count} vezes", d.name);
                }
            }
        }
    }

    #[test]
    fn todo_deck_tem_entre_22_e_25_terrenos() {
        let db = build_database().expect("catálogo carrega");
        for d in all_decks() {
            let terrenos: u32 = d
                .cards
                .iter()
                .filter(|(name, _)| card(&db, name).type_line.is_land())
                .map(|(_, n)| *n as u32)
                .sum();
            assert!(
                (22..=25).contains(&terrenos),
                "deck {} tem {terrenos} terrenos",
                d.name
            );
        }
    }

    #[test]
    fn todo_deck_pode_pagar_as_proprias_cores() {
        // Um deck que declara cor sem fonte dela trava na primeira mão.
        let db = build_database().expect("catálogo carrega");
        let mut verificadas = 0usize;
        for d in all_decks() {
            for (name, _) in &d.cards {
                for cor in card(&db, name).colors().iter() {
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
        for d in all_decks() {
            let criaturas: u32 = d
                .cards
                .iter()
                .filter(|(name, _)| card(&db, name).type_line.has_type(CardType::Creature))
                .map(|(_, n)| *n as u32)
                .sum();
            assert!(criaturas >= 8, "deck {} só tem {criaturas} criaturas", d.name);
        }
    }

    #[test]
    fn deck_by_name_ignora_caixa_e_rejeita_desconhecido() {
        let db = build_database().expect("catálogo carrega");
        assert_eq!(deck_by_name(&db, "goblin onslaught").map(|v| v.len()), Some(60));
        assert!(deck_by_name(&db, "não existe").is_none());
    }
}
