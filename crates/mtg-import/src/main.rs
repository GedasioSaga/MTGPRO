//! `mtg-import` — baixa o catálogo do Scryfall, compila o texto para o IR e grava.
//!
//! ```text
//! mtg-import sync --cache .cache/scryfall --db catalog.sqlite
//! mtg-import catalog
//! ```
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mtg_import::compile::compile_card;
use mtg_import::report::{self, CoverageHeader, CoverageReport};
use mtg_import::scryfall::{self, reject_reason, ScryfallCard};
use mtg_import::store::{CardRow, ImportStore};
use mtg_import::{ImportError, ImportStats};

const DEFAULT_BULK: &str = "oracle_cards";
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
            eprintln!("  mtg-import sync --cache <dir> --db <arquivo.sqlite> [--bulk oracle_cards]");
            eprintln!("      [--limit N] [--offline] [--coverage <arquivo.md>] [--coverage-top 50]");
            eprintln!("  mtg-import catalog");
            Err(ImportError::Api("comando não reconhecido".into()))
        }
    }
}

fn parse_sync_args(args: &[String]) -> Result<SyncArgs, ImportError> {
    let mut out = SyncArgs {
        cache: PathBuf::from(".cache/scryfall"),
        db: PathBuf::from("catalog.sqlite"),
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

fn import_file(
    path: &Path,
    store: &mut ImportStore,
    limit: Option<u64>,
    coverage: &mut CoverageReport,
) -> Result<ImportStats, ImportError> {
    let mut stats = ImportStats::default();
    let stream = scryfall::stream_cards(path)?;
    let session = store.begin()?;
    let mut index = 0u32;

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
        let definition = if compiled.playable {
            Some(serde_json::to_string(&compiled.def)?)
        } else {
            None
        };
        session.upsert(&row_from(&card, &compiled, definition))?;
    }
    session.commit()?;
    Ok(stats)
}

fn row_from(
    card: &ScryfallCard,
    compiled: &mtg_import::compile::Compiled,
    definition: Option<String>,
) -> CardRow {
    let name = card.name.clone().unwrap_or_default();
    let mana_value = card
        .cmc
        .map(|v| v.round() as i64)
        .unwrap_or_else(|| compiled.def.mana_value() as i64);
    CardRow {
        card_key: card.oracle_id.clone().unwrap_or_else(|| name.clone()),
        oracle_id: card.oracle_id.clone(),
        name,
        mana_cost: card.effective_mana_cost().unwrap_or("").to_string(),
        mana_value,
        type_line: card.effective_type_line().unwrap_or("").to_string(),
        oracle_text: card.effective_oracle_text().to_string(),
        power: card.effective_power().map(str::to_string),
        toughness: card.effective_toughness().map(str::to_string),
        loyalty: card.effective_loyalty().map(str::to_string),
        colors: compiled.colors.clone(),
        color_identity: compiled.color_identity.clone(),
        keywords: card.keywords.clone().unwrap_or_default().join(","),
        rarity: card.rarity.clone().unwrap_or_default(),
        set_code: card.set.clone().unwrap_or_default(),
        collector_number: card.collector_number.clone().unwrap_or_default(),
        artist: card.effective_artist().map(str::to_string),
        image_normal: card.image("normal").map(str::to_string),
        image_art_crop: card.image("art_crop").map(str::to_string),
        layout: card.layout.clone().unwrap_or_else(|| "normal".to_string()),
        legalities: card
            .legalities
            .as_ref()
            .map(|l| serde_json::to_string(l).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string()),
        playable: compiled.playable,
        unplayable_reason: compiled.reason.clone(),
        unplayable_pattern: compiled.pattern.clone(),
        definition,
    }
}

fn print_summary(
    stats: &ImportStats,
    store: &ImportStore,
    report_lines: usize,
) -> Result<(), ImportError> {
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
    println!("no banco .............. {} linhas, {} jogáveis", store.count()?, store.count_playable()?);
    Ok(())
}

fn percent(part: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    part as f64 * 100.0 / total as f64
}
