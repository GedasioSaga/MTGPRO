//! O `LegalitySource` de verdade: banimento e rotação vindos do Scryfall.
//!
//! `CatalogLegality` (em `legality`) responde "legal" para tudo que exista no
//! catálogo em Lua, porque o `.lua` guarda raridade mas não guarda legalidade.
//! Isso serve para tamanho, cópias, singleton e identidade de cor, e **não**
//! serve para banimento. Aqui está a fonte que serve: o objeto `legalities` do
//! Scryfall, já importado para a tabela `cards` do catálogo em SQLite.
//!
//! Duas metades, separadas de propósito:
//!   - `ScryfallLegality` — o índice em memória e o parser do objeto
//!     `legalities`. Não sabe o que é um banco; é testável sem arquivo.
//!   - `from_sqlite` / `from_connection` — o único ponto que fala SQL, uma
//!     única `SELECT` de três colunas.
//!
//! Por que não passa por `mtg-db`: aquele crate expõe leitura de carta
//! (`CardDef`), e `CardDef` não carrega o campo `legalities`. Uma leitura em
//! massa de legalidade lá seria o caminho melhor; enquanto ela não existe, esta
//! `SELECT` é a ligação, e ela depende de três nomes de coluna só.
use mtg_core::card::CardDatabase;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mtg_core::types::Rarity;
use rusqlite::{Connection, OpenFlags};

use crate::format::Format;
use crate::legality::LegalitySource;

/// Colunas lidas do catálogo. Trocar isto é trocar o contrato com `mtg-db`.
const QUERY: &str = "SELECT name, rarity, legalities FROM cards WHERE legalities IS NOT NULL";

/// Valores do Scryfall que contam como "pode jogar".
///
/// `restricted` entra porque a carta é jogável — o limite de uma cópia é
/// contagem de cópias, não legalidade. `banned` e `not_legal` ficam de fora.
fn is_playable_status(status: &str) -> bool {
    matches!(status, "legal" | "restricted")
}

#[derive(Debug, thiserror::Error)]
pub enum LegalityDbError {
    #[error("catálogo em {path}: {source}")]
    Sqlite {
        path: String,
        #[source]
        source: rusqlite::Error,
    },
}

/// Índice de legalidade por nome de carta, carregado do banco do Scryfall.
///
/// Carta ausente é **ilegal** em todo formato — mesma escolha de
/// `InMemoryLegality`: nome digitado errado tem de aparecer como problema.
#[derive(Debug, Clone, Default)]
pub struct ScryfallLegality {
    /// Chave em minúsculas. `BTreeMap` e não `HashMap`: qualquer varredura
    /// sobre este índice precisa sair sempre na mesma ordem.
    entries: BTreeMap<String, Entry>,
    /// Cartas escritas à mão no catálogo Lua, em minúsculas.
    ///
    /// Existem porque o bulk `oracle_cards` traz UMA impressão por carta, e
    /// quando essa impressão é digital-only o importador a descarta — foi o que
    /// aconteceu com `Boomerang`, `Raging Goblin` e `Steel Wall`, que existem em
    /// papel há décadas. Sem esta ressalva, carta que nós mesmos escrevemos e
    /// testamos reprovaria por "ausente", derrubando decks inteiros.
    ///
    /// Carta curada conta como legal em qualquer formato. É deliberado: quem a
    /// escreveu foi este projeto, então ela é conhecida — diferente de um nome
    /// digitado errado, que continua reprovando.
    curated: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    rarity: Rarity,
    /// Um bit por formato, na ordem fixa de `Format::ALL`.
    legal: [bool; Format::ALL.len()],
}

impl ScryfallLegality {
    pub fn new() -> ScryfallLegality {
        ScryfallLegality::default()
    }

    /// Monta o índice a partir das linhas cruas do catálogo:
    /// `(nome, raridade, objeto legalities em JSON)`.
    ///
    /// Linha com raridade irreconhecível é **descartada**, não adivinhada: uma
    /// raridade errada faria a carta passar ou falhar em Pauper por engano, e
    /// engano silencioso é pior que carta faltando.
    pub fn from_rows<I, S>(rows: I) -> ScryfallLegality
    where
        I: IntoIterator<Item = (S, S, S)>,
        S: AsRef<str>,
    {
        let mut out = ScryfallLegality::new();
        for (name, rarity, legalities) in rows {
            let Some(rarity) = Rarity::from_slug(rarity.as_ref()) else {
                continue;
            };
            out.insert_row(name.as_ref(), rarity, legalities.as_ref());
        }
        out
    }

    fn insert_row(&mut self, name: &str, rarity: Rarity, legalities_json: &str) {
        let statuses = parse_legalities(legalities_json);
        let mut legal = [false; Format::ALL.len()];
        for (i, format) in Format::ALL.into_iter().enumerate() {
            legal[i] = match format.scryfall_key() {
                // Casual não é sancionado: não há lista, então nada é ilegal.
                None => true,
                Some(key) => statuses
                    .get(key)
                    .is_some_and(|status| is_playable_status(status)),
            };
        }
        self.entries
            .insert(name.to_ascii_lowercase(), Entry { rarity, legal });
    }

