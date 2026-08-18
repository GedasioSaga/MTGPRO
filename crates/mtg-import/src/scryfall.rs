//! Acesso ao bulk data do Scryfall: catálogo, download com cache e streaming.
//!
//! O arquivo tem ~23 MB comprimido e centenas de MB cru. Nada aqui carrega o
//! arquivo inteiro na memória: o download vai direto para disco e a leitura é
//! linha a linha, uma desserialização por linha.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::ImportError;

const BULK_ENDPOINT: &str = "https://api.scryfall.com/bulk-data";
/// A API do Scryfall exige User-Agent próprio; cliente genérico é bloqueado.
const USER_AGENT: &str = "MTGPRO/0.1";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// O download é de dezenas de MB — timeout global curto derrubaria a conexão
/// no meio em rede lenta.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(900);

// ---------------------------------------------------------------------------
// Catálogo de bulk
// ---------------------------------------------------------------------------

/// Uma entrada do catálogo `/bulk-data`.
#[derive(Debug, Clone, Deserialize)]
pub struct BulkItem {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub jsonl_download_uri: Option<String>,
    #[serde(default)]
    pub download_uri: Option<String>,
    #[serde(default)]
    pub compressed_size: Option<u64>,
    #[serde(default)]
    pub size: Option<u64>,
}

impl BulkItem {
    /// URI preferida: JSONL quando existe, senão o JSON monolítico. O leitor
    /// de `stream_cards` tolera os dois formatos.
    pub fn best_uri(&self) -> Option<&str> {
        self.jsonl_download_uri.as_deref().or(self.download_uri.as_deref())
    }
}

#[derive(Deserialize)]
struct BulkList {
    #[serde(default)]
    data: Vec<BulkItem>,
}

fn agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .build();
    config.into()
}

/// Lista as entradas de bulk disponíveis, na ordem em que o Scryfall devolve.
pub fn fetch_catalog() -> Result<Vec<BulkItem>, ImportError> {
    let body = get_text(BULK_ENDPOINT)?;
    let list: BulkList = serde_json::from_str(&body)
        .map_err(|e| ImportError::Api(format!("catálogo de bulk ilegível: {e}")))?;
    if list.data.is_empty() {
        return Err(ImportError::Api("catálogo de bulk veio vazio".into()));
    }
    Ok(list.data)
}

fn get_text(url: &str) -> Result<String, ImportError> {
    let mut response = agent(CONNECT_TIMEOUT * 2)
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .call()
        .map_err(|e| ImportError::Http { url: url.to_string(), message: e.to_string() })?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| ImportError::Http { url: url.to_string(), message: e.to_string() })
}

// ---------------------------------------------------------------------------
// Download com cache
// ---------------------------------------------------------------------------

/// O que fica gravado ao lado do arquivo baixado, para saber se ele ainda vale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMeta {
    pub kind: String,
    pub updated_at: String,
    pub uri: String,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DownloadOutcome {
    pub path: PathBuf,
    pub updated_at: String,
    /// `false` quando o cache já estava em dia e nada foi baixado.
    pub downloaded: bool,
    pub bytes: u64,
}

fn data_path(cache_dir: &Path, kind: &str) -> PathBuf {
    cache_dir.join(format!("{kind}.jsonl.gz"))
}

fn meta_path(cache_dir: &Path, kind: &str) -> PathBuf {
    cache_dir.join(format!("{kind}.meta.json"))
}

fn io_err(path: &Path, source: std::io::Error) -> ImportError {
    ImportError::Io { path: path.display().to_string(), source }
}

