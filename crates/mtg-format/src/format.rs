//! Os formatos jogáveis e as regras de construção que cada um impõe.
//!
//! Só o que é *estrutural* mora aqui: tamanho, cópias, singleton, comandante.
//! Se uma carta é ou não legal num formato é dado externo (o campo
//! `legalities` do Scryfall), e vem por `LegalitySource` — este módulo não
//! tenta adivinhar banimento nenhum.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Format {
    Standard,
    Modern,
    Pauper,
    Commander,
    Casual,
}

impl Format {
    /// Ordem fixa, usada por qualquer varredura que precise ser determinística.
    pub const ALL: [Format; 5] = [
        Format::Standard,
        Format::Modern,
        Format::Pauper,
        Format::Commander,
        Format::Casual,
    ];

    /// Chave correspondente no objeto `legalities` do Scryfall.
    ///
    /// `Casual` não tem chave porque não é um formato sancionado: nada é
    /// ilegal nele, então não há o que consultar.
    pub fn scryfall_key(self) -> Option<&'static str> {
        match self {
            Format::Standard => Some("standard"),
            Format::Modern => Some("modern"),
            Format::Pauper => Some("pauper"),
            Format::Commander => Some("commander"),
            Format::Casual => None,
        }
    }

    /// Nome curto e estável, para configuração e API.
    pub fn slug(self) -> &'static str {
        match self {
            Format::Standard => "standard",
            Format::Modern => "modern",
            Format::Pauper => "pauper",
            Format::Commander => "commander",
            Format::Casual => "casual",
        }
    }

    pub fn from_slug(s: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.slug().eq_ignore_ascii_case(s))
    }

    /// Mínimo de cartas no deck principal. CR 100.2a para os construídos,
    /// CR 903.5a para Commander.
    pub fn min_deck_size(self) -> u32 {
        match self {
            Format::Standard | Format::Modern | Format::Pauper => 60,
            Format::Commander => 100,
            Format::Casual => 40,
        }
    }

    /// Formatos de tamanho fechado. CR 903.5a — Commander é exatamente 100,
    /// comandante incluído; não existe "deck grande" legal.
    pub fn exact_deck_size(self) -> Option<u32> {
        match self {
            Format::Commander => Some(100),
            _ => None,
        }
    }

    /// Máximo de cópias de uma carta que não seja terreno básico.
    /// CR 100.2a para o limite de quatro, CR 903.5b para o singleton.
    pub fn max_copies(self) -> Option<u8> {
        match self {
            Format::Standard | Format::Modern | Format::Pauper => Some(4),
            Format::Commander => Some(1),
            Format::Casual => None,
        }
    }

    /// CR 903.3 — o deck de Commander é definido por um comandante.
    pub fn requires_commander(self) -> bool {
        matches!(self, Format::Commander)
    }

    /// CR 903.5c — nenhuma carta pode ter cor fora da identidade do comandante.
    pub fn checks_color_identity(self) -> bool {
        matches!(self, Format::Commander)
    }

    /// Se o formato tem lista de legalidade. `Casual` não tem.
    pub fn checks_legality(self) -> bool {
        self.scryfall_key().is_some()
    }

    /// Pauper aceita só cartas impressas em comum.
    pub fn commons_only(self) -> bool {
        matches!(self, Format::Pauper)
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Format::Standard => "Standard",
            Format::Modern => "Modern",
            Format::Pauper => "Pauper",
            Format::Commander => "Commander",
            Format::Casual => "Casual",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_ida_e_volta_para_todo_formato() {
        for f in Format::ALL {
            let voltou = Format::from_slug(f.slug());
            let Some(voltou) = voltou else {
                panic!("slug '{}' de {f} não volta a ser formato", f.slug());
            };
            assert_eq!(voltou, f);
        }
        assert_eq!(Format::from_slug("COMMANDER"), Some(Format::Commander));
        assert_eq!(Format::from_slug("vintage"), None);
    }

    #[test]
    fn casual_e_o_unico_sem_lista_de_legalidade() {
        for f in Format::ALL {
            assert_eq!(
                f.checks_legality(),
                f != Format::Casual,
                "{f} discorda de si mesmo sobre ter lista de legalidade"
            );
        }
    }
}