    /// Registra as cartas curadas em Lua como conhecidas e legais.
    ///
    /// Ver o campo `curated`: o filtro de "existe em papel" do importador
    /// descarta a impressão digital-only de cartas antigas, e sem isto elas
    /// aparecem como ilegais em todo formato.
    pub fn with_curated(mut self, db: &CardDatabase) -> ScryfallLegality {
        for card in &db.cards {
            self.curated.insert(card.name.to_ascii_lowercase());
        }
        self
    }

    /// Quantas cartas curadas cobrem buraco do índice do Scryfall.
    pub fn curated_len(&self) -> usize {
        self.curated.len()
    }

    /// Carrega o índice do catálogo em SQLite, somente leitura.
    pub fn from_sqlite(path: &Path) -> Result<ScryfallLegality, LegalityDbError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(
            |source| LegalityDbError::Sqlite {
                path: path.display().to_string(),
                source,
            },
        )?;
        Self::from_connection(&conn).map_err(|err| match err {
            LegalityDbError::Sqlite { source, .. } => LegalityDbError::Sqlite {
                path: path.display().to_string(),
                source,
            },
        })
    }

    /// Mesma carga sobre uma conexão já aberta — é o que o teste usa, com um
    /// banco em memória, para exercitar a `SELECT` de verdade sem depender de
    /// um arquivo de 80 MB existir na máquina.
    pub fn from_connection(conn: &Connection) -> Result<ScryfallLegality, LegalityDbError> {
        let fail = |source: rusqlite::Error| LegalityDbError::Sqlite {
            path: "<conexão aberta>".to_string(),
            source,
        };
        let mut stmt = conn.prepare(QUERY).map_err(fail)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(fail)?;

        let mut out = ScryfallLegality::new();
        for row in rows {
            let (name, rarity, legalities) = row.map_err(fail)?;
            let Some(rarity) = Rarity::from_slug(&rarity) else {
                continue;
            };
            out.insert_row(&name, rarity, &legalities);
        }
        Ok(out)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn get(&self, card: &str) -> Option<&Entry> {
        self.entries.get(&card.to_ascii_lowercase())
    }
}

impl LegalitySource for ScryfallLegality {
    fn legal_in(&self, card: &str, format: Format) -> bool {
        let Some(entry) = self.get(card) else {
            // Curada em Lua e ausente do bulk: conhecida, logo legal. Ver `curated`.
            return self.curated.contains(&card.to_ascii_lowercase());
        };
        let Some(index) = Format::ALL.iter().position(|f| *f == format) else {
            return false;
        };
        entry.legal.get(index).copied().unwrap_or(false)
    }

    fn rarity(&self, card: &str) -> Option<Rarity> {
        self.get(card).map(|e| e.rarity)
    }
}

// ---------------------------------------------------------------------------
// Parser do objeto `legalities`
// ---------------------------------------------------------------------------

/// Lê `{"standard":"legal","modern":"banned",...}` — objeto plano de string
/// para string, que é exatamente o formato do campo do Scryfall.
///
/// Escrito à mão em vez de puxar um parser de JSON como dependência (regra da
/// casa: se cabe em menos de 30 linhas, escreva). Entrada malformada devolve o
/// que deu para ler, e o efeito prático é a carta ficar ilegal — o lado seguro.
fn parse_legalities(json: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut chars = json.chars();
    loop {
        let Some(key) = next_quoted(&mut chars) else {
            return out;
        };
        // Entre chave e valor tem de vir `:`; sem ele o par está quebrado.
        let mut saw_colon = false;
        for c in chars.by_ref() {
            if c == ':' {
                saw_colon = true;
                break;
            }
            if !c.is_whitespace() {
                return out;
            }
        }
        if !saw_colon {
            return out;
        }
        let Some(value) = next_quoted(&mut chars) else {
            return out;
        };
        out.insert(key, value);
    }
}

