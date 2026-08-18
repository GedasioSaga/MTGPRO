//! A lista de deck: o que o jogador declara antes da partida.
//!
//! A lista é declarada por *nome* de carta, não por id: id é posição no vetor
//! do catálogo e muda quando alguém insere uma carta nova no meio de um `.lua`.
use mtg_core::card::CardDatabase;
use mtg_core::ids::CardDefId;
use mtg_core::mana::Color;
use serde::{Deserialize, Serialize};

use crate::format::Format;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckList {
    pub name: String,
    pub description: String,
    pub colors: Vec<Color>,
    /// Formato para o qual a lista foi montada.
    pub format: Format,
    /// CR 903.3 — comandante, fora do deck principal e na zona de comando.
    /// `None` em todo formato que não seja Commander.
    #[serde(default)]
    pub commander: Option<String>,
    /// `(nome da carta, quantidade)`, sem o comandante.
    pub cards: Vec<(String, u8)>,
}

impl DeckList {
    /// Total de cartas do deck, comandante incluído. CR 903.5a conta o
    /// comandante dentro das 100, então contar só a biblioteca daria 99 e
    /// faria todo deck de Commander parecer pequeno.
    pub fn size(&self) -> u32 {
        self.library_size() + u32::from(self.commander.is_some())
    }

    /// Cartas que começam na biblioteca — o que `expand` devolve.
    pub fn library_size(&self) -> u32 {
        self.cards.iter().map(|(_, n)| u32::from(*n)).sum()
    }

    /// Expande a biblioteca para ids, repetindo cada carta pela quantidade
    /// declarada. O comandante **não** entra: ele começa na zona de comando.
    ///
    /// `None` se alguma carta não existir no catálogo: um deck incompleto não
    /// é um deck, e devolver uma versão menor esconderia o erro de digitação.
    pub fn expand(&self, db: &CardDatabase) -> Option<Vec<CardDefId>> {
        let mut out = Vec::with_capacity(self.library_size() as usize);
        for (name, count) in &self.cards {
            let id = db.id_by_name(name)?;
            for _ in 0..*count {
                out.push(id);
            }
        }
        Some(out)
    }

    /// Id do comandante. `None` quando a lista não tem comandante; `Some(None)`
    /// não existe de propósito — comandante citado e inexistente é erro que
    /// `validate` reporta, não algo que este método deva engolir.
    pub fn commander_id(&self, db: &CardDatabase) -> Option<CardDefId> {
        db.id_by_name(self.commander.as_deref()?)
    }

    pub fn is_commander(&self) -> bool {
        self.format.requires_commander()
    }
}