/// Lê o `.meta.json` do cache. Meta corrompida não é erro fatal: significa
/// apenas "não sei o que tem em disco", e o download acontece de novo.
fn read_meta(cache_dir: &Path, kind: &str) -> Option<CacheMeta> {
    let raw = std::fs::read_to_string(meta_path(cache_dir, kind)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Baixa o bulk `kind` para `cache_dir`, pulando o download quando o
/// `updated_at` em cache já é o do catálogo.
pub fn download_bulk(kind: &str, cache_dir: &Path) -> Result<DownloadOutcome, ImportError> {
    let catalog = fetch_catalog()?;
    let item = catalog
        .into_iter()
        .find(|i| i.kind == kind)
        .ok_or_else(|| ImportError::UnknownBulk(kind.to_string()))?;
    download_item(&item, cache_dir)
}

/// Mesma coisa que `download_bulk`, sem consultar a API de novo quando o
/// chamador já tem o item em mãos.
pub fn download_item(item: &BulkItem, cache_dir: &Path) -> Result<DownloadOutcome, ImportError> {
    let updated_at = item
        .updated_at
        .clone()
        .ok_or_else(|| ImportError::Api(format!("bulk '{}' sem updated_at", item.kind)))?;
    let uri = item
        .best_uri()
        .ok_or_else(|| ImportError::Api(format!("bulk '{}' sem URI de download", item.kind)))?
        .to_string();

    std::fs::create_dir_all(cache_dir).map_err(|e| io_err(cache_dir, e))?;
    let target = data_path(cache_dir, &item.kind);

    if let Some(meta) = read_meta(cache_dir, &item.kind) {
        if meta.updated_at == updated_at && target.is_file() {
            let bytes = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(meta.bytes);
            return Ok(DownloadOutcome { path: target, updated_at, downloaded: false, bytes });
        }
    }

    // Baixa para arquivo temporário e só então renomeia: download interrompido
    // não pode ficar em disco parecendo cache válido.
    let temp = cache_dir.join(format!("{}.part", item.kind));
    let bytes = download_to_file(&uri, &temp)?;
    std::fs::rename(&temp, &target).map_err(|e| io_err(&target, e))?;

    let meta = CacheMeta { kind: item.kind.clone(), updated_at: updated_at.clone(), uri, bytes };
    let meta_json = serde_json::to_string_pretty(&meta)?;
    let mp = meta_path(cache_dir, &item.kind);
    std::fs::write(&mp, meta_json).map_err(|e| io_err(&mp, e))?;

    Ok(DownloadOutcome { path: target, updated_at, downloaded: true, bytes })
}

fn download_to_file(url: &str, dest: &Path) -> Result<u64, ImportError> {
    let mut response = agent(DOWNLOAD_TIMEOUT)
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/x-ndjson")
        .call()
        .map_err(|e| ImportError::Http { url: url.to_string(), message: e.to_string() })?;

    let mut reader = response.body_mut().as_reader();
    let file = File::create(dest).map_err(|e| io_err(dest, e))?;
    let mut writer = std::io::BufWriter::new(file);
    let copied = std::io::copy(&mut reader, &mut writer)
        .map_err(|e| ImportError::Http { url: url.to_string(), message: e.to_string() })?;
    writer.flush().map_err(|e| io_err(dest, e))?;
    Ok(copied)
}

// ---------------------------------------------------------------------------
// Leitura em streaming
// ---------------------------------------------------------------------------

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Iterador sobre as cartas de um JSONL, comprimido ou não.
///
/// A compressão é detectada pelos bytes mágicos, não pela extensão: o cliente
/// HTTP pode ter descomprimido a resposta no caminho, e nesse caso o arquivo
/// com nome `.gz` está cru.
pub struct CardStream {
    reader: Box<dyn BufRead>,
    path: String,
    line_no: u64,
    line: String,
    finished: bool,
}

impl CardStream {
    pub fn open(path: &Path) -> Result<CardStream, ImportError> {
        let mut file = File::open(path).map_err(|e| io_err(path, e))?;
        let mut magic = [0u8; 2];
        let gzipped = match file.read_exact(&mut magic) {
            Ok(()) => magic == GZIP_MAGIC,
            // Arquivo com menos de dois bytes: vazio para todos os efeitos.
            Err(_) => false,
        };
        file.rewind().map_err(|e| io_err(path, e))?;

        let reader: Box<dyn BufRead> = if gzipped {
            Box::new(BufReader::with_capacity(
                1 << 16,
                flate2::read::MultiGzDecoder::new(BufReader::with_capacity(1 << 16, file)),
            ))
        } else {
            Box::new(BufReader::with_capacity(1 << 16, file))
        };

        Ok(CardStream {
            reader,
            path: path.display().to_string(),
            line_no: 0,
            line: String::new(),
            finished: false,
        })
    }

    pub fn line_no(&self) -> u64 {
        self.line_no
    }
}

impl Iterator for CardStream {
    type Item = Result<ScryfallCard, ImportError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.finished {
                return None;
            }
            self.line.clear();
            match self.reader.read_line(&mut self.line) {
                Ok(0) => {
                    self.finished = true;
                    return None;
                }
                Ok(_) => {
                    self.line_no += 1;
                    // Tolera também o bulk em formato de array JSON: as linhas
                    // "[", "]" e a vírgula final não são objetos de carta.
                    let trimmed = self.line.trim().trim_end_matches(',');
                    if trimmed.is_empty() || trimmed == "[" || trimmed == "]" {
                        continue;
                    }
                    let parsed = serde_json::from_str::<ScryfallCard>(trimmed).map_err(|e| {
                        ImportError::Json { path: self.path.clone(), line: self.line_no, source: e }
                    });
                    return Some(parsed);
                }
                Err(e) => {
                    self.finished = true;
                    return Some(Err(ImportError::Io { path: self.path.clone(), source: e }));
                }
            }
        }
    }
}