/// Próxima string entre aspas, com `\"` e `\\` respeitados. `None` quando o
/// objeto acabou (`}`) ou o texto terminou no meio.
fn next_quoted(chars: &mut std::str::Chars<'_>) -> Option<String> {
    let mut opened = false;
    for c in chars.by_ref() {
        if c == '"' {
            opened = true;
            break;
        }
        if c == '}' {
            return None;
        }
    }
    if !opened {
        return None;
    }
    let mut out = String::new();
    let mut escaped = false;
    for c in chars.by_ref() {
        if escaped {
            out.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => return Some(out),
            _ => out.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Linha real do catálogo (Nissa, Worldsoul Speaker), campos podados.
    const NISSA: &str = r#"{"alchemy":"not_legal","brawl":"not_legal","commander":"legal","duel":"legal","legacy":"legal","modern":"not_legal","pauper":"not_legal","standard":"not_legal","vintage":"legal"}"#;

    #[test]
    fn objeto_legalities_do_scryfall_e_lido_par_a_par() {
        let map = parse_legalities(NISSA);
        assert_eq!(map.get("commander").map(String::as_str), Some("legal"));
        assert_eq!(map.get("standard").map(String::as_str), Some("not_legal"));
        assert_eq!(map.get("pauper").map(String::as_str), Some("not_legal"));
        assert_eq!(map.get("nao_existe"), None);
        assert_eq!(map.len(), 9);
    }

    #[test]
    fn banimento_e_rotacao_valem_e_carta_ausente_e_ilegal() {
        let src = ScryfallLegality::from_rows([
            ("Nissa, Worldsoul Speaker", "rare", NISSA),
            (
                "Lightning Bolt",
                "common",
                r#"{"standard":"not_legal","modern":"legal","pauper":"legal","commander":"legal"}"#,
            ),
            (
                "Sensei Divining Top",
                "rare",
                r#"{"standard":"not_legal","modern":"banned","pauper":"not_legal","commander":"legal"}"#,
            ),
        ]);
        assert_eq!(src.len(), 3);

        // Rotação: fora de Standard, dentro de Commander.
        assert!(!src.legal_in("Nissa, Worldsoul Speaker", Format::Standard));
        assert!(src.legal_in("nissa, worldsoul speaker", Format::Commander));

        // Banimento: `banned` não é jogável, mesmo com a carta conhecida.
        assert!(!src.legal_in("Sensei Divining Top", Format::Modern));
        assert!(src.legal_in("Sensei Divining Top", Format::Commander));

        assert!(src.legal_in("Lightning Bolt", Format::Modern));
        assert!(src.legal_in("Lightning Bolt", Format::Pauper));
        assert_eq!(src.rarity("LIGHTNING BOLT"), Some(Rarity::Common));

        // Casual não tem lista: tudo o que a fonte conhece passa.
        assert!(src.legal_in("Sensei Divining Top", Format::Casual));

        // Carta que a fonte não conhece é ilegal em todo formato.
        for f in Format::ALL {
            assert!(!src.legal_in("Black Lotus", f), "desconhecida passou em {f}");
        }
        assert_eq!(src.rarity("Black Lotus"), None);
    }

    #[test]
    fn linha_com_raridade_desconhecida_e_descartada() {
        let src = ScryfallLegality::from_rows([(
            "Carta Torta",
            "raridade-que-nao-existe",
            r#"{"modern":"legal"}"#,
        )]);
        assert!(src.is_empty());
        assert!(!src.legal_in("Carta Torta", Format::Modern));
    }

    #[test]
    fn carrega_do_catalogo_em_sqlite_com_a_mesma_select_da_producao() {
        // Banco em memória com as três colunas que a `SELECT` usa: se o esquema
        // de `mtg-db` renomear uma delas, este teste é quem avisa.
        let conn = match Connection::open_in_memory() {
            Ok(c) => c,
            Err(e) => panic!("sqlite em memória deve abrir: {e}"),
        };
        if let Err(e) = conn.execute("CREATE TABLE cards (name TEXT, rarity TEXT, legalities TEXT)", []) {
            panic!("esquema de teste não criou: {e}");
        }
        let rows = [
            ("Lightning Bolt", "common", r#"{"modern":"legal","pauper":"legal","standard":"not_legal","commander":"legal"}"#),
            ("Sensei Divining Top", "rare", r#"{"modern":"banned","pauper":"not_legal","standard":"not_legal","commander":"legal"}"#),
        ];
        for (n, r, l) in rows {
            if let Err(e) = conn.execute(
                "INSERT INTO cards (name, rarity, legalities) VALUES (?1, ?2, ?3)",
                (n, r, l),
            ) {
                panic!("insert de teste falhou: {e}");
            }
        }
        // Linha sem legalidade: a `SELECT` filtra, então não entra no índice.
        if let Err(e) = conn.execute(
            "INSERT INTO cards (name, rarity, legalities) VALUES ('Sem Dado', 'rare', NULL)",
            [],
        ) {
            panic!("insert nulo de teste falhou: {e}");
        }

        let src = match ScryfallLegality::from_connection(&conn) {
            Ok(s) => s,
            Err(e) => panic!("carga do sqlite falhou: {e}"),
        };
        assert_eq!(src.len(), 2, "a linha sem legalities não podia entrar");
        assert!(src.legal_in("Lightning Bolt", Format::Pauper));
        assert!(!src.legal_in("Sensei Divining Top", Format::Modern));
        assert!(!src.legal_in("Sem Dado", Format::Commander));
        assert_eq!(src.rarity("Sensei Divining Top"), Some(Rarity::Rare));
    }
}
