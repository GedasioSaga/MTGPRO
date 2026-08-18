//! De onde vem a resposta para "esta carta é legal neste formato?".
//!
//! O motor não sabe, e não deveria saber: banimento e rotação são dado
//! externo, que muda sem que uma linha de Rust mude. A fonte real é o campo
//! `legalities` do Scryfall (`legalities.standard == "legal"`), importado para
//! o banco. Aqui só existe o *contrato* e dois provedores pequenos: um em
//! memória, para teste, e um que lê o próprio catálogo em Lua.
use std::collections::{BTreeMap, BTreeSet};

use mtg_core::card::CardDatabase;
use mtg_core::types::Rarity;

use crate::format::Format;

/// Consulta de legalidade e raridade por nome de carta.
///
/// O nome é a chave porque é assim que uma lista de deck é escrita — id de
/// catálogo muda quando alguém insere carta no meio de um `.lua`.
pub trait LegalitySource {
    /// `true` se a carta pode ser jogada no formato.
    fn legal_in(&self, card: &str, format: Format) -> bool;
    /// Raridade impressa, ou `None` se a fonte não conhece a carta.
    fn rarity(&self, card: &str) -> Option<Rarity>;
}

/// O que a fonte sabe sobre uma carta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardLegality {
    /// Formatos em que a carta é legal. `BTreeSet` e não `HashSet`: qualquer
    /// varredura sobre isto tem de sair sempre na mesma ordem.
    pub formats: BTreeSet<Format>,
    pub rarity: Rarity,
}

/// Provedor em memória — o que os testes usam e o que um importador pode
/// preencher a partir do JSON do Scryfall.
///
/// Carta desconhecida é **ilegal**, não legal: um nome digitado errado tem de
/// aparecer como problema, e não passar por engano.
#[derive(Debug, Clone, Default)]
pub struct InMemoryLegality {
    entries: BTreeMap<String, CardLegality>,
}

impl InMemoryLegality {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declara uma carta. Nome é comparado sem diferenciar maiúsculas.
    pub fn insert(&mut self, card: &str, rarity: Rarity, formats: &[Format]) -> &mut Self {
        self.entries.insert(
            card.to_ascii_lowercase(),
            CardLegality { formats: formats.iter().copied().collect(), rarity },
        );
        self
    }

    /// Declara uma carta legal em todos os formatos sancionados.
    pub fn insert_everywhere(&mut self, card: &str, rarity: Rarity) -> &mut Self {
        let all: Vec<Format> = Format::ALL.into_iter().filter(|f| f.checks_legality()).collect();
        self.insert(card, rarity, &all)
    }

    pub fn get(&self, card: &str) -> Option<&CardLegality> {
        self.entries.get(&card.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl LegalitySource for InMemoryLegality {
    fn legal_in(&self, card: &str, format: Format) -> bool {
        match self.get(card) {
            Some(e) => e.formats.contains(&format),
            None => false,
        }
    }
    fn rarity(&self, card: &str) -> Option<Rarity> {
        self.get(card).map(|e| e.rarity)
    }
}

/// Provedor sobre o catálogo curado em Lua.
///
/// **Limitação assumida:** os scripts em `cards/*.lua` guardam raridade, mas
/// não guardam legalidade — esse dado só existe no Scryfall. Então este
/// provedor responde "legal" para toda carta que exista no catálogo, e é
/// honesto sobre isso: serve para validar tamanho, cópias, singleton,
/// identidade de cor e raridade. Para valer banimento e rotação de verdade, a
/// integração troca isto por um provedor alimentado pelo banco.
pub struct CatalogLegality<'a> {
    db: &'a CardDatabase,
}

impl<'a> CatalogLegality<'a> {
    pub fn new(db: &'a CardDatabase) -> Self {
        Self { db }
    }
}

impl LegalitySource for CatalogLegality<'_> {
    fn legal_in(&self, card: &str, _format: Format) -> bool {
        self.db.by_name(card).is_some()
    }
    fn rarity(&self, card: &str) -> Option<Rarity> {
        self.db.by_name(card).map(|c| c.rarity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carta_nao_declarada_e_ilegal_em_todo_formato() {
        let mut src = InMemoryLegality::new();
        src.insert("Lightning Bolt", Rarity::Common, &[Format::Modern, Format::Commander]);

        assert!(src.legal_in("lightning bolt", Format::Modern));
        assert!(src.legal_in("Lightning Bolt", Format::Commander));
        assert!(!src.legal_in("Lightning Bolt", Format::Standard));
        assert_eq!(src.rarity("LIGHTNING BOLT"), Some(Rarity::Common));

        for f in Format::ALL {
            assert!(!src.legal_in("Black Lotus", f), "carta desconhecida passou em {f}");
        }
        assert_eq!(src.rarity("Black Lotus"), None);
    }
}