/// Itera as cartas de um JSONL sem carregar o arquivo na memória.
pub fn stream_cards(path: &Path) -> Result<CardStream, ImportError> {
    CardStream::open(path)
}

// ---------------------------------------------------------------------------
// A carta como o Scryfall entrega
// ---------------------------------------------------------------------------

/// Subconjunto dos campos do Scryfall que este projeto usa.
///
/// Todo campo é opcional de propósito: ficha não tem `oracle_id`, carta de duas
/// faces não tem `oracle_text` na raiz, e o Scryfall acrescenta e remove campo
/// sem aviso. Uma carta esquisita tem que virar contador de descarte, nunca
/// pânico.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScryfallCard {
    #[serde(default)]
    pub oracle_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mana_cost: Option<String>,
    #[serde(default)]
    pub cmc: Option<f64>,
    #[serde(default)]
    pub type_line: Option<String>,
    #[serde(default)]
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub loyalty: Option<String>,
    #[serde(default)]
    pub colors: Option<Vec<String>>,
    #[serde(default)]
    pub color_identity: Option<Vec<String>>,
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
    #[serde(default)]
    pub rarity: Option<String>,
    #[serde(default)]
    pub set: Option<String>,
    #[serde(default)]
    pub collector_number: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub image_uris: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub card_faces: Option<Vec<CardFace>>,
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub legalities: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub digital: Option<bool>,
    #[serde(default)]
    pub games: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CardFace {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mana_cost: Option<String>,
    #[serde(default)]
    pub type_line: Option<String>,
    #[serde(default)]
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub loyalty: Option<String>,
    #[serde(default)]
    pub colors: Option<Vec<String>>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub image_uris: Option<BTreeMap<String, String>>,
}

impl ScryfallCard {
    /// Face de referência para os campos que não existem na raiz de carta
    /// multiface. Não torna a carta jogável — só evita perder metadado.
    pub fn front_face(&self) -> Option<&CardFace> {
        self.card_faces.as_ref().and_then(|f| f.first())
    }

    pub fn effective_mana_cost(&self) -> Option<&str> {
        match self.mana_cost.as_deref() {
            Some(s) if !s.is_empty() => Some(s),
            _ => self.front_face().and_then(|f| f.mana_cost.as_deref()),
        }
    }

    pub fn effective_type_line(&self) -> Option<&str> {
        match self.type_line.as_deref() {
            Some(s) if !s.is_empty() => Some(s),
            _ => self.front_face().and_then(|f| f.type_line.as_deref()),
        }
    }

    pub fn effective_oracle_text(&self) -> &str {
        match self.oracle_text.as_deref() {
            Some(s) => s,
            None => self.front_face().and_then(|f| f.oracle_text.as_deref()).unwrap_or(""),
        }
    }

    pub fn effective_power(&self) -> Option<&str> {
        self.power.as_deref().or_else(|| self.front_face().and_then(|f| f.power.as_deref()))
    }

