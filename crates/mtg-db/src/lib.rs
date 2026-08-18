//! Persistência do catálogo de cartas e de decks.
//!
//! SQLite embutido (rusqlite "bundled") guarda duas cópias de cada carta:
//! colunas soltas (indexáveis, usadas por `search`) e uma coluna `definition`
//! com o `CardDef` inteiro em JSON, que é a fonte da verdade ao carregar —
//! assim um campo novo em `CardDef` nunca é perdido por esquecimento de
//! coluna. O schema evita sintaxe exclusiva do SQLite (além de
//! AUTOINCREMENT) para poder migrar para PostgreSQL depois.
use std::path::Path;

use mtg_core::{CardDatabase, CardDef, CardType, Color, ColorSet, Rarity};
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
    pub limit: usize,
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
        Ok(())
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
                    definition     = excluded.definition
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

    /// Busca cartas por filtros combinados. Todo valor de usuário entra por
    /// placeholder — nenhuma cláusula é montada por concatenação de string.
    pub fn search(&self, q: &CardQuery) -> Result<Vec<CardDef>, DbError> {
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(text) = &q.text {
            clauses.push("(name LIKE ?  ESCAPE '\\' OR oracle_text LIKE ? ESCAPE '\\')".into());
            let pattern = like_pattern(text);
            params.push(Box::new(pattern.clone()));
            params.push(Box::new(pattern));
        }
        if let Some(colors) = &q.colors {
            if !colors.is_empty() {
                let mask = colors.iter().fold(ColorSet::COLORLESS, |acc, c| acc.union(ColorSet::single(*c)));
                clauses.push("(colors_mask & ? ) != 0".into());
                params.push(Box::new(mask.0 as i64));
            }
        }
        if let Some(types) = &q.types {
            if !types.is_empty() {
                let mask: u16 = types.iter().fold(0u16, |acc, t| acc | card_type_bit(*t));
                clauses.push("(types_mask & ?) != 0".into());
                params.push(Box::new(mask as i64));
            }
        }
        if let Some(mv) = q.mana_value_max {
            clauses.push("mana_value <= ?".into());
            params.push(Box::new(mv as i64));
        }

        let mut sql = String::from("SELECT definition FROM cards");
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY name LIMIT ?");
        params.push(Box::new(q.limit as i64));

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

/// Representação textual estável do custo de mana, só para exibição/índice —
/// `definition` (JSON) continua sendo a fonte da verdade estrutural.
fn mana_cost_text(card: &CardDef) -> String {
    card.mana_cost
        .symbols
        .iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>()
        .join(" ")
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
}
