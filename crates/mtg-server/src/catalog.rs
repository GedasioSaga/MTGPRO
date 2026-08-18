//! Carga do catálogo de cartas e dos decks jogáveis.
//!
//! A fonte é `mtg-cards`: os scripts em `cards/*.lua` viram `CardDatabase`, e
//! as listas de `mtg_cards::decks()` viram `DeckInfo`. Este módulo não define
//! carta nenhuma — se definisse, o servidor teria uma segunda verdade sobre o
//! que existe no jogo, e as duas divergiriam na primeira carta nova.
//!
//! `mtg-cards` resolve sozinho de onde ler (variável `MTG_CARDS_DIR`, o
//! diretório `cards/` do repositório, ou a cópia embutida no binário), então o
//! servidor roda tanto do checkout quanto de um binário solto.
use mtg_core::card::CardDatabase;
use mtg_core::ids::CardDefId;

/// Deck jogável: id estável (usado pelo cliente em `start`), metadados para
/// a UI e a lista de cartas na proporção em que entram na biblioteca.
pub struct DeckInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub colors: Vec<String>,
    pub cards: Vec<CardDefId>,
}

/// Carrega catálogo e decks. Falha alto: subir o servidor com catálogo vazio
/// só adiaria o erro para o primeiro `start` do cliente, longe da causa.
pub fn load() -> Result<(CardDatabase, Vec<DeckInfo>), mtg_cards::LoadError> {
    let db = mtg_cards::build_database()?;

    let mut decks = Vec::new();
    for list in mtg_cards::decks() {
        // `expand` devolve `None` quando a lista cita carta fora do catálogo.
        // Pular em silêncio esconderia erro de digitação na lista, então o
        // deck inválido derruba a carga junto com o resto.
        let Some(cards) = list.expand(&db) else {
            return Err(mtg_cards::LoadError::Empty);
        };
        decks.push(DeckInfo {
            id: slug(&list.name),
            name: list.name.clone(),
            description: list.description.clone(),
            colors: list.colors.iter().map(|c| c.letter().to_string()).collect(),
            cards,
        });
    }

    Ok((db, decks))
}

/// Id de URL a partir do nome: minúsculas, sem acento comum ao português e com
/// hífen no lugar de qualquer coisa que não seja letra ou dígito.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = true; // evita hífen inicial
    for ch in name.chars() {
        let mapped = match ch {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'ê' | 'è' | 'ë' | 'É' | 'Ê' | 'È' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            other => other,
        };
        if mapped.is_ascii_alphanumeric() {
            out.push(mapped.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_normaliza_nome() {
        assert_eq!(slug("Goblin Onslaught"), "goblin-onslaught");
        assert_eq!(slug("Chamado da Floresta"), "chamado-da-floresta");
        assert_eq!(slug("Açaí   Verde!"), "acai-verde");
        assert_eq!(slug("  x  "), "x");
    }

    #[test]
    fn catalogo_real_carrega_com_decks_completos() {
        let (db, decks) = match load() {
            Ok(v) => v,
            Err(err) => panic!("catálogo não carregou: {err}"),
        };
        assert!(!db.cards.is_empty(), "catálogo sem cartas");
        assert!(!decks.is_empty(), "nenhum deck jogável");
        for d in &decks {
            assert!(!d.id.is_empty(), "deck '{}' sem id", d.name);
            assert_eq!(d.cards.len(), 60, "deck '{}' não tem 60 cartas", d.name);
            for id in &d.cards {
                assert!(db.get(*id).is_some(), "deck '{}' cita id inexistente", d.name);
            }
        }
        // Ids têm de ser únicos: o cliente escolhe deck por id.
        let mut ids: Vec<&str> = decks.iter().map(|d| d.id.as_str()).collect();
        ids.sort_unstable();
        let total = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), total, "ids de deck duplicados");
    }
}
