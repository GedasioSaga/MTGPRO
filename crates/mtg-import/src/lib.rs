//! Importador do catálogo do Scryfall.
//!
//! Três etapas independentes, cada uma testável sozinha:
//!
//! 1. `scryfall` — rede e disco: catálogo de bulk, download com cache por
//!    `updated_at`, leitura em streaming do JSONL.
//! 2. `parse` + `compile` — texto para IR: custo de mana, linha de tipo e
//!    texto de oráculo viram `CardDef`.
//! 3. `store` — grava no SQLite, na tabela `cards` de `mtg-db`, que é a
//!    mesma que o servidor lê. Não há tabela nem arquivo próprio do
//!    importador: uma fonte de verdade, não duas.
//!
//! O ponto que governa o desenho todo: o Scryfall entrega **texto**, o motor
//! roda sobre **IR**. Importar não é traduzir tudo — é traduzir o que dá para
//! traduzir com fidelidade e marcar o resto como `playable = false`, que
//! continua no catálogo (buscável, navegável) mas não entra em deck.
#![forbid(unsafe_code)]

pub mod compile;
pub mod parse;
pub mod report;
pub mod scryfall;
pub mod store;

use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("erro de rede em {url}: {message}")]
    Http { url: String, message: String },
    #[error("erro de I/O em {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("JSON inválido em {path}, linha {line}: {source}")]
    Json {
        path: String,
        line: u64,
        #[source]
        source: serde_json::Error,
    },
    #[error("resposta inesperada da API do Scryfall: {0}")]
    Api(String),
    #[error("bulk '{0}' não existe no catálogo do Scryfall")]
    UnknownBulk(String),
    #[error("erro no banco do catálogo: {0}")]
    Db(#[from] mtg_db::DbError),
    #[error("erro de serialização JSON: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Contadores do que entrou e do que ficou de fora. Chave ordenada porque a
/// saída é comparada entre execuções — `HashMap` daria ordem diferente a cada
/// rodada e estragaria o diff.
#[derive(Debug, Clone, Default)]
pub struct ImportStats {
    pub total_lines: u64,
    pub rejected: BTreeMap<&'static str, u64>,
    pub playable: u64,
    /// Quantas das `playable` vieram da segunda passada (`mtg_oracle::compile`)
    /// em vez do compilador deste crate. Dois compiladores convivem aqui; sem
    /// este número não dá para saber qual dos dois está pagando a cobertura.
    pub playable_second_pass: u64,
    pub unplayable: u64,
    /// Motivo pelo qual a carta não é jogável, do mais frequente ao menos.
    pub unplayable_reasons: BTreeMap<String, u64>,
}

impl ImportStats {
    pub fn reject(&mut self, reason: &'static str) {
        *self.rejected.entry(reason).or_insert(0) += 1;
    }
    pub fn note_unplayable(&mut self, reason: &str) {
        self.unplayable += 1;
        *self.unplayable_reasons.entry(reason.to_string()).or_insert(0) += 1;
    }
    pub fn total_rejected(&self) -> u64 {
        self.rejected.values().sum()
    }
    /// Motivos de não-jogável ordenados por frequência decrescente; empate
    /// desempata por nome, para a saída ser idêntica entre execuções.
    pub fn top_unplayable(&self, limit: usize) -> Vec<(&str, u64)> {
        let mut v: Vec<(&str, u64)> =
            self.unplayable_reasons.iter().map(|(k, n)| (k.as_str(), *n)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        v.truncate(limit);
        v
    }
}
