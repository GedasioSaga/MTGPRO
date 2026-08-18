//! Gravação do catálogo importado em SQLite.
//!
//! Tabela própria, separada de `cards` (que é de `mtg-db` e guarda as cartas
//! escritas à mão em Lua). Duas razões: as 172 cartas escritas à mão têm IR
//! melhor que qualquer coisa derivada de texto e não podem ser sobrescritas por
//! nome; e o catálogo importado precisa de colunas que `cards` não tem
//! (`playable`, `oracle_id`, imagem, legalidade).
//!
//! Todo valor entra por placeholder. Nenhuma cláusula é montada por
//! concatenação de string.
use std::path::Path;

use rusqlite::{params, Connection};

use crate::ImportError;

/// Uma linha pronta para gravar. Campo textual guarda o dado do Scryfall como
/// veio; `definition` guarda o `CardDef` serializado quando a carta compilou.
#[derive(Debug, Clone, Default)]
pub struct CardRow {
    pub card_key: String,
    pub oracle_id: Option<String>,
    pub name: String,
    pub mana_cost: String,
    pub mana_value: i64,
    pub type_line: String,
    pub oracle_text: String,
    pub power: Option<String>,
    pub toughness: Option<String>,
    pub loyalty: Option<String>,
    pub colors: String,
    pub color_identity: String,
    pub keywords: String,
    pub rarity: String,
    pub set_code: String,
    pub collector_number: String,
    pub artist: Option<String>,
    pub image_normal: Option<String>,
    pub image_art_crop: Option<String>,
    pub layout: String,
    pub legalities: String,
    pub playable: bool,
    pub unplayable_reason: Option<String>,
    /// Texto de oráculo normalizado que travou a compilação. Fica no banco
    /// para a rodada seguinte poder consultar padrão sem reimportar.
    pub unplayable_pattern: Option<String>,
    pub definition: Option<String>,
}

pub struct ImportStore {
    conn: Connection,
}