    pub fn effective_toughness(&self) -> Option<&str> {
        self.toughness.as_deref().or_else(|| self.front_face().and_then(|f| f.toughness.as_deref()))
    }

    pub fn effective_loyalty(&self) -> Option<&str> {
        self.loyalty.as_deref().or_else(|| self.front_face().and_then(|f| f.loyalty.as_deref()))
    }

    pub fn effective_artist(&self) -> Option<&str> {
        self.artist.as_deref().or_else(|| self.front_face().and_then(|f| f.artist.as_deref()))
    }

    /// URL de imagem para a UI. Prefere a raiz; carta de duas faces só tem
    /// imagem por face.
    pub fn image(&self, key: &str) -> Option<&str> {
        if let Some(map) = &self.image_uris {
            if let Some(url) = map.get(key) {
                return Some(url.as_str());
            }
        }
        self.front_face()
            .and_then(|f| f.image_uris.as_ref())
            .and_then(|m| m.get(key))
            .map(|s| s.as_str())
    }

    pub fn is_multiface(&self) -> bool {
        self.card_faces.as_ref().map(|f| f.len() > 1).unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Leitura por face
    //
    // Os `effective_*` acima existem para *metadado*: eles caem na face da
    // frente só para não perder informação. Os `face_*` abaixo existem para
    // *compilação*, e a diferença importa: quando o compilador escolheu ler a
    // face 0 de "Fire // Ice", o custo é `{1}{R}`, nunca `{1}{R} // {1}{U}`
    // da raiz. Por isso aqui não há queda para a raiz nos campos de regra
    // (custo, tipo, texto, P/T, cor) — só nos de apresentação (artista,
    // imagem), onde a raiz é a fonte certa em carta dividida.
    // -----------------------------------------------------------------------

    /// Quantas faces vieram em `card_faces`. Zero quando o campo não veio.
    pub fn face_count(&self) -> usize {
        self.card_faces.as_ref().map(|f| f.len()).unwrap_or(0)
    }

    /// A face de índice `i`, se existir. Índice fora da lista é caso normal —
    /// o Scryfall muda a quantidade de faces sem aviso —, nunca panic.
    pub fn face(&self, i: usize) -> Option<&CardFace> {
        self.card_faces.as_ref().and_then(|f| f.get(i))
    }

    /// Resolve `face` para a `CardFace` correspondente. `None` (raiz) e índice
    /// inexistente dão o mesmo resultado: nenhuma face.
    fn chosen(&self, face: Option<usize>) -> Option<&CardFace> {
        face.and_then(|i| self.face(i))
    }

    /// Nome da face escolhida — é ele que o texto de oráculo usa para falar de
    /// si. Cai no nome da carta inteira quando não há face.
    pub fn face_name(&self, face: Option<usize>) -> &str {
        match self.chosen(face).and_then(|f| f.name.as_deref()) {
            Some(n) if !n.is_empty() => n,
            _ => self.name.as_deref().unwrap_or(""),
        }
    }

    pub fn face_mana_cost(&self, face: Option<usize>) -> Option<&str> {
        match self.chosen(face) {
            Some(f) => f.mana_cost.as_deref(),
            None => self.mana_cost.as_deref(),
        }
    }

    pub fn face_type_line(&self, face: Option<usize>) -> Option<&str> {
        match self.chosen(face) {
            Some(f) => f.type_line.as_deref(),
            None => self.type_line.as_deref(),
        }
    }

    pub fn face_oracle_text(&self, face: Option<usize>) -> &str {
        match self.chosen(face) {
            Some(f) => f.oracle_text.as_deref().unwrap_or(""),
            None => self.oracle_text.as_deref().unwrap_or(""),
        }
    }

    pub fn face_power(&self, face: Option<usize>) -> Option<&str> {
        match self.chosen(face) {
            Some(f) => f.power.as_deref(),
            None => self.power.as_deref(),
        }
    }

    pub fn face_toughness(&self, face: Option<usize>) -> Option<&str> {
        match self.chosen(face) {
            Some(f) => f.toughness.as_deref(),
            None => self.toughness.as_deref(),
        }
    }

    pub fn face_loyalty(&self, face: Option<usize>) -> Option<&str> {
        match self.chosen(face) {
            Some(f) => f.loyalty.as_deref(),
            None => self.loyalty.as_deref(),
        }
    }

    /// Cores impressas da face. Carta de duas faces não traz `colors` na raiz,
    /// e sem esta queda um Delver azul viraria incolor no banco.
    pub fn face_colors(&self, face: Option<usize>) -> Option<&Vec<String>> {
        match self.chosen(face).and_then(|f| f.colors.as_ref()) {
            Some(c) => Some(c),
            None => self.colors.as_ref(),
        }
    }

    /// Artista e imagem são apresentação: carta dividida traz os dois só na
    /// raiz, carta de duas faces só na face. Aqui a queda vale nos dois
    /// sentidos.
    pub fn face_artist(&self, face: Option<usize>) -> Option<&str> {
        self.chosen(face).and_then(|f| f.artist.as_deref()).or(self.artist.as_deref())
    }

    pub fn face_image(&self, face: Option<usize>, key: &str) -> Option<&str> {
        let from_face = self
            .chosen(face)
            .and_then(|f| f.image_uris.as_ref())
            .and_then(|m| m.get(key))
            .map(String::as_str);
        match from_face {
            Some(url) => Some(url),
            None => self.image_uris.as_ref().and_then(|m| m.get(key)).map(String::as_str),
        }
    }
}

// ---------------------------------------------------------------------------
// Filtro de entrada
// ---------------------------------------------------------------------------

/// Por que uma carta nem chega a ser compilada. Cada motivo é contado, porque
/// "descartei 1.900 fichas" é informação e "descartei 1.900 cartas" não é.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    /// Ficha, emblema, série de arte: objeto de jogo que não é carta.
    NonCardLayout,
    /// Carta de formato casual (plano, esquema, vanguard) sem regras no motor.
    UnsupportedFormat,
    /// Só existe em cliente digital (Arena/MTGO); não é carta de papel.
    NotPaper,
    /// Sem nome ou sem linha de tipo: registro inutilizável.
    Malformed,
}

