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
use std::path::PathBuf;

use mtg_cards::{DeckList, Format};
use mtg_core::card::CardDatabase;
use mtg_core::ids::CardDefId;
use mtg_db::CardStore;
use mtg_format::ScryfallLegality;


/// Deck jogável: id estável (usado pelo cliente em `start`), metadados para
/// a UI e a lista de cartas na proporção em que entram na biblioteca.
pub struct DeckInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub colors: Vec<String>,
    /// Cartas que começam na biblioteca. Num deck de Commander são 99: o
    /// comandante começa na zona de comando (CR 903.6) e não entra aqui.
    pub cards: Vec<CardDefId>,
    /// Formato para o qual a lista foi montada. Não é o formato da partida —
    /// o cliente pode pedir a mesma lista em outro, e aí ela é revalidada.
    pub format: Format,
    /// CR 903.3 — nome do comandante declarado pela lista, se houver.
    pub commander: Option<String>,
    pub commander_id: Option<CardDefId>,
    /// A lista declarada, guardada inteira porque validar legalidade exige os
    /// NOMES das cartas (é assim que `mtg-format` consulta o Scryfall), e a
    /// expansão em `cards` já perdeu essa informação.
    pub list: DeckList,
}

/// Carrega catálogo e decks. Falha alto: subir o servidor com catálogo vazio
/// só adiaria o erro para o primeiro `start` do cliente, longe da causa.
pub fn load() -> Result<(CardDatabase, Vec<DeckInfo>), mtg_cards::LoadError> {
    let db = mtg_cards::build_database()?;

    let mut decks = Vec::new();
    for list in mtg_cards::all_decks() {
        // `expand` devolve `None` quando a lista cita carta fora do catálogo.
        // Pular em silêncio esconderia erro de digitação na lista, então o
        // deck inválido derruba a carga junto com o resto.
        let Some(cards) = list.expand(&db) else {
            return Err(mtg_cards::LoadError::Empty);
        };
        // Comandante citado e inexistente no catálogo é erro de lista, e some
        // silenciosamente se virar `None` — a carga inteira cai junto.
        let commander_id = match &list.commander {
            Some(name) => match db.id_by_name(name) {
                Some(id) => Some(id),
                None => return Err(mtg_cards::LoadError::Empty),
            },
            None => None,
        };
        decks.push(DeckInfo {
            id: slug(&list.name),
            name: list.name.clone(),
            description: list.description.clone(),
            colors: list.colors.iter().map(|c| c.letter().to_string()).collect(),
            cards,
            format: list.format,
            commander: list.commander.clone(),
            commander_id,
            list,
        });
    }

    Ok((db, decks))
}

/// Onde o catálogo amplo (Scryfall) vive. A resolução mora em `mtg-db`, e não
/// aqui, porque o `mtg-import` precisa da MESMA resposta — quando cada binário
/// tinha o seu padrão, a importação gravava num arquivo que o servidor nunca
/// abria. `:memory:` roda sem tocar disco (teste, sandbox).
pub fn db_path() -> PathBuf {
    mtg_db::default_db_path()
}

/// Índice de legalidade real (banimento e rotação do Scryfall), lido do mesmo
/// banco do catálogo.
///
/// `None` quando o banco não existe, não abre ou não tem nenhuma linha com
/// `legalities`. Isso é deliberado e o chamador precisa tratar: o índice
/// responde "ilegal" para toda carta que não conhece, então um índice vazio
/// reprovaria todo deck em todo formato e ninguém conseguiria iniciar partida.
pub fn load_legality(lua: &CardDatabase) -> Option<ScryfallLegality> {
    let path = db_path();
    if path.as_os_str() == ":memory:" || !path.is_file() {
        tracing::warn!(path = %path.display(), "sem banco do Scryfall — legalidade cai na estrutural");
        return None;
    }
    match ScryfallLegality::from_sqlite(&path) {
        Ok(index) if index.is_empty() => {
            tracing::warn!("banco sem coluna `legalities` preenchida — legalidade cai na estrutural");
            None
        }
        Ok(index) => {
            // Cartas curadas em Lua entram como conhecidas: o bulk descarta a
            // impressão digital-only de algumas delas, e sem isto reprovariam.
            let index = index.with_curated(lua);
            tracing::info!(
                cards = index.len(),
                curated = index.curated_len(),
                "índice de legalidade carregado"
            );
            Some(index)
        }
        Err(err) => {
            tracing::warn!(%err, "falha ao ler legalidade — caindo na validação estrutural");
            None
        }
    }
}

