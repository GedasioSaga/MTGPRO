//! `mtg-import` — baixa o catálogo do Scryfall, compila o texto para o IR e grava.
//!
//! Grava na tabela `cards` do MESMO arquivo SQLite que o `mtg-server` lê —
//! o caminho sai de `mtg_db::default_db_path()` (variável `MTG_DB_PATH`, e na
//! falta dela `data/catalog.db`). Não existe banco separado do importador.
//!
//! ```text
//! mtg-import sync --cache .cache/scryfall
//! mtg-import sync --db data/catalog.db --limit 5000
//! mtg-import catalog
//! ```
#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mtg_import::compile::compile_card;
use mtg_import::report::{self, CoverageHeader, CoverageReport};
use mtg_import::scryfall::{self, reject_reason};
use mtg_import::store::{imported_card, ImportStore};
use mtg_import::{ImportError, ImportStats};

const DEFAULT_BULK: &str = "oracle_cards";
/// Duas cartas de nome igual e `oracle_id` diferente existem — as seis
/// versões de "Everythingamajig", as faces-frente de Jumpstart. A tabela
/// `cards` tem `name UNIQUE` (é o que dá paginação estável e busca por nome
/// sem ambiguidade), então a segunda em diante é descartada aqui, contada e
/// dita em voz alta, em vez de sumir dentro de um `INSERT OR IGNORE`.
const DUPLICATE_NAME: &str = "nome repetido no bulk (variante de mesmo nome)";
const DEFAULT_REPORT_LINES: usize = 15;
const DEFAULT_COVERAGE_TOP: usize = 50;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mtg-import: {e}");
            ExitCode::FAILURE
        }
    }
}

struct SyncArgs {
    cache: PathBuf,
    db: PathBuf,
    bulk: String,
    limit: Option<u64>,
    offline: bool,
    report_lines: usize,
    /// Onde gravar o relatório de padrões não suportados. `None` não grava.
    coverage: Option<PathBuf>,
    coverage_top: usize,
}

fn run() -> Result<(), ImportError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("sync") => sync(parse_sync_args(&args[1..])?),
        Some("catalog") => catalog(),
        _ => {
            eprintln!("uso:");
            eprintln!("  mtg-import sync [--cache <dir>] [--db <arquivo.db>] [--bulk oracle_cards]");
            eprintln!("      [--limit N] [--offline] [--coverage <arquivo.md>] [--coverage-top 50]");
            eprintln!("  mtg-import catalog");
            eprintln!();
            eprintln!("  --db  arquivo SQLite do catálogo. É o MESMO que o mtg-server lê.");
            eprintln!("        Padrão: ${{{env}}} se definida, senão {default}.",
                env = mtg_db::DB_PATH_ENV, default = mtg_db::DEFAULT_DB_PATH);
            eprintln!("        As cartas importadas convivem com as curadas em Lua na");
            eprintln!("        tabela `cards`; em colisão de nome, a curada é a que vale.");
            Err(ImportError::Api("comando não reconhecido".into()))
        }
    }
}

fn parse_sync_args(args: &[String]) -> Result<SyncArgs, ImportError> {
    let mut out = SyncArgs {
        cache: PathBuf::from(".cache/scryfall"),
        db: mtg_db::default_db_path(),
        bulk: DEFAULT_BULK.to_string(),
        limit: None,
        offline: false,
        report_lines: DEFAULT_REPORT_LINES,
        coverage: None,
        coverage_top: DEFAULT_COVERAGE_TOP,
    };
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let value = || -> Result<String, ImportError> {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| ImportError::Api(format!("faltou o valor de {flag}")))
        };
        match flag {
            "--cache" => {
                out.cache = PathBuf::from(value()?);
                i += 2;
            }
            "--db" => {
                out.db = PathBuf::from(value()?);
                i += 2;
            }
            "--bulk" => {
                out.bulk = value()?;
                i += 2;
            }
            "--limit" => {
                let raw = value()?;
                out.limit = Some(
                    raw.parse::<u64>()
                        .map_err(|_| ImportError::Api(format!("--limit inválido: {raw}")))?,
                );
                i += 2;
            }
            "--report" => {
                let raw = value()?;
                out.report_lines = raw
                    .parse::<usize>()
                    .map_err(|_| ImportError::Api(format!("--report inválido: {raw}")))?;
                i += 2;
            }
            "--coverage" => {
                out.coverage = Some(PathBuf::from(value()?));
                i += 2;
            }
            "--coverage-top" => {
                let raw = value()?;
                out.coverage_top = raw
                    .parse::<usize>()
                    .map_err(|_| ImportError::Api(format!("--coverage-top inválido: {raw}")))?;
                i += 2;
            }
            "--offline" => {
                out.offline = true;
                i += 1;
            }
            other => return Err(ImportError::Api(format!("opção desconhecida: {other}"))),
        }
    }
    Ok(out)
}