impl Reject {
    pub fn label(self) -> &'static str {
        match self {
            Reject::NonCardLayout => "layout nao-carta (ficha/emblema/art series/front card)",
            Reject::UnsupportedFormat => "formato sem regras no motor (plano/esquema/vanguard)",
            Reject::NotPaper => "nao existe em papel",
            Reject::Malformed => "registro sem nome ou sem linha de tipo",
        }
    }
}

/// Layouts que não são cartas de jogo — nunca entram no catálogo.
///
/// `front_card` é a capa de tema de Jumpstart: linha de tipo literalmente
/// `"Card"`, sem custo e sem texto de regras. Entrava no catálogo e virava a
/// segunda maior causa de "não jogável" (`tipo 'Card'`), escondendo trabalho
/// real atrás de 291 linhas que nunca serão carta.
const NON_CARD_LAYOUTS: [&str; 7] = [
    "token",
    "double_faced_token",
    "emblem",
    "art_series",
    "reversible_card",
    "minigame",
    "front_card",
];

/// Layouts de formatos casuais que o motor não implementa.
const UNSUPPORTED_FORMAT_LAYOUTS: [&str; 6] =
    ["planar", "scheme", "vanguard", "augment", "host", "attraction"];

/// Decide se a carta entra no catálogo. `None` significa aceita.
pub fn reject_reason(card: &ScryfallCard) -> Option<Reject> {
    let layout = card.layout.as_deref().unwrap_or("normal");
    if NON_CARD_LAYOUTS.contains(&layout) {
        return Some(Reject::NonCardLayout);
    }
    if UNSUPPORTED_FORMAT_LAYOUTS.contains(&layout) {
        return Some(Reject::UnsupportedFormat);
    }
    let paper = card.games.as_ref().map(|g| g.iter().any(|x| x == "paper")).unwrap_or(false);
    if !paper {
        return Some(Reject::NotPaper);
    }
    let has_name = card.name.as_deref().map(|n| !n.trim().is_empty()).unwrap_or(false);
    let has_type = card.effective_type_line().map(|t| !t.trim().is_empty()).unwrap_or(false);
    if !has_name || !has_type {
        return Some(Reject::Malformed);
    }
    None
}