/// Abre o SQLite do catálogo, migra e semeia as cartas curadas em Lua.
///
/// Nunca derruba o servidor: sem banco em disco cai para um banco em memória,
/// e sem nem isso devolve `None` e as rotas de catálogo respondem 503. Perder
/// a navegação por 35 mil cartas é ruim; não subir a partida por causa disso
/// é pior, porque `/ws/match` não depende deste banco em nada.
pub fn open_store(lua: &CardDatabase) -> Option<CardStore> {
    let path = db_path();
    let store = if path.as_os_str() == ":memory:" {
        CardStore::open_in_memory()
    } else {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(err) = std::fs::create_dir_all(parent) {
                    tracing::warn!(%err, path = %parent.display(), "não deu para criar o diretório do banco");
                }
            }
        }
        CardStore::open(&path)
    };

    let store = match store {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(%err, path = %path.display(), "banco do catálogo indisponível — caindo para memória");
            match CardStore::open_in_memory() {
                Ok(s) => s,
                Err(err) => {
                    tracing::error!(%err, "SQLite em memória também falhou — catálogo desligado");
                    return None;
                }
            }
        }
    };

    // Semear DEPOIS do import é a ordem real e é segura: `seed_from` só toca a
    // linha de mesmo nome, nunca apaga o resto. Ver o comentário em
    // `mtg_db::CardStore::seed_from` para a regra de colisão de nome.
    match store.seed_from(lua) {
        Ok(n) => tracing::info!(cards = n, "catálogo curado semeado no SQLite"),
        Err(err) => tracing::warn!(%err, "falha ao semear o catálogo curado"),
    }

    // O número REAL do banco, não o das cartas curadas. Enquanto o servidor só
    // logava as 174 de Lua, uma importação gravada no arquivo errado passava
    // despercebida — o log dizia o mesmo com 174 e com 32.798 cartas.
    match store.stats() {
        Ok(stats) => tracing::info!(
            total = stats.total,
            playable = stats.playable,
            unsupported = stats.unsupported,
            sets = stats.by_set.len(),
            path = %path.display(),
            "catálogo SQLite pronto"
        ),
        Err(err) => tracing::warn!(%err, "não deu para contar o catálogo no SQLite"),
    }
    Some(store)
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
        let mut commanders = 0usize;
        for d in &decks {
            assert!(!d.id.is_empty(), "deck '{}' sem id", d.name);
            // CR 903.5a — as 100 de Commander contam o comandante, que fica na
            // zona de comando; a biblioteca tem 99.
            let esperado = if d.format == Format::Commander { 99 } else { 60 };
            assert_eq!(d.cards.len(), esperado, "deck '{}' tem tamanho errado", d.name);
            if d.format == Format::Commander {
                commanders += 1;
                assert!(d.commander_id.is_some(), "deck '{}' sem comandante", d.name);
            }
            for id in &d.cards {
                assert!(db.get(*id).is_some(), "deck '{}' cita id inexistente", d.name);
            }
        }
        assert!(commanders >= 2, "catálogo sem decks de Commander suficientes para a mesa");
        // Ids têm de ser únicos: o cliente escolhe deck por id.
        let mut ids: Vec<&str> = decks.iter().map(|d| d.id.as_str()).collect();
        ids.sort_unstable();
        let total = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), total, "ids de deck duplicados");
    }
}
