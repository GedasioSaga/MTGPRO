//! Persistência do catálogo de cartas e de decks.
//!
//! SQLite embutido (rusqlite "bundled") guarda duas cópias de cada carta:
//! colunas soltas (indexáveis, usadas por `search`) e uma coluna `definition`
//! com o `CardDef` inteiro em JSON, que é a fonte da verdade ao carregar —
//! assim um campo novo em `CardDef` nunca é perdido por esquecimento de
//! coluna. O schema evita sintaxe exclusiva do SQLite (além de
//! AUTOINCREMENT) para poder migrar para PostgreSQL depois.
use std::path::Path;

use mtg_core::{CardDatabase, CardDef, CardType, Color, ColorSet, ManaSymbol, Rarity};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, ToSql};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("erro de SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("erro de serialização JSON: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("deck não encontrado: {0}")]
    DeckNotFound(String),
}

/// Filtro de busca no catálogo. Todo campo `None` é ignorado.
#[derive(Debug, Clone, Default)]
pub struct CardQuery {
    /// Casa contra nome ou texto de oráculo (substring, sem diferenciar caixa).
    pub text: Option<String>,
    /// Cartas cuja identidade de cor intersecta alguma destas cores.
    pub colors: Option<Vec<Color>>,
    /// Cartas que têm ao menos um destes tipos.
    pub types: Option<Vec<CardType>>,
    pub mana_value_max: Option<u32>,
    /// Custo de mana convertido exato.
    pub mana_value: Option<u32>,
    /// Código de coleção, sem diferenciar caixa.
    pub set_code: Option<String>,
    /// `Some(true)` = só o que o motor sabe jogar; `Some(false)` = só o resto.
    pub playable: Option<bool>,
    pub limit: usize,
    pub offset: usize,
}

/// Carta pronta para inserção em lote vinda de fonte externa (Scryfall).
///
/// `def` carrega sempre o metadado — nome, custo, tipos, raridade. As
/// habilidades dentro dele só são fiéis quando `playable` é verdadeiro; para
/// carta não compilada elas ficam vazias de propósito, porque IR parcial no
/// motor é pior que carta ausente.
#[derive(Debug, Clone)]
pub struct ImportedCard {
    pub oracle_id: String,
    pub def: CardDef,
    pub playable: bool,
    /// Por que não deu para compilar (frase legível, para a UI).
    pub unsupported_reason: Option<String>,
    /// Trecho do oracle text que derrubou a compilação — é o que agrupa o
    /// relatório de cobertura por padrão faltante.
    pub unsupported_pattern: Option<String>,
    pub image_art_crop: Option<String>,
    pub image_normal: Option<String>,
    /// JSON cru do mapa de legalidades do Scryfall.
    pub legalities: Option<String>,
}

/// Linha de catálogo para listagem — o que a UI precisa sem pagar o parse do
/// `CardDef` inteiro. Com página de 200 isso é a diferença entre KB e MB.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CardSummary {
    pub oracle_id: Option<String>,
    pub name: String,
    pub mana_cost: String,
    pub mana_value: u32,
    pub colors: Vec<String>,
    pub type_line: String,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub rarity: String,
    pub set_code: String,
    pub oracle_text: String,
    pub playable: bool,
    pub unsupported_reason: Option<String>,
    pub unsupported_pattern: Option<String>,
    pub image_art_crop: Option<String>,
    pub image_normal: Option<String>,
}

/// Carta única: resumo mais o `CardDef` completo (a IR que o motor executa).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CardDetail {
    #[serde(flatten)]
    pub summary: CardSummary,
    pub legalities: Option<String>,
    pub imported_at: Option<String>,
    pub definition: CardDef,
}

