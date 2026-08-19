//! Trava de cobertura: o número de cartas jogáveis não pode cair sem alguém ver.
//!
//! O problema que este teste resolve: a cobertura é um número agregado que só
//! aparece depois de importar 32 mil cartas do Scryfall. Uma mudança no parser
//! que quebre 300 cartas não quebra teste nenhum — todos os testes de unidade
//! continuam verdes, porque cada um prova uma carta que continua funcionando.
//! A queda só apareceria na próxima importação, e só se alguém comparasse o
//! relatório antigo com o novo, de cabeça.
//!
//! Então aqui roda o compilador de verdade — o mesmo `compile_card` que o
//! `mtg-import sync` usa — sobre um conjunto FIXO de cartas reais, versionado
//! no repositório, e compara com números gravados. Sem rede, sem banco.
//!
//! # Por que a igualdade é exata, e não "não pode cair"
//!
//! Uma trava do tipo `>=` erode: a cobertura sobe, ninguém atualiza o número
//! gravado, e meses depois uma regressão até o valor antigo passa despercebida
//! porque ainda satisfaz o `>=`. Com igualdade exata, toda mudança de
//! cobertura — para cima ou para baixo — aparece como diff neste arquivo, na
//! revisão, com o sinal explícito. Subir de propósito custa um comando; cair
//! sem querer custa um teste vermelho.
//!
//! # Como atualizar quando a cobertura sobe de propósito
//!
//! ```text
//! UPDATE_COVERAGE_BASELINE=1 cargo test -p mtg-import --test coverage_regression
//! ```
//!
//! Isso reescreve `crates/mtg-oracle/tests/fixtures/coverage_baseline.txt`.
//! **Confira o diff antes de commitar**: se um número caiu, a mudança quebrou
//! carta que antes compilava, e o commit certo é o que conserta, não o que
//! grava a perda.
//!
//! # O conjunto fixo
//!
//! 500 cartas reais do bulk `oracle_cards`, escolhidas por passada uniforme
//! sobre o catálogo elegível ordenado por nome. Uniforme, e não escolhidas a
//! dedo, para a amostra ter a mesma mistura do catálogo — inclusive as cartas
//! que não compilam, que são a maioria e são o que se está tentando destravar.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mtg_import::compile::compile_card;
use mtg_import::scryfall::{reject_reason, ScryfallCard};
use mtg_oracle::coverage::{self, Pool};

/// Quantas cartas o conjunto fixo tem. Se este número mudar, o arquivo foi
/// mexido — e aí a comparação com o baseline não significaria mais nada.
const FIXTURE_CARDS: usize = 500;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../mtg-oracle/tests/fixtures")
}

/// Números medidos sobre o conjunto fixo, na ordem de [`Pool::ALL`].
#[derive(Debug, Default, PartialEq, Eq)]
struct Measured {
    cards: u64,
    /// Por slug de pool: (no conjunto, jogáveis).
    pools: BTreeMap<String, (u64, u64)>,
}

impl Measured {
    /// Formato de uma chave por linha, ordem fixa, para o diff do arquivo
    /// mostrar exatamente qual pool mexeu.
    fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("# Cobertura gravada sobre o conjunto fixo de cartas reais.\n");
        out.push_str("# Gerado por: UPDATE_COVERAGE_BASELINE=1 cargo test -p mtg-import \\\n");
        out.push_str("#             --test coverage_regression\n");
        out.push_str("# NAO editar a mao. Numero que cai = carta que parou de compilar.\n");
        out.push_str(&format!("cards={}\n", self.cards));
        for pool in Pool::ALL {
            let (total, playable) = self.pools.get(pool.slug()).copied().unwrap_or((0, 0));
            out.push_str(&format!("{}.total={}\n", pool.slug(), total));
            out.push_str(&format!("{}.playable={}\n", pool.slug(), playable));
        }
        out
    }

    fn from_text(text: &str) -> Measured {
        let mut m = Measured::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let Ok(n) = value.trim().parse::<u64>() else { continue };
            match key.trim() {
                "cards" => m.cards = n,
                other => {
                    if let Some(slug) = other.strip_suffix(".total") {
                        m.pools.entry(slug.to_string()).or_insert((0, 0)).0 = n;
                    } else if let Some(slug) = other.strip_suffix(".playable") {
                        m.pools.entry(slug.to_string()).or_insert((0, 0)).1 = n;
                    }
                }
            }
        }
        m
    }
}