impl ImportStore {
    pub fn open(path: &Path) -> Result<ImportStore, ImportError> {
        let store = ImportStore { conn: Connection::open(path)? };
        store.tune()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<ImportStore, ImportError> {
        let store = ImportStore { conn: Connection::open_in_memory()? };
        store.tune()?;
        store.migrate()?;
        Ok(store)
    }

    /// 35 mil INSERTs numa transação só: o WAL e o synchronous relaxado tiram
    /// o fsync por commit, que é o gargalo real desta carga.
    fn tune(&self) -> Result<(), ImportError> {
        self.conn.pragma_update(None, "journal_mode", "WAL")?;
        self.conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    }

    pub fn migrate(&self) -> Result<(), ImportError> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS scryfall_cards (
                card_key          TEXT PRIMARY KEY,
                oracle_id         TEXT,
                name              TEXT NOT NULL,
                mana_cost         TEXT NOT NULL,
                mana_value        INTEGER NOT NULL,
                type_line         TEXT NOT NULL,
                oracle_text       TEXT NOT NULL,
                power             TEXT,
                toughness         TEXT,
                loyalty           TEXT,
                colors            TEXT NOT NULL,
                color_identity    TEXT NOT NULL,
                keywords          TEXT NOT NULL,
                rarity            TEXT NOT NULL,
                set_code          TEXT NOT NULL,
                collector_number  TEXT NOT NULL,
                artist            TEXT,
                image_normal      TEXT,
                image_art_crop    TEXT,
                layout            TEXT NOT NULL,
                legalities        TEXT NOT NULL,
                playable          INTEGER NOT NULL,
                unplayable_reason TEXT,
                unplayable_pattern TEXT,
                definition        TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_scryfall_name ON scryfall_cards(name);
            CREATE INDEX IF NOT EXISTS idx_scryfall_playable ON scryfall_cards(playable);
            CREATE INDEX IF NOT EXISTS idx_scryfall_mana_value ON scryfall_cards(mana_value);
            CREATE INDEX IF NOT EXISTS idx_scryfall_type_line ON scryfall_cards(type_line);

            CREATE TABLE IF NOT EXISTS scryfall_import_meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        self.add_missing_columns()?;
        Ok(())
    }

    /// Banco criado por uma versão anterior não tem as colunas novas.
    /// `ALTER TABLE` não aceita `IF NOT EXISTS`, então a coluna é conferida
    /// antes; o nome vem de constante do código, nunca de fora.
    fn add_missing_columns(&self) -> Result<(), ImportError> {
        for (column, ddl) in [(
            "unplayable_pattern",
            "ALTER TABLE scryfall_cards ADD COLUMN unplayable_pattern TEXT",
        )] {
            if !self.has_column("scryfall_cards", column)? {
                self.conn.execute_batch(ddl)?;
            }
        }
        Ok(())
    }

    fn has_column(&self, table: &str, column: &str) -> Result<bool, ImportError> {
        let mut stmt = self.conn.prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")?;
        let mut rows = stmt.query(params![table, column])?;
        Ok(rows.next()?.is_some())
    }

    pub fn begin(&mut self) -> Result<WriteSession<'_>, ImportError> {
        Ok(WriteSession { tx: self.conn.transaction()? })
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), ImportError> {
        self.conn.execute(
            "INSERT INTO scryfall_import_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>, ImportError> {
        let mut stmt =
            self.conn.prepare("SELECT value FROM scryfall_import_meta WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Passa o conteúdo do WAL para o arquivo principal e esvazia o log.
    /// Sem isso o `.sqlite` fica pequeno e o dado real mora no `.sqlite-wal`.
    pub fn checkpoint(&self) -> Result<(), ImportError> {
        self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    pub fn count(&self) -> Result<i64, ImportError> {
        let n =
            self.conn.query_row("SELECT COUNT(*) FROM scryfall_cards", [], |row| row.get(0))?;
        Ok(n)
    }

    pub fn count_playable(&self) -> Result<i64, ImportError> {
        let n = self.conn.query_row(
            "SELECT COUNT(*) FROM scryfall_cards WHERE playable = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(n)
    }

    /// Cartas jogáveis, para o servidor montar deck. `definition` é o `CardDef`
    /// em JSON, na mesma forma que `mtg-db` usa.
    pub fn playable_definitions(&self, limit: i64) -> Result<Vec<String>, ImportError> {
        let mut stmt = self.conn.prepare(
            "SELECT definition FROM scryfall_cards
             WHERE playable = 1 AND definition IS NOT NULL
             ORDER BY name LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

/// Transação aberta durante a importação inteira.
pub struct WriteSession<'a> {
    tx: rusqlite::Transaction<'a>,
}

impl WriteSession<'_> {
    pub fn upsert(&self, row: &CardRow) -> Result<(), ImportError> {
        self.tx.execute(
            r#"
            INSERT INTO scryfall_cards (
                card_key, oracle_id, name, mana_cost, mana_value, type_line, oracle_text,
                power, toughness, loyalty, colors, color_identity, keywords, rarity,
                set_code, collector_number, artist, image_normal, image_art_crop, layout,
                legalities, playable, unplayable_reason, unplayable_pattern, definition
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25
            )
            ON CONFLICT(card_key) DO UPDATE SET
                oracle_id         = excluded.oracle_id,
                name              = excluded.name,
                mana_cost         = excluded.mana_cost,
                mana_value        = excluded.mana_value,
                type_line         = excluded.type_line,
                oracle_text       = excluded.oracle_text,
                power             = excluded.power,
                toughness         = excluded.toughness,
                loyalty           = excluded.loyalty,
                colors            = excluded.colors,
                color_identity    = excluded.color_identity,
                keywords          = excluded.keywords,
                rarity            = excluded.rarity,
                set_code          = excluded.set_code,
                collector_number  = excluded.collector_number,
                artist            = excluded.artist,
                image_normal      = excluded.image_normal,
                image_art_crop    = excluded.image_art_crop,
                layout            = excluded.layout,
                legalities        = excluded.legalities,
                playable          = excluded.playable,
                unplayable_reason = excluded.unplayable_reason,
                unplayable_pattern = excluded.unplayable_pattern,
                definition        = excluded.definition
            "#,
            params![
                row.card_key,
                row.oracle_id,
                row.name,
                row.mana_cost,
                row.mana_value,
                row.type_line,
                row.oracle_text,
                row.power,
                row.toughness,
                row.loyalty,
                row.colors,
                row.color_identity,
                row.keywords,
                row.rarity,
                row.set_code,
                row.collector_number,
                row.artist,
                row.image_normal,
                row.image_art_crop,
                row.layout,
                row.legalities,
                row.playable as i64,
                row.unplayable_reason,
                row.unplayable_pattern,
                row.definition,
            ],
        )?;
        Ok(())
    }

    pub fn commit(self) -> Result<(), ImportError> {
        self.tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, playable: bool) -> CardRow {
        CardRow {
            card_key: key.to_string(),
            name: format!("Carta {key}"),
            mana_cost: "{R}".to_string(),
            mana_value: 1,
            type_line: "Instant".to_string(),
            oracle_text: "texto".to_string(),
            colors: "R".to_string(),
            color_identity: "R".to_string(),
            keywords: String::new(),
            rarity: "common".to_string(),
            set_code: "tst".to_string(),
            collector_number: "1".to_string(),
            layout: "normal".to_string(),
            legalities: "{}".to_string(),
            playable,
            definition: playable.then(|| "{}".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn upsert_is_idempotent_by_key() {
        let mut store = ImportStore::open_in_memory().expect("abrir");
        {
            let s = store.begin().expect("transação");
            s.upsert(&row("a", true)).expect("insert");
            s.upsert(&row("b", false)).expect("insert");
            s.upsert(&row("a", true)).expect("upsert repetido");
            s.commit().expect("commit");
        }
        assert_eq!(store.count().expect("contar"), 2);
        assert_eq!(store.count_playable().expect("contar jogáveis"), 1);
    }

    #[test]
    fn meta_round_trip() {
        let store = ImportStore::open_in_memory().expect("abrir");
        assert_eq!(store.meta("updated_at").expect("ler"), None);
        store.set_meta("updated_at", "2026-08-18").expect("gravar");
        store.set_meta("updated_at", "2026-08-19").expect("regravar");
        assert_eq!(store.meta("updated_at").expect("ler"), Some("2026-08-19".to_string()));
    }

    #[test]
    fn playable_definitions_are_ordered_by_name() {
        let mut store = ImportStore::open_in_memory().expect("abrir");
        {
            let s = store.begin().expect("transação");
            s.upsert(&row("z", true)).expect("insert");
            s.upsert(&row("a", true)).expect("insert");
            s.upsert(&row("m", false)).expect("insert");
            s.commit().expect("commit");
        }
        let defs = store.playable_definitions(10).expect("consultar");
        assert_eq!(defs.len(), 2, "não jogável não entra em deck");
    }
}