/// Página de busca. `total` é a contagem sem `limit`/`offset` — é o que
/// permite ao cliente saber quantas páginas existem.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CardPage {
    pub total: u64,
    pub limit: usize,
    pub offset: usize,
    pub items: Vec<CardSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SetCount {
    pub set_code: String,
    pub total: u64,
    pub playable: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CatalogStats {
    pub total: u64,
    pub playable: u64,
    pub unsupported: u64,
    /// Ordenado por `set_code` — saída determinística, nunca ordem de hash.
    pub by_set: Vec<SetCount>,
}

pub struct CardStore {
    conn: Connection,
}

impl CardStore {
    pub fn open(path: &Path) -> Result<CardStore, DbError> {
        let conn = Connection::open(path)?;
        let store = CardStore { conn };
        store.init_connection()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<CardStore, DbError> {
        let conn = Connection::open_in_memory()?;
        let store = CardStore { conn };
        store.init_connection()?;
        store.migrate()?;
        Ok(store)
    }

    fn init_connection(&self) -> Result<(), DbError> {
        // deck_cards referencia decks por nome — cascata evita registro órfão
        // quando um deck é reescrito.
        self.conn.pragma_update(None, "foreign_keys", true)?;
        Ok(())
    }

    /// Cria as tabelas e índices se ainda não existirem. Idempotente.
    pub fn migrate(&self) -> Result<(), DbError> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS cards (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                name            TEXT NOT NULL UNIQUE,
                mana_cost_text  TEXT NOT NULL,
                mana_value      INTEGER NOT NULL,
                colors_mask     INTEGER NOT NULL,
                types_mask      INTEGER NOT NULL,
                type_line_text  TEXT NOT NULL,
                power           INTEGER,
                toughness       INTEGER,
                rarity          TEXT NOT NULL,
                set_code        TEXT NOT NULL,
                art_key         TEXT,
                oracle_text     TEXT NOT NULL,
                definition      TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cards_name ON cards(name);
            CREATE INDEX IF NOT EXISTS idx_cards_mana_value ON cards(mana_value);
            CREATE INDEX IF NOT EXISTS idx_cards_colors_mask ON cards(colors_mask);
            CREATE INDEX IF NOT EXISTS idx_cards_types_mask ON cards(types_mask);
            CREATE INDEX IF NOT EXISTS idx_cards_oracle_text ON cards(oracle_text);

            CREATE TABLE IF NOT EXISTS decks (
                name        TEXT PRIMARY KEY,
                description TEXT,
                colors      TEXT
            );
            CREATE TABLE IF NOT EXISTS deck_cards (
                deck_name TEXT NOT NULL REFERENCES decks(name) ON DELETE CASCADE,
                card_name TEXT NOT NULL,
                quantity  INTEGER NOT NULL,
                PRIMARY KEY (deck_name, card_name)
            );
            "#,
        )?;
        self.add_import_columns()?;
        // Índices criados depois das colunas — `oracle_id` e `playable` só
        // existem a partir de `add_import_columns`.
        self.conn.execute_batch(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_cards_oracle_id ON cards(oracle_id);
            CREATE INDEX IF NOT EXISTS idx_cards_playable ON cards(playable);
            CREATE INDEX IF NOT EXISTS idx_cards_set_code ON cards(set_code);
            CREATE INDEX IF NOT EXISTS idx_cards_playable_name ON cards(playable, name);
            "#,
        )?;
        Ok(())
    }

    /// Colunas do catálogo amplo (Scryfall) acrescentadas à tabela original.
    ///
    /// `ALTER TABLE ADD COLUMN` não é idempotente no SQLite — repetir dá erro
    /// de coluna duplicada —, então a lista é filtrada pelo que `PRAGMA
    /// table_info` já reporta. Nenhum `DROP`, nenhum `CREATE TABLE ... AS`:
    /// o catálogo Lua já gravado tem de sobreviver à migração.
    ///
    /// `oracle_id` entra sem `UNIQUE` porque o SQLite proíbe restrição de
    /// unicidade em `ADD COLUMN`; a unicidade vem do índice criado em
    /// `migrate`, que trata NULLs como distintos — exatamente o que se quer,
    /// já que carta curada em Lua não tem `oracle_id`.
    fn add_import_columns(&self) -> Result<(), DbError> {
        const COLUMNS: [(&str, &str); 8] = [
            ("oracle_id", "TEXT"),
            ("playable", "BOOLEAN NOT NULL DEFAULT 1"),
            ("unsupported_reason", "TEXT"),
            ("unsupported_pattern", "TEXT"),
            ("image_art_crop", "TEXT"),
            ("image_normal", "TEXT"),
            ("legalities", "TEXT"),
            ("imported_at", "TEXT"),
        ];

        let existing = self.column_names("cards")?;
        for (name, decl) in COLUMNS {
            if existing.iter().any(|c| c == name) {
                continue;
            }
            // `name` e `decl` são literais desta constante, nunca entrada
            // externa — não há caminho de injeção aqui, e DDL do SQLite não
            // aceita placeholder para identificador.
            self.conn.execute_batch(&format!("ALTER TABLE cards ADD COLUMN {name} {decl}"))?;
        }
        Ok(())
    }

    fn column_names(&self, table: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare("SELECT name FROM pragma_table_info(?1)")?;
        let rows = stmt.query_map(params![table], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Insere ou atualiza cada carta do banco em memória. Idempotente por nome.
    pub fn seed_from(&self, db: &CardDatabase) -> Result<usize, DbError> {
        let tx = self.conn.unchecked_transaction()?;
        let mut count = 0usize;
        for card in &db.cards {
            let definition = serde_json::to_string(card)?;
            let colors_mask = card.colors().0 as i64;
            let types_mask = type_line_mask(card) as i64;
            tx.execute(
                r#"
                INSERT INTO cards (
                    name, mana_cost_text, mana_value, colors_mask, types_mask,
                    type_line_text, power, toughness, rarity, set_code, art_key,
                    oracle_text, definition
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                ON CONFLICT(name) DO UPDATE SET
                    mana_cost_text = excluded.mana_cost_text,
                    mana_value     = excluded.mana_value,
                    colors_mask    = excluded.colors_mask,
                    types_mask     = excluded.types_mask,
                    type_line_text = excluded.type_line_text,
                    power          = excluded.power,
                    toughness      = excluded.toughness,
                    rarity         = excluded.rarity,
                    set_code       = excluded.set_code,
                    art_key        = excluded.art_key,
                    oracle_text    = excluded.oracle_text,
                    definition     = excluded.definition,
                    -- Carta curada ganha o nome: a linha volta a ser Lua pura,
                    -- mesmo que o import do Scryfall tenha chegado antes.
                    oracle_id           = NULL,
                    playable            = 1,
                    unsupported_reason  = NULL,
                    unsupported_pattern = NULL
                "#,
                params![
                    card.name,
                    mana_cost_text(card),
                    card.mana_value() as i64,
                    colors_mask,
                    types_mask,
                    card.type_line.render(),
                    card.power,
                    card.toughness,
                    rarity_text(card.rarity),
                    card.set_code,
                    card.art_key,
                    card.oracle_text,
                    definition,
                ],
            )?;
            count += 1;
        }
        tx.commit()?;
        Ok(count)
    }

    /// Grava um lote de cartas importadas em UMA transação.
    ///
    /// Chave de identidade é `oracle_id`, não `name`: reimportar o bulk do
    /// Scryfall atualiza a linha existente em vez de duplicar. Quando o nome
    /// já pertence a uma carta curada em Lua, o `INSERT OR IGNORE` deixa a
    /// carta curada de pé — é a regra de convivência das duas fontes, e ela
    /// vive aqui e não no chamador para não haver duas versões dela.
    ///
    /// Devolve quantas linhas foram de fato escritas (inseridas ou
    /// atualizadas); a diferença para `batch.len()` é o número de cartas do
    /// Scryfall ofuscadas por uma carta Lua de mesmo nome.
    pub fn import_cards(&self, batch: &[ImportedCard]) -> Result<usize, DbError> {
        let tx = self.conn.unchecked_transaction()?;
        let mut written = 0usize;
        {
            let mut update = tx.prepare(
                r#"
                UPDATE cards SET
                    name = ?2, mana_cost_text = ?3, mana_value = ?4, colors_mask = ?5,
                    types_mask = ?6, type_line_text = ?7, power = ?8, toughness = ?9,
                    rarity = ?10, set_code = ?11, art_key = ?12, oracle_text = ?13,
                    definition = ?14, playable = ?15, unsupported_reason = ?16,
                    unsupported_pattern = ?17, image_art_crop = ?18, image_normal = ?19,
                    legalities = ?20, imported_at = ?21
                WHERE oracle_id = ?1
                "#,
            )?;
            let mut insert = tx.prepare(
                r#"
                INSERT OR IGNORE INTO cards (
                    oracle_id, name, mana_cost_text, mana_value, colors_mask,
                    types_mask, type_line_text, power, toughness, rarity, set_code,
                    art_key, oracle_text, definition, playable, unsupported_reason,
                    unsupported_pattern, image_art_crop, image_normal, legalities,
                    imported_at
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                    ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
                )
                "#,
            )?;

            let now = now_iso8601();
            for card in batch {
                let def = &card.def;
                let definition = serde_json::to_string(def)?;
                let values = params![
                    card.oracle_id,
                    def.name,
                    mana_cost_text(def),
                    def.mana_value() as i64,
                    def.colors().0 as i64,
                    type_line_mask(def) as i64,
                    def.type_line.render(),
                    def.power,
                    def.toughness,
                    rarity_text(def.rarity),
                    def.set_code,
                    def.art_key,
                    def.oracle_text,
                    definition,
                    card.playable,
                    card.unsupported_reason,
                    card.unsupported_pattern,
                    card.image_art_crop,
                    card.image_normal,
                    card.legalities,
                    now,
                ];
                // Atualiza primeiro; só insere se o `oracle_id` ainda não
                // existe. Fazer o contrário exigiria upsert com dois alvos de
                // conflito (`name` e `oracle_id`), cuja ordem de disparo não é
                // garantida.
                let mut touched = update.execute(values)?;
                if touched == 0 {
                    touched = insert.execute(values)?;
                }
                written += touched;
            }
        }
        tx.commit()?;
        Ok(written)
    }

    /// Carrega o catálogo inteiro. `definition` é a fonte da verdade — as
    /// colunas indexáveis só existem para acelerar `search`.
    pub fn load_all(&self) -> Result<CardDatabase, DbError> {
        let mut stmt = self.conn.prepare("SELECT definition FROM cards ORDER BY id")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut cards = Vec::new();
        for definition in rows {
            let json = definition?;
            let card: CardDef = serde_json::from_str(&json)?;
            cards.push(card);
        }
        Ok(CardDatabase { cards })
    }

    /// Só o que o motor sabe executar. É este o catálogo que vira `Game` —
    /// `load_all` traria as ~35 mil linhas do Scryfall, incluindo as que não
    /// têm IR fiel, e carta sem comportamento em deck é bug silencioso.
    pub fn load_playable(&self) -> Result<CardDatabase, DbError> {
        let mut stmt =
            self.conn.prepare("SELECT definition FROM cards WHERE playable ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut cards = Vec::new();
        for definition in rows {
            let card: CardDef = serde_json::from_str(&definition?)?;
            cards.push(card);
        }
        let mut db = CardDatabase { cards };
        db.reindex();
        Ok(db)
    }

    pub fn find_by_name(&self, name: &str) -> Result<Option<CardDef>, DbError> {
        let definition: Option<String> = self
            .conn
            .query_row(
                "SELECT definition FROM cards WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        match definition {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Busca cartas por filtros combinados, devolvendo o `CardDef` completo.
    /// Continua existindo para quem precisa da IR (montagem de deck, testes);
    /// para listar catálogo use `search_page`, que não paga o parse do JSON.
    pub fn search(&self, q: &CardQuery) -> Result<Vec<CardDef>, DbError> {
        let (where_sql, filter_params) = build_filter(q);
        let sql = format!("SELECT definition FROM cards{where_sql} ORDER BY name LIMIT ? OFFSET ?");
        let mut params = filter_params;
        params.push(Box::new(q.limit as i64));
        params.push(Box::new(q.offset as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_from_iter(param_refs), |row| row.get::<_, String>(0))?;

        let mut out = Vec::new();
        for definition in rows {
            let json = definition?;
            out.push(serde_json::from_str(&json)?);
        }
        Ok(out)
    }

    /// Busca paginada para listagem. `total` vem de um `COUNT(*)` com o mesmo
    /// `WHERE`, então o cliente sabe quantas páginas existem sem baixar todas.
    ///
    /// A ordenação é `name`, que é UNIQUE na tabela: duas páginas seguidas
    /// nunca repetem nem pulam item, o que uma ordenação por coluna repetida
    /// (raridade, custo) não garantiria.
    pub fn search_page(&self, q: &CardQuery) -> Result<CardPage, DbError> {
        let (where_sql, filter_params) = build_filter(q);

        let count_sql = format!("SELECT COUNT(*) FROM cards{where_sql}");
        let total: i64 = {
            let mut stmt = self.conn.prepare(&count_sql)?;
            let refs: Vec<&dyn ToSql> = filter_params.iter().map(|p| p.as_ref()).collect();
            stmt.query_row(params_from_iter(refs), |row| row.get(0))?
        };

        let sql =
            format!("SELECT {SUMMARY_COLUMNS} FROM cards{where_sql} ORDER BY name LIMIT ? OFFSET ?");
        let mut params = filter_params;
        params.push(Box::new(q.limit as i64));
        params.push(Box::new(q.offset as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_from_iter(refs), row_to_summary)?;

        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }
        Ok(CardPage { total: total.max(0) as u64, limit: q.limit, offset: q.offset, items })
    }

    /// Uma carta pelo `oracle_id` do Scryfall, com resumo e `CardDef`.
    pub fn find_by_oracle_id(&self, oracle_id: &str) -> Result<Option<CardDetail>, DbError> {
        let sql = format!(
            "SELECT {SUMMARY_COLUMNS}, legalities, imported_at, definition \
             FROM cards WHERE oracle_id = ?1"
        );
        let row = self
            .conn
            .query_row(&sql, params![oracle_id], |row| {
                let summary = row_to_summary(row)?;
                let legalities: Option<String> = row.get(SUMMARY_COLUMN_COUNT)?;
                let imported_at: Option<String> = row.get(SUMMARY_COLUMN_COUNT + 1)?;
                let definition: String = row.get(SUMMARY_COLUMN_COUNT + 2)?;
                Ok((summary, legalities, imported_at, definition))
            })
            .optional()?;

        match row {
            Some((summary, legalities, imported_at, definition)) => Ok(Some(CardDetail {
                summary,
                legalities,
                imported_at,
                definition: serde_json::from_str(&definition)?,
            })),
            None => Ok(None),
        }
    }

    /// Totais do catálogo e recorte por coleção, para a tela de cobertura.
    pub fn stats(&self) -> Result<CatalogStats, DbError> {
        let (total, playable): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN playable THEN 1 ELSE 0 END), 0) FROM cards",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT set_code, COUNT(*), COALESCE(SUM(CASE WHEN playable THEN 1 ELSE 0 END), 0) \
             FROM cards GROUP BY set_code ORDER BY set_code",
        )?;
        let rows = stmt.query_map([], |row| {
            let total: i64 = row.get(1)?;
            let playable: i64 = row.get(2)?;
            Ok(SetCount {
                set_code: row.get(0)?,
                total: total.max(0) as u64,
                playable: playable.max(0) as u64,
            })
        })?;
        let mut by_set = Vec::new();
        for r in rows {
            by_set.push(r?);
        }

        let total = total.max(0) as u64;
        let playable = playable.max(0) as u64;
        Ok(CatalogStats { total, playable, unsupported: total.saturating_sub(playable), by_set })
    }

    /// Substitui a lista de cartas de um deck (upsert por nome).
    pub fn save_deck(&self, name: &str, cards: &[(String, u8)]) -> Result<(), DbError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO decks (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            params![name],
        )?;
        tx.execute("DELETE FROM deck_cards WHERE deck_name = ?1", params![name])?;
        for (card_name, quantity) in cards {
            tx.execute(
                "INSERT INTO deck_cards (deck_name, card_name, quantity) VALUES (?1, ?2, ?3)",
                params![name, card_name, *quantity as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_deck(&self, name: &str) -> Result<Vec<(String, u8)>, DbError> {
        let exists: Option<String> = self
            .conn
            .query_row("SELECT name FROM decks WHERE name = ?1", params![name], |row| row.get(0))
            .optional()?;
        if exists.is_none() {
            return Err(DbError::DeckNotFound(name.to_string()));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT card_name, quantity FROM deck_cards WHERE deck_name = ?1 ORDER BY card_name")?;
        let rows = stmt.query_map(params![name], |row| {
            let quantity: i64 = row.get(1)?;
            Ok((row.get::<_, String>(0)?, quantity as u8))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_decks(&self) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare("SELECT name FROM decks ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

/// Colunas de `CardSummary`, na ordem que `row_to_summary` espera. Uma lista
/// só, para índice e leitura nunca saírem de sincronia.
const SUMMARY_COLUMNS: &str = "oracle_id, name, mana_cost_text, mana_value, colors_mask, \
     type_line_text, power, toughness, rarity, set_code, oracle_text, playable, \
     unsupported_reason, unsupported_pattern, image_art_crop, image_normal";
const SUMMARY_COLUMN_COUNT: usize = 16;

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<CardSummary> {
    let mana_value: i64 = row.get(3)?;
    let colors_mask: i64 = row.get(4)?;
    let colors = ColorSet(colors_mask.clamp(0, u8::MAX as i64) as u8)
        .iter()
        .map(|c| c.letter().to_string())
        .collect();
    Ok(CardSummary {
        oracle_id: row.get(0)?,
        name: row.get(1)?,
        mana_cost: row.get(2)?,
        mana_value: mana_value.max(0) as u32,
        colors,
        type_line: row.get(5)?,
        power: row.get(6)?,
        toughness: row.get(7)?,
        rarity: row.get(8)?,
        set_code: row.get(9)?,
        oracle_text: row.get(10)?,
        playable: row.get(11)?,
        unsupported_reason: row.get(12)?,
        unsupported_pattern: row.get(13)?,
        image_art_crop: row.get(14)?,
        image_normal: row.get(15)?,
    })
}

/// Monta a cláusula `WHERE` (já com o espaço à esquerda, ou vazia) e os
/// parâmetros na mesma ordem. Todo valor de usuário entra por placeholder —
/// nenhuma cláusula é montada por concatenação de valor.
fn build_filter(q: &CardQuery) -> (String, Vec<Box<dyn ToSql>>) {
    let mut clauses: Vec<&str> = Vec::new();
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();

    if let Some(text) = &q.text {
        clauses.push("(name LIKE ? ESCAPE '\\' OR oracle_text LIKE ? ESCAPE '\\')");
        let pattern = like_pattern(text);
        params.push(Box::new(pattern.clone()));
        params.push(Box::new(pattern));
    }
    if let Some(colors) = &q.colors {
        if !colors.is_empty() {
            let mask = colors.iter().fold(ColorSet::COLORLESS, |acc, c| acc.union(ColorSet::single(*c)));
            clauses.push("(colors_mask & ?) != 0");
            params.push(Box::new(mask.0 as i64));
        }
    }
    if let Some(types) = &q.types {
        if !types.is_empty() {
            let mask: u16 = types.iter().fold(0u16, |acc, t| acc | card_type_bit(*t));
            clauses.push("(types_mask & ?) != 0");
            params.push(Box::new(mask as i64));
        }
    }
    if let Some(mv) = q.mana_value_max {
        clauses.push("mana_value <= ?");
        params.push(Box::new(mv as i64));
    }
    if let Some(mv) = q.mana_value {
        clauses.push("mana_value = ?");
        params.push(Box::new(mv as i64));
    }
    if let Some(set_code) = &q.set_code {
        clauses.push("set_code = ? COLLATE NOCASE");
        params.push(Box::new(set_code.clone()));
    }
    if let Some(playable) = q.playable {
        clauses.push("playable = ?");
        params.push(Box::new(playable));
    }

    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    (where_sql, params)
}

/// Instante atual em ISO-8601 UTC, sem depender de crate de data.
/// Algoritmo civil-from-days de Howard Hinnant — a era começa em 0000-03-01
/// para que o ano bissexto caia no fim do ciclo e a aritmética feche sem
/// caso especial. Relógio antes de 1970 vira o próprio epoch.
fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = era * 400 + yoe + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Escapa `%`, `_` e `\` do texto de busca antes de embutir em um `LIKE`,
/// para que caractere de usuário nunca vire coringa de padrão.
fn like_pattern(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('%');
    for c in text.chars() {
        match c {
            '%' | '_' | '\\' => {
                escaped.push('\\');
                escaped.push(c);
            }
            _ => escaped.push(c),
        }
    }
    escaped.push('%');
    escaped
}

/// Custo de mana na notação impressa (`{3}{G}{G}`), para exibição e índice —
/// `definition` (JSON) continua sendo a fonte da verdade estrutural.
///
/// Antes isto era `format!("{s:?}")`, que vazava `Generic(3) Colored(Green)`
/// para a API. A notação de chaves é a que o cliente sabe renderizar.
fn mana_cost_text(card: &CardDef) -> String {
    let mut out = String::new();
    for symbol in &card.mana_cost.symbols {
        match symbol {
            ManaSymbol::Generic(n) => out.push_str(&format!("{{{n}}}")),
            ManaSymbol::Colored(c) => out.push_str(&format!("{{{}}}", c.letter())),
            ManaSymbol::Colorless => out.push_str("{C}"),
            ManaSymbol::Hybrid(a, b) => {
                out.push_str(&format!("{{{}/{}}}", a.letter(), b.letter()))
            }
            ManaSymbol::MonoHybrid(c) => out.push_str(&format!("{{2/{}}}", c.letter())),
            ManaSymbol::Phyrexian(c) => out.push_str(&format!("{{{}/P}}", c.letter())),
            ManaSymbol::Snow => out.push_str("{S}"),
            ManaSymbol::X => out.push_str("{X}"),
        }
    }
    out
}

fn rarity_text(r: Rarity) -> &'static str {
    match r {
        Rarity::Common => "common",
        Rarity::Uncommon => "uncommon",
        Rarity::Rare => "rare",
        Rarity::Mythic => "mythic",
        Rarity::Special => "special",
    }
}

/// Bitmask de `CardType` para permitir filtro indexado em `search`. Auxiliar
/// interno ao mtg-db — não faz parte do contrato de `mtg-core`.
fn card_type_bit(t: CardType) -> u16 {
    match t {
        CardType::Artifact => 1 << 0,
        CardType::Battle => 1 << 1,
        CardType::Creature => 1 << 2,
        CardType::Enchantment => 1 << 3,
        CardType::Instant => 1 << 4,
        CardType::Land => 1 << 5,
        CardType::Planeswalker => 1 << 6,
        CardType::Sorcery => 1 << 7,
        CardType::Kindred => 1 << 8,
    }
}

fn type_line_mask(card: &CardDef) -> u16 {
    card.type_line.types.iter().fold(0u16, |acc, t| acc | card_type_bit(*t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::{CardDefId, ManaCost, ManaSymbol, Supertype, TypeLine};

    fn creature(name: &str, cost: Vec<ManaSymbol>, colors: Vec<Color>, power: i32, toughness: i32) -> CardDef {
        let mut type_line = TypeLine::default();
        type_line.types.push(CardType::Creature);
        type_line.subtypes.push("Elf".to_string());
        let color_override = if colors.is_empty() {
            None
        } else {
            Some(colors.into_iter().fold(ColorSet::COLORLESS, |acc, c| acc.union(ColorSet::single(c))))
        };
        CardDef {
            id: CardDefId(0),
            name: name.to_string(),
            mana_cost: ManaCost { symbols: cost },
            type_line,
            color_override,
            power: Some(power),
            toughness: Some(toughness),
            loyalty: None,
            abilities: Vec::new(),
            spell_effect: None,
            spell_targets: Vec::new(),
            oracle_text: format!("{name} entra no campo de batalha."),
            flavor_text: None,
            rarity: Rarity::Common,
            set_code: "TST".to_string(),
            collector_number: "1".to_string(),
            artist: None,
            art_key: None,
        }
    }

    fn land(name: &str) -> CardDef {
        let mut type_line = TypeLine::default();
        type_line.supertypes.push(Supertype::Basic);
        type_line.types.push(CardType::Land);
        type_line.subtypes.push("Forest".to_string());
        CardDef {
            id: CardDefId(1),
            name: name.to_string(),
            mana_cost: ManaCost::FREE,
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
            rarity: Rarity::Common,
            set_code: "TST".to_string(),
            collector_number: "2".to_string(),
            artist: None,
            art_key: None,
        }
    }

    fn sample_db() -> CardDatabase {
        let mut db = CardDatabase {
            cards: vec![
                creature("Elvish Mystic", vec![ManaSymbol::Colored(Color::Green)], vec![Color::Green], 1, 1),
                creature(
                    "Serra Angel",
                    vec![ManaSymbol::Generic(3), ManaSymbol::Colored(Color::White), ManaSymbol::Colored(Color::White)],
                    vec![Color::White],
                    4,
                    4,
                ),
                land("Forest"),
            ],
        };
        db.reindex();
        db
    }

    #[test]
    fn migrate_is_idempotent() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        store.migrate().expect("primeira migração");
        store.migrate().expect("segunda migração não deve falhar");
    }

    #[test]
    fn seed_and_load_round_trip() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let db = sample_db();
        let inserted = store.seed_from(&db).expect("seed");
        assert_eq!(inserted, db.cards.len());

        let loaded = store.load_all().expect("load_all");
        assert_eq!(loaded.cards.len(), db.cards.len());
        assert!(!db.cards.is_empty(), "amostra vazia: o round-trip não compararia nada");
        for original in &db.cards {
            let found = loaded.cards.iter().find(|c| c.name == original.name).expect("carta carregada");
            assert_eq!(found, original, "round-trip deve preservar CardDef igual");
        }
    }

    #[test]
    fn seed_is_idempotent_by_name() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let db = sample_db();
        store.seed_from(&db).expect("primeiro seed");
        let second = store.seed_from(&db).expect("segundo seed (upsert)");
        assert_eq!(second, db.cards.len());
        let loaded = store.load_all().expect("load_all");
        assert_eq!(loaded.cards.len(), db.cards.len(), "upsert não deve duplicar linha");
    }

    #[test]
    fn find_by_name_is_case_insensitive() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        store.seed_from(&sample_db()).expect("seed");
        let found = store.find_by_name("forest").expect("query").expect("Forest deve existir");
        assert_eq!(found.name, "Forest");
        assert!(store.find_by_name("Nao Existe").expect("query").is_none());
    }

    #[test]
    fn search_filters_by_color() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        store.seed_from(&sample_db()).expect("seed");
        let results = store
            .search(&CardQuery { colors: Some(vec![Color::Green]), limit: 10, ..Default::default() })
            .expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Elvish Mystic");
    }

    #[test]
    fn search_filters_by_mana_value_and_types() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        store.seed_from(&sample_db()).expect("seed");

        let cheap = store
            .search(&CardQuery { mana_value_max: Some(1), limit: 10, ..Default::default() })
            .expect("search");
        // Elvish Mystic (CMC 1) e Forest (CMC 0) qualificam; Serra Angel (CMC 5) não.
        assert_eq!(cheap.len(), 2);

        let creatures_only = store
            .search(&CardQuery { types: Some(vec![CardType::Creature]), limit: 10, ..Default::default() })
            .expect("search");
        assert_eq!(creatures_only.len(), 2);
        assert!(creatures_only.iter().all(|c| c.type_line.has_type(CardType::Creature)));
    }

    #[test]
    fn search_text_matches_name_or_oracle_text() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        store.seed_from(&sample_db()).expect("seed");
        let results = store
            .search(&CardQuery { text: Some("Serra".to_string()), limit: 10, ..Default::default() })
            .expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Serra Angel");
    }

    #[test]
    fn search_text_escapes_like_wildcards() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        store.seed_from(&sample_db()).expect("seed");
        // "%" não deve virar coringa e casar tudo.
        let results = store
            .search(&CardQuery { text: Some("%".to_string()), limit: 10, ..Default::default() })
            .expect("search");
        assert!(results.is_empty());
    }

    #[test]
    fn deck_round_trip() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        assert!(store.list_decks().expect("list_decks").is_empty());

        let cards = vec![("Elvish Mystic".to_string(), 4u8), ("Forest".to_string(), 17u8)];
        store.save_deck("Mono Verde", &cards).expect("save_deck");

        let decks = store.list_decks().expect("list_decks");
        assert_eq!(decks, vec!["Mono Verde".to_string()]);

        let loaded = store.load_deck("Mono Verde").expect("load_deck");
        assert_eq!(loaded, cards);

        // Salvar de novo substitui a lista anterior em vez de acumular.
        let smaller = vec![("Forest".to_string(), 10u8)];
        store.save_deck("Mono Verde", &smaller).expect("save_deck (substituição)");
        assert_eq!(store.load_deck("Mono Verde").expect("load_deck"), smaller);
    }

    #[test]
    fn load_deck_missing_returns_error() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let err = store.load_deck("Nao Existe").unwrap_err();
        assert!(matches!(err, DbError::DeckNotFound(_)));
    }

    // -----------------------------------------------------------------------
    // Catálogo amplo (Scryfall)
    // -----------------------------------------------------------------------

    /// Carta sintética de import. `index` gera nome único e ordenável, para os
    /// testes de paginação poderem conferir a ausência de repetição.
    fn imported(index: usize, playable: bool) -> ImportedCard {
        let name = format!("Synthetic {index:06}");
        let mut def = creature(&name, vec![ManaSymbol::Generic(index as u8 % 8)], Vec::new(), 2, 2);
        def.set_code = format!("S{:02}", index % 4);
        ImportedCard {
            oracle_id: format!("oracle-{index:06}"),
            def,
            playable,
            unsupported_reason: if playable { None } else { Some("efeito sem IR".into()) },
            unsupported_pattern: if playable { None } else { Some("whenever ~ dies".into()) },
            image_art_crop: Some(format!("https://img.example/{index}/crop.jpg")),
            image_normal: Some(format!("https://img.example/{index}/normal.jpg")),
            legalities: Some(r#"{"standard":"not_legal"}"#.to_string()),
        }
    }

    fn migrated_columns(store: &CardStore) -> Vec<String> {
        store.column_names("cards").expect("pragma table_info")
    }

    #[test]
    fn migrate_adds_import_columns_and_is_idempotent() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        store.migrate().expect("segunda migração");
        store.migrate().expect("terceira migração");

        let columns = migrated_columns(&store);
        for expected in [
            "oracle_id",
            "playable",
            "unsupported_reason",
            "unsupported_pattern",
            "image_art_crop",
            "image_normal",
            "legalities",
            "imported_at",
        ] {
            assert!(columns.iter().any(|c| c == expected), "coluna ausente: {expected}");
        }
        // Rodar de novo não pode duplicar coluna.
        let unique: std::collections::BTreeSet<&String> = columns.iter().collect();
        assert_eq!(unique.len(), columns.len(), "coluna duplicada após migrar 3x");
    }

    #[test]
    fn migrate_preserves_lua_catalog() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        store.seed_from(&sample_db()).expect("seed");
        store.migrate().expect("migrar de novo sobre dado existente");

        let loaded = store.load_all().expect("load_all");
        assert_eq!(loaded.cards.len(), 3, "migração não pode apagar carta curada");
        // Carta Lua é sempre jogável e não tem oracle_id.
        let page = store
            .search_page(&CardQuery { playable: Some(true), limit: 10, ..Default::default() })
            .expect("search_page");
        assert_eq!(page.total, 3);
        assert!(page.items.iter().all(|c| c.oracle_id.is_none()));
    }

    #[test]
    fn bulk_import_of_ten_thousand_rows_is_fast() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let batch: Vec<ImportedCard> = (0..10_000).map(|i| imported(i, i % 3 != 0)).collect();

        let started = std::time::Instant::now();
        let written = store.import_cards(&batch).expect("import_cards");
        let elapsed = started.elapsed();

        assert_eq!(written, 10_000);
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "inserção em lote levou {elapsed:?}, acima do teto de 5s"
        );

        let stats = store.stats().expect("stats");
        assert_eq!(stats.total, 10_000);
        assert_eq!(stats.playable + stats.unsupported, stats.total);
        assert_eq!(stats.by_set.len(), 4);
        let mut sorted = stats.by_set.clone();
        sorted.sort_by(|a, b| a.set_code.cmp(&b.set_code));
        assert_eq!(sorted, stats.by_set, "by_set tem de sair ordenado por set_code");
    }

    #[test]
    fn import_is_idempotent_by_oracle_id() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let batch: Vec<ImportedCard> = (0..50).map(|i| imported(i, true)).collect();
        store.import_cards(&batch).expect("primeiro import");
        store.import_cards(&batch).expect("segundo import");
        assert_eq!(store.stats().expect("stats").total, 50, "reimportar não pode duplicar");
    }

    #[test]
    fn import_never_overwrites_curated_lua_card() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        store.seed_from(&sample_db()).expect("seed");

        // Mesmo nome de uma carta Lua, porém não jogável e vinda do Scryfall.
        let mut clash = imported(1, false);
        clash.def.name = "Forest".to_string();
        let written = store.import_cards(&[clash]).expect("import_cards");
        assert_eq!(written, 0, "a carta Lua tem de barrar a homônima do Scryfall");

        let forest = store.find_by_name("Forest").expect("query").expect("Forest existe");
        assert_eq!(forest.set_code, "TST", "carta curada foi sobrescrita");
        let page = store
            .search_page(&CardQuery { text: Some("Forest".into()), limit: 10, ..Default::default() })
            .expect("search_page");
        assert_eq!(page.total, 1);
        assert!(page.items[0].playable, "carta curada tem de continuar jogável");
    }

    #[test]
    fn seed_reclaims_row_previously_imported() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let mut incoming = imported(7, false);
        incoming.def.name = "Forest".to_string();
        store.import_cards(&[incoming]).expect("import antes do seed");

        store.seed_from(&sample_db()).expect("seed depois do import");
        let page = store
            .search_page(&CardQuery { text: Some("Forest".into()), limit: 10, ..Default::default() })
            .expect("search_page");
        assert_eq!(page.total, 1);
        assert!(page.items[0].playable, "seed Lua tem de tornar a linha jogável");
        assert!(page.items[0].oracle_id.is_none(), "linha reclamada não é mais do Scryfall");
    }

    #[test]
    fn search_page_reports_total_and_never_repeats_across_pages() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let batch: Vec<ImportedCard> = (0..250).map(|i| imported(i, true)).collect();
        store.import_cards(&batch).expect("import_cards");

        let mut seen: Vec<String> = Vec::new();
        let mut offset = 0usize;
        loop {
            let page = store
                .search_page(&CardQuery { limit: 40, offset, ..Default::default() })
                .expect("search_page");
            assert_eq!(page.total, 250, "total tem de ignorar limit/offset");
            assert_eq!(page.offset, offset);
            if page.items.is_empty() {
                break;
            }
            seen.extend(page.items.iter().map(|c| c.name.clone()));
            offset += 40;
        }

        assert_eq!(seen.len(), 250, "paginação pulou ou repetiu item");
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 250, "item repetido entre páginas");

        // Ordenação estável: a concatenação das páginas já sai ordenada.
        let mut ordered = seen.clone();
        ordered.sort();
        assert_eq!(ordered, seen, "ordenação instável entre páginas");
    }

    #[test]
    fn search_page_filters_by_playable() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let batch: Vec<ImportedCard> = (0..100).map(|i| imported(i, i % 4 == 0)).collect();
        store.import_cards(&batch).expect("import_cards");

        let playable = store
            .search_page(&CardQuery { playable: Some(true), limit: 200, ..Default::default() })
            .expect("search_page");
        assert_eq!(playable.total, 25);
        assert!(playable.items.iter().all(|c| c.playable));
        assert!(playable.items.iter().all(|c| c.unsupported_reason.is_none()));

        let unsupported = store
            .search_page(&CardQuery { playable: Some(false), limit: 200, ..Default::default() })
            .expect("search_page");
        assert_eq!(unsupported.total, 75);
        assert!(unsupported.items.iter().all(|c| !c.playable));
        assert!(unsupported.items.iter().all(|c| c.unsupported_reason.is_some()));

        let all = store
            .search_page(&CardQuery { limit: 200, ..Default::default() })
            .expect("search_page");
        assert_eq!(all.total, 100);
    }

    #[test]
    fn search_page_filters_by_set_and_mana_value() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let batch: Vec<ImportedCard> = (0..40).map(|i| imported(i, true)).collect();
        store.import_cards(&batch).expect("import_cards");

        let by_set = store
            .search_page(&CardQuery { set_code: Some("s01".into()), limit: 100, ..Default::default() })
            .expect("search_page");
        assert_eq!(by_set.total, 10, "filtro de set tem de ignorar caixa");
        assert!(by_set.items.iter().all(|c| c.set_code == "S01"));

        let by_mv = store
            .search_page(&CardQuery { mana_value: Some(3), limit: 100, ..Default::default() })
            .expect("search_page");
        assert!(by_mv.total > 0);
        assert!(by_mv.items.iter().all(|c| c.mana_value == 3));
    }

    #[test]
    fn find_by_oracle_id_round_trips_definition() {
        let store = CardStore::open_in_memory().expect("abrir banco em memória");
        let card = imported(3, false);
        store.import_cards(&[card.clone()]).expect("import_cards");

        let found = store
            .find_by_oracle_id("oracle-000003")
            .expect("query")
            .expect("carta importada tem de ser encontrada");
        assert_eq!(found.summary.name, card.def.name);
        assert_eq!(found.definition, card.def);
        assert!(!found.summary.playable);
        assert_eq!(found.summary.unsupported_pattern.as_deref(), Some("whenever ~ dies"));
        assert!(found.summary.image_normal.is_some());
        assert!(found.imported_at.is_some(), "imported_at tem de ser preenchido");

        assert!(store.find_by_oracle_id("nao-existe").expect("query").is_none());
    }

    #[test]
    fn now_iso8601_has_expected_shape() {
        let stamp = now_iso8601();
        assert_eq!(stamp.len(), 20, "formato ISO-8601 inesperado: {stamp}");
        assert!(stamp.ends_with('Z'));
        let year: i32 = stamp[..4].parse().expect("ano numérico");
        assert!(year >= 2024, "relógio ou conversão civil errada: {stamp}");
    }
}