/// Roda o compilador de verdade sobre o conjunto fixo.
fn measure() -> Measured {
    let path = fixtures_dir().join("coverage_sample.jsonl");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("conjunto fixo ilegível em {}: {e}", path.display()));

    let mut m = Measured::default();
    for pool in Pool::ALL {
        m.pools.insert(pool.slug().to_string(), (0, 0));
    }

    for (i, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let card: ScryfallCard = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("linha {} do conjunto fixo não é ScryfallCard: {e}", i + 1));
        // O conjunto foi filtrado na geração. Se uma carta passa a ser
        // recusada na entrada, o denominador mudou e comparar jogáveis com o
        // baseline seria comparar coisas diferentes.
        assert!(
            reject_reason(&card).is_none(),
            "linha {}: carta do conjunto fixo passou a ser recusada na entrada",
            i + 1
        );

        let compiled = compile_card(&card, i as u32);
        let rarity = card.rarity.as_deref().unwrap_or_default();
        let mask = coverage::pools_of(rarity, |format| {
            card.legalities.as_ref().and_then(|l| l.get(format)).map(String::as_str)
        });

        m.cards += 1;
        for pool in Pool::ALL {
            if mask.contains(pool) {
                let entry = m.pools.entry(pool.slug().to_string()).or_insert((0, 0));
                entry.0 += 1;
                if compiled.playable {
                    entry.1 += 1;
                }
            }
        }
    }
    m
}

#[test]
fn fixture_is_intact() {
    let m = measure();
    assert_eq!(
        m.cards as usize, FIXTURE_CARDS,
        "o conjunto fixo tem de ter {FIXTURE_CARDS} cartas; mexer nele invalida o baseline"
    );
    // Sem esta linha o teste passaria com um arquivo de 500 cartas vazias.
    let catalog = m.pools.get(Pool::Catalog.slug()).copied().unwrap_or((0, 0));
    assert_eq!(catalog.0 as usize, FIXTURE_CARDS, "toda carta conta no catálogo");
    assert!(catalog.1 > 0, "nenhuma carta do conjunto compila: o teste não estaria medindo nada");
}

#[test]
fn format_pools_are_populated() {
    // A cobertura por formato depende de `legalities` estar no conjunto fixo.
    // Se o campo sumir, os pools zeram e a trava dos formatos viraria uma
    // comparação de 0 com 0 — verde para sempre, medindo nada.
    let m = measure();
    for pool in Pool::FORMATS {
        let (total, _) = m.pools.get(pool.slug()).copied().unwrap_or((0, 0));
        assert!(total > 0, "pool {} vazio: `legalities` sumiu do conjunto fixo", pool.slug());
    }
}

#[test]
fn coverage_has_not_regressed() {
    let measured = measure();
    let path = fixtures_dir().join("coverage_baseline.txt");

    if std::env::var("UPDATE_COVERAGE_BASELINE").is_ok() {
        std::fs::write(&path, measured.to_text())
            .unwrap_or_else(|e| panic!("não deu para gravar {}: {e}", path.display()));
        eprintln!("baseline regravado em {} — confira o diff", path.display());
        return;
    }

    let recorded = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "baseline ausente em {} ({e}).\n\
             Gere com: UPDATE_COVERAGE_BASELINE=1 cargo test -p mtg-import \
             --test coverage_regression",
            path.display()
        )
    });
    let recorded = Measured::from_text(&recorded);

    if measured != recorded {
        let mut diff = String::new();
        for pool in Pool::ALL {
            let now = measured.pools.get(pool.slug()).copied().unwrap_or((0, 0));
            let was = recorded.pools.get(pool.slug()).copied().unwrap_or((0, 0));
            if now != was {
                let sinal = if now.1 < was.1 { "QUEDA" } else { "subiu" };
                diff.push_str(&format!(
                    "  {:<16} jogáveis {} -> {} ({sinal}), no conjunto {} -> {}\n",
                    pool.slug(),
                    was.1,
                    now.1,
                    was.0,
                    now.0
                ));
            }
        }
        panic!(
            "a cobertura sobre o conjunto fixo mudou:\n{diff}\n\
             Se CAIU, a mudança quebrou carta que antes compilava — conserte, não grave a perda.\n\
             Se subiu de propósito, regrave com:\n  \
             UPDATE_COVERAGE_BASELINE=1 cargo test -p mtg-import --test coverage_regression"
        );
    }
}