fn catalog() -> Result<(), ImportError> {
    for item in scryfall::fetch_catalog()? {
        println!(
            "{:<16} {:<30} {}",
            item.kind,
            item.updated_at.as_deref().unwrap_or("-"),
            item.best_uri().unwrap_or("-")
        );
    }
    Ok(())
}

fn sync(args: SyncArgs) -> Result<(), ImportError> {
    let (path, updated_at, downloaded) = if args.offline {
        let path = args.cache.join(format!("{}.jsonl.gz", args.bulk));
        if !path.is_file() {
            return Err(ImportError::Api(format!(
                "modo offline mas {} não está em cache",
                path.display()
            )));
        }
        (path, "cache".to_string(), false)
    } else {
        let outcome = scryfall::download_bulk(&args.bulk, &args.cache)?;
        println!(
            "bulk {} de {} ({:.1} MB) — {}",
            args.bulk,
            outcome.updated_at,
            outcome.bytes as f64 / 1_048_576.0,
            if outcome.downloaded { "baixado agora" } else { "cache em dia, sem download" }
        );
        (outcome.path, outcome.updated_at, outcome.downloaded)
    };
    let _ = downloaded;

    let mut store = ImportStore::open(&args.db)?;
    println!("banco do catálogo ..... {} (o mesmo que o mtg-server lê)", args.db.display());
    let mut coverage = CoverageReport::new();
    let started = Instant::now();
    let stats = import_file(&path, &mut store, args.limit, &mut coverage)?;
    let elapsed = started.elapsed();
    store.set_meta("bulk_kind", &args.bulk)?;
    store.set_meta("bulk_updated_at", &updated_at)?;
    store.set_meta("cards_total", &stats.total_lines.to_string())?;
    store.set_meta("cards_playable", &stats.playable.to_string())?;
    // Sem o checkpoint o grosso do dado fica no `-wal` e o tamanho medido do
    // arquivo principal mentiria.
    store.checkpoint()?;

    print_summary(&stats, &store, args.report_lines)?;
    println!("tempo de importação ... {:.1}s", elapsed.as_secs_f64());
    let db_bytes = std::fs::metadata(&args.db).map(|m| m.len()).unwrap_or(0);
    println!("banco ................. {:.1} MB ({})", db_bytes as f64 / 1_048_576.0, args.db.display());

    if let Some(target) = &args.coverage {
        let header = CoverageHeader {
            total_lines: stats.total_lines,
            rejected: stats.total_rejected(),
            playable: stats.playable,
            unplayable: stats.unplayable,
            elapsed_secs: elapsed.as_secs_f64(),
            db_bytes,
        };
        let markdown = report::to_markdown(&coverage, &header, &updated_at, args.coverage_top);
        if let Some(dir) = target.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| ImportError::Io { path: dir.display().to_string(), source: e })?;
            }
        }
        std::fs::write(target, markdown)
            .map_err(|e| ImportError::Io { path: target.display().to_string(), source: e })?;
        println!("relatório de padrões .. {}", target.display());
    }
    Ok(())
}