#[cfg(test)]
mod tests {
    // `Write` já vem de `use super::*`; o módulo o importa no topo.
    use super::*;

    fn card_json(extra: &str) -> String {
        format!(
            r#"{{"oracle_id":"abc","name":"Shock","mana_cost":"{{R}}","cmc":1.0,
             "type_line":"Instant","oracle_text":"Shock deals 2 damage to any target.",
             "rarity":"common","set":"m21","collector_number":"159",
             "layout":"normal","games":["paper","arena"]{extra}}}"#
        )
        .replace('\n', "")
    }

    #[test]
    fn stream_reads_plain_jsonl_line_by_line() {
        let dir = std::env::temp_dir().join("mtg_import_stream_plain");
        std::fs::create_dir_all(&dir).expect("criar diretório de teste");
        let path = dir.join("cards.jsonl");
        let mut f = File::create(&path).expect("criar arquivo");
        writeln!(f, "{}", card_json("")).expect("escrever");
        writeln!(f).expect("escrever linha vazia");
        writeln!(f, "{}", card_json(r#","artist":"Jim""#)).expect("escrever");
        drop(f);

        let cards: Vec<_> = stream_cards(&path).expect("abrir").collect();
        assert_eq!(cards.len(), 2, "linha vazia não pode virar carta");
        let first = cards[0].as_ref().expect("primeira carta");
        assert_eq!(first.name.as_deref(), Some("Shock"));
        let second = cards[1].as_ref().expect("segunda carta");
        assert_eq!(second.effective_artist(), Some("Jim"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn stream_reads_gzipped_jsonl() {
        let dir = std::env::temp_dir().join("mtg_import_stream_gz");
        std::fs::create_dir_all(&dir).expect("criar diretório de teste");
        let path = dir.join("cards.jsonl.gz");
        let file = File::create(&path).expect("criar arquivo");
        let mut enc = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        writeln!(enc, "{}", card_json("")).expect("escrever");
        enc.finish().expect("fechar gzip");

        let cards: Vec<_> = stream_cards(&path).expect("abrir").collect();
        assert_eq!(cards.len(), 1);
        assert!(cards[0].is_ok(), "linha gzipada deve desserializar");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_fields_never_panic() {
        // Carta de duas faces: sem oracle_text, sem power na raiz.
        let raw = r#"{"name":"Delver of Secrets // Insectile Aberration","layout":"transform",
          "games":["paper"],"card_faces":[{"name":"Delver of Secrets","mana_cost":"{U}",
          "type_line":"Creature — Human Wizard","oracle_text":"At the beginning of your upkeep, look at the top card.",
          "power":"1","toughness":"1"}]}"#;
        let card: ScryfallCard = serde_json::from_str(raw).expect("desserializar");
        assert_eq!(card.effective_power(), Some("1"));
        assert!(card.effective_oracle_text().contains("upkeep"));
        assert_eq!(card.effective_type_line(), Some("Creature — Human Wizard"));
        assert_eq!(card.oracle_id, None);
    }

    #[test]
    fn rejects_are_classified() {
        let token: ScryfallCard =
            serde_json::from_str(r#"{"name":"Soldier","layout":"token","type_line":"Token Creature","games":["paper"]}"#)
                .expect("desserializar");
        assert_eq!(reject_reason(&token), Some(Reject::NonCardLayout));

        let arena_only: ScryfallCard =
            serde_json::from_str(r#"{"name":"Alrund's Epiphany","layout":"normal","type_line":"Sorcery","games":["arena"]}"#)
                .expect("desserializar");
        assert_eq!(reject_reason(&arena_only), Some(Reject::NotPaper));

        let no_type: ScryfallCard =
            serde_json::from_str(r#"{"name":"Coisa","layout":"normal","games":["paper"]}"#)
                .expect("desserializar");
        assert_eq!(reject_reason(&no_type), Some(Reject::Malformed));

        let ok: ScryfallCard = serde_json::from_str(&card_json("")).expect("desserializar");
        assert_eq!(reject_reason(&ok), None);
    }
}
