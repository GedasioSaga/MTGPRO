//! Gravação do catálogo importado — **no mesmo banco que o servidor lê**.
//!
//! Este módulo já teve tabela própria (`scryfall_cards`) em arquivo próprio
//! (`./catalog.sqlite`), enquanto o servidor lia `cards` em `data/catalog.db`.
//! Importava-se 32 mil cartas para um arquivo que ninguém abria, e `/api/stats`
//! seguia respondendo as 174 curadas. Agora existe uma tabela só, `cards` de
//! `mtg-db`, e um caminho só, `mtg_db::default_db_path()`.
//!
//! O que sobrou aqui é a fina camada de adaptação: converter a saída do
//! compilador (`Compiled`) para o registro que `mtg-db` aceita
//! (`ImportedCard`), e agrupar a escrita em lotes transacionais.
use std::path::Path;

use mtg_db::{CardStore, ImportedCard};

use crate::compile::Compiled;
use crate::scryfall::ScryfallCard;
use crate::ImportError;

/// Quantas cartas por transação. Uma transação só para o bulk inteiro seria
/// mais rápida ainda, mas mantém 32 mil registros vivos na memória do SQLite
/// e não deixa nada gravado se a carga cair no meio; 2 mil fecha o commit a
/// cada poucos décimos de segundo e o custo de fsync some no WAL.
pub const BATCH_SIZE: usize = 2_000;

pub struct ImportStore {
    inner: CardStore,
    pending: Vec<ImportedCard>,
    written: usize,
}

impl ImportStore {
    /// Abre (e cria, se preciso) o banco do catálogo, já migrado.
    pub fn open(path: &Path) -> Result<ImportStore, ImportError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ImportError::Io { path: parent.display().to_string(), source: e })?;
            }
        }
        ImportStore::from_store(CardStore::open(path)?)
    }

    pub fn open_in_memory() -> Result<ImportStore, ImportError> {
        ImportStore::from_store(CardStore::open_in_memory()?)
    }

    fn from_store(inner: CardStore) -> Result<ImportStore, ImportError> {
        inner.tune_for_bulk_write()?;
        Ok(ImportStore { inner, pending: Vec::with_capacity(BATCH_SIZE), written: 0 })
    }

    /// Enfileira uma carta; grava sozinho quando o lote enche.
    pub fn push(&mut self, card: ImportedCard) -> Result<(), ImportError> {
        self.pending.push(card);
        if self.pending.len() >= BATCH_SIZE {
            self.flush()?;
        }
        Ok(())
    }

    /// Grava o lote pendente em UMA transação e esvazia a fila.
    pub fn flush(&mut self) -> Result<(), ImportError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.written += self.inner.import_cards(&self.pending)?;
        self.pending.clear();
        Ok(())
    }

    /// Linhas de fato escritas até agora. A diferença para o número de cartas
    /// enfileiradas são as homônimas de carta curada em Lua, que `mtg-db`
    /// recusa de propósito para não estragar a IR escrita à mão.
    pub fn written(&self) -> usize {
        self.written
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), ImportError> {
        self.inner.set_meta(key, value)?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>, ImportError> {
        Ok(self.inner.meta(key)?)
    }

    /// Remove do banco as cartas importadas que esta carga não trouxe. Ver
    /// [`mtg_db::CardStore::prune_imported_except`] — só vale para carga
    /// completa, e nunca toca carta curada em Lua.
    pub fn prune_stale(&self, seen: &[String]) -> Result<usize, ImportError> {
        Ok(self.inner.prune_imported_except(seen)?)
    }

    pub fn checkpoint(&self) -> Result<(), ImportError> {
        self.inner.checkpoint()?;
        Ok(())
    }

    /// Total e jogáveis no banco inteiro — inclui as cartas curadas em Lua,
    /// que compartilham a tabela. É de propósito: é este o número que o
    /// servidor vai reportar em `/api/stats`.
    pub fn counts(&self) -> Result<(u64, u64), ImportError> {
        let stats = self.inner.stats()?;
        Ok((stats.total, stats.playable))
    }
}