/// Lê o JSONL, compila e enfileira. A gravação sai em lotes de
/// `store::BATCH_SIZE`, cada lote numa transação — 32 mil `INSERT` soltos
/// levariam minutos só de `fsync`.
fn import_file(
    path: &Path,
    store: &mut ImportStore,
    limit: Option<u64>,
    coverage: &mut CoverageReport,
) -> Result<ImportStats, ImportError> {
    let mut stats = ImportStats::default();
    let stream = scryfall::stream_cards(path)?;
    let mut index = 0u32;
    let mut seen_names: HashSet<String> = HashSet::new();

    for entry in stream {
        stats.total_lines += 1;
        if let Some(max) = limit {
            if stats.total_lines > max {
                stats.total_lines -= 1;
                break;
            }
        }
        let card = match entry {
            Ok(c) => c,
            Err(_) => {
                stats.reject("linha JSON ilegível");
                continue;
            }
        };
        if let Some(reason) = reject_reason(&card) {
            stats.reject(reason.label());
            continue;
        }
        // `reject_reason` já garantiu nome não vazio; `compile_card` usa este
        // mesmo campo como `def.name`, que é a chave única da tabela.
        if !seen_names.insert(card.name.clone().unwrap_or_default()) {
            stats.reject(DUPLICATE_NAME);
            continue;
        }
        let compiled = compile_card(&card, index);
        index = index.saturating_add(1);
        coverage.observe_type_line(&compiled.def.type_line);
        if compiled.playable {
            stats.playable += 1;
        } else {
            stats.note_unplayable(compiled.reason.as_deref().unwrap_or("motivo não registrado"));
            match compiled.pattern.as_deref() {
                Some(text) => coverage.add_text_block(text, &compiled.def.name),
                None => coverage.add_structural_block(),
            }
        }
        store.push(imported_card(&card, compiled))?;
    }
    store.flush()?;
    Ok(stats)
}

fn print_summary(
    stats: &ImportStats,
    store: &ImportStore,
    report_lines: usize,
) -> Result<(), ImportError> {
    let (db_total, db_playable) = store.counts()?;
    let kept = stats.playable + stats.unplayable;
    println!();
    println!("lido do bulk .......... {}", stats.total_lines);
    println!("descartado na entrada . {}", stats.total_rejected());
    for (reason, n) in &stats.rejected {
        println!("    {n:>7}  {reason}");
    }
    println!("no catálogo ........... {kept}");
    println!(
        "    jogável ........... {} ({:.1}%)",
        stats.playable,
        percent(stats.playable, kept)
    );
    println!(
        "    não jogável ....... {} ({:.1}%)",
        stats.unplayable,
        percent(stats.unplayable, kept)
    );
    if report_lines > 0 && !stats.unplayable_reasons.is_empty() {
        println!();
        println!("o que mais impede a compilação:");
        for (reason, n) in stats.top_unplayable(report_lines) {
            println!("    {n:>7}  {reason}");
        }
    }
    println!();
    println!("no banco .............. {db_total} linhas, {db_playable} jogáveis");
    println!("    gravadas agora .... {}", store.written());
    let blocked = kept.saturating_sub(store.written() as u64);
    if blocked > 0 {
        // Homônima de carta curada em Lua: `mtg-db` recusa para não estragar
        // IR escrita à mão. Contar em silêncio esconderia carta ausente.
        println!("    barradas por carta curada de mesmo nome ... {blocked}");
    }
    assert_no_silent_loss(kept, store.written() as u64, blocked);
    Ok(())
}

/// A soma tem de fechar: tudo que foi compilado ou virou linha no banco, ou
/// foi barrado por carta curada homônima. Se sobrar diferença, alguma carta
/// sumiu sem explicação — e carta ausente em silêncio é o bug que este
/// importador existe para não ter.
fn assert_no_silent_loss(kept: u64, written: u64, blocked: u64) {
    if written + blocked != kept {
        eprintln!(
            "aviso: {kept} compiladas, {written} gravadas, {blocked} barradas —              {} cartas sumiram sem motivo registrado",
            kept.saturating_sub(written + blocked)
        );
    }
}

fn percent(part: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    part as f64 * 100.0 / total as f64
}