/// Converte a saída do compilador no registro que `mtg-db` grava.
///
/// **Carta não compilada entra no catálogo, mas sem habilidade nenhuma.** O
/// metadado (nome, custo, linha de tipo, P/T, raridade, coleção, texto de
/// oráculo, arte) é fiel e serve para busca e navegação; a IR parcial que o
/// compilador conseguiu extrair é descartada. Meia habilidade é pior que
/// habilidade nenhuma: uma carta que faz metade do que o texto promete quebra
/// a partida em silêncio, enquanto uma sem comportamento simplesmente não é
/// escalada — `playable = false` a mantém fora de qualquer deck.
pub fn imported_card(card: &ScryfallCard, compiled: Compiled) -> ImportedCard {
    let Compiled { mut def, playable, reason, pattern, color_identity, colors, second_pass: _ } =
        compiled;
    let _ = colors; // já está em `def` (custo de mana + `color_override`).

    if !playable {
        def.abilities.clear();
        def.spell_effect = None;
        def.spell_targets.clear();
    }

    ImportedCard {
        // `oracle_id` é a chave de identidade da carta no Scryfall. Sem ele a
        // linha não é reimportável de forma idempotente, então o nome serve de
        // último recurso — colide com a carta curada homônima, e nesse caso a
        // curada ganha, que é o comportamento desejado.
        oracle_id: card.oracle_id.clone().unwrap_or_else(|| def.name.clone()),
        def,
        playable,
        unsupported_reason: reason,
        unsupported_pattern: pattern,
        image_art_crop: card.image("art_crop").map(str::to_string),
        image_normal: card.image("normal").map(str::to_string),
        legalities: card
            .legalities
            .as_ref()
            .and_then(|l| serde_json::to_string(l).ok()),
        color_identity: Some(color_identity),
        keywords: card.keywords.as_ref().map(|k| k.join(",")),
        layout: card.layout.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_card;

    /// JSON mínimo de carta do Scryfall. Entrada hostil de propósito: campo
    /// faltando não pode virar panic em lugar nenhum do caminho.
    fn scryfall(name: &str, oracle_id: &str, text: &str) -> ScryfallCard {
        let json = serde_json::json!({
            "oracle_id": oracle_id,
            "name": name,
            "mana_cost": "{1}{G}",
            "cmc": 2.0,
            "type_line": "Creature — Elf Druid",
            "oracle_text": text,
            "power": "1",
            "toughness": "1",
            "colors": ["G"],
            "color_identity": ["G"],
            "keywords": ["Trample"],
            "rarity": "common",
            "set": "tst",
            "collector_number": "1",
            "layout": "normal",
            "legalities": {"standard": "legal"},
        });
        serde_json::from_value(json).expect("carta de teste tem de desserializar")
    }

    #[test]
    fn nao_jogavel_entra_no_catalogo_sem_habilidade() {
        // Texto que o compilador não sabe traduzir.
        let card = scryfall(
            "Coisa Estranha",
            "oracle-1",
            "Whenever a player casts a spell with a converted mana cost equal to \
             the number of cards in your graveyard, scry 7 and then rearrange fate.",
        );
        let record = imported_card(&card, compile_card(&card, 0));

        assert!(!record.playable, "texto sem IR não pode virar carta jogável");
        assert!(record.unsupported_reason.is_some(), "motivo tem de ser preenchido");
        assert!(record.def.abilities.is_empty(), "IR parcial não pode ficar na carta");
        assert!(record.def.spell_effect.is_none());
        // ... mas o metadado continua fiel, que é o que faz a carta navegável.
        assert_eq!(record.def.name, "Coisa Estranha");
        assert_eq!(record.def.set_code, "tst");
        assert_eq!(record.def.power, Some(1));
        assert_eq!(record.def.mana_value(), 2);
        assert!(!record.def.oracle_text.is_empty());
        assert_eq!(record.color_identity.as_deref(), Some("G"));
        assert_eq!(record.keywords.as_deref(), Some("Trample"));
        assert_eq!(record.layout.as_deref(), Some("normal"));
    }

    #[test]
    fn lote_grava_no_banco_do_servidor() {
        let mut store = ImportStore::open_in_memory().expect("abrir");
        for i in 0..(BATCH_SIZE + 5) {
            let card = scryfall(&format!("Carta {i:05}"), &format!("oracle-{i:05}"), "Trample");
            store.push(imported_card(&card, compile_card(&card, i as u32))).expect("push");
        }
        // O lote cheio já foi gravado sozinho; o resto sai no flush.
        assert_eq!(store.written(), BATCH_SIZE, "lote cheio tem de gravar sozinho");
        store.flush().expect("flush");
        assert_eq!(store.written(), BATCH_SIZE + 5);

        let (total, _playable) = store.counts().expect("contar");
        assert_eq!(total, BATCH_SIZE as u64 + 5);
    }

    #[test]
    fn reimportar_nao_duplica() {
        let mut store = ImportStore::open_in_memory().expect("abrir");
        for _ in 0..2 {
            for i in 0..10 {
                let card = scryfall(&format!("Carta {i}"), &format!("oracle-{i}"), "Trample");
                store.push(imported_card(&card, compile_card(&card, i))).expect("push");
            }
            store.flush().expect("flush");
        }
        assert_eq!(store.counts().expect("contar").0, 10, "oracle_id repetido é update");
    }

    #[test]
    fn meta_round_trip() {
        let store = ImportStore::open_in_memory().expect("abrir");
        assert_eq!(store.meta("bulk_updated_at").expect("ler"), None);
        store.set_meta("bulk_updated_at", "2026-08-18").expect("gravar");
        assert_eq!(
            store.meta("bulk_updated_at").expect("ler"),
            Some("2026-08-18".to_string())
        );
    }
}
