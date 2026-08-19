//! Relatório de cobertura do oráculo.
//!
//! Saber que 24 mil cartas não compilam não diz o que fazer em seguida. Saber
//! que uma única construção — "when ~ enters the battlefield, create a token" —
//! trava centenas delas, sim. Este módulo agrega os textos que não compilaram
//! num punhado de **padrões normalizados**, ordenados por quantas cartas cada
//! um destrava.
//!
//! A normalização é o miolo: sem ela cada carta viraria o próprio bucket, e o
//! relatório teria dezenas de milhares de linhas de contagem 1. Números viram
//! `N`, subtipos de criatura viram `<tipo>`, cores viram `<cor>` — o que sobra
//! é a **forma** da frase, que é o que se implementa uma vez e vale para todas.
use std::collections::BTreeMap;

use mtg_core::types::{CardType, TypeLine};
use mtg_oracle::coverage::{self, Capability, Gap, Pool, PoolMask};

/// Quantos caracteres de padrão cabem numa linha de tabela sem estourar.
/// Texto mais longo é cortado em fronteira de palavra: dois textos que só
/// diferem depois disso são, para efeito de esforço, o mesmo trabalho.
const MAX_PATTERN_LEN: usize = 150;

/// Quantas palavras formam o "começo de frase" da segunda tabela.
const PREFIX_WORDS: usize = 6;

/// Em quantas cartas um subtipo precisa aparecer para entrar no vocabulário.
///
/// Linha de tipo de carta-piada é ruído com forma de subtipo: `B.F.M.` é
/// "Creature — The Biggest, Baddest, Nastiest,", e sem limiar a palavra "the"
/// virava subtipo e destruía todo padrão que dissesse "the top card". Subtipo
/// de verdade aparece em dezenas de cartas; ruído aparece em uma.
const MIN_SUBTYPE_CARDS: u32 = 3;

/// Subtipos que sobrevivem ao limiar mas não devem virar `<tipo>`.
///
/// Duas famílias: palavra comum de texto de oráculo ("time counter", "the
/// first time"), e tipo de ficha de artefato que aparece de carona em linha de
/// criatura ("Artifact Creature — Food Golem"). Juntar Food com Clue num
/// `<tipo>` esconderia trabalho diferente atrás do mesmo número.
const SUBTYPE_STOPLIST: [&str; 15] = [
    "eye",
    "egg",
    "mount",
    "spy",
    "time",
    "saga",
    "food",
    "clue",
    "treasure",
    "blood",
    "gold",
    "map",
    "powerstone",
    "incubator",
    "junk",
];

const COLOR_WORDS: [&str; 5] = ["white", "blue", "black", "red", "green"];

const BASIC_LAND_WORDS: [&str; 5] = ["plains", "island", "swamp", "mountain", "forest"];

/// "two".."ten". `one` fica de fora de propósito: em texto de oráculo a
/// contagem 1 é escrita "a"/"an", e "one" quase sempre é "choose one" ou
/// "one of them" — trocá-lo por `N` inventaria padrão.
const WORD_NUMBERS: [&str; 9] =
    ["two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];

/// Vocabulário de subtipos de criatura, colhido do próprio bulk em vez de
/// escrito à mão: lista fixa envelhece a cada coleção nova.
#[derive(Debug, Clone, Default)]
pub struct Vocabulary {
    /// Subtipo para em quantas cartas ele foi visto.
    creature_subtypes: BTreeMap<String, u32>,
}

impl Vocabulary {
    /// Só linha de tipo de criatura contribui. Subtipo de artefato
    /// (`Treasure`, `Clue`) e de encantamento (`Aura`, `Saga`) fica literal,
    /// porque implementar Treasure não é implementar Clue — juntá-los num
    /// `<tipo>` esconderia trabalho diferente atrás do mesmo número.
    pub fn observe(&mut self, type_line: &TypeLine) {
        if !type_line.types.contains(&CardType::Creature) {
            return;
        }
        for sub in &type_line.subtypes {
            let word = sub.trim().to_lowercase();
            let wordlike = word.chars().all(|c| c.is_ascii_alphabetic() || c == '\'');
            if word.len() >= 3 && wordlike {
                *self.creature_subtypes.entry(word).or_insert(0) += 1;
            }
        }
    }

    /// Só os subtipos que passaram do limiar — é o vocabulário que a
    /// normalização de fato usa.
    pub fn len(&self) -> usize {
        self.creature_subtypes.values().filter(|n| **n >= MIN_SUBTYPE_CARDS).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn known(&self, word: &str) -> bool {
        self.creature_subtypes.get(word).is_some_and(|n| *n >= MIN_SUBTYPE_CARDS)
    }

    /// Reconhece a palavra como subtipo, tolerando plural regular e o plural
    /// em `-ves` ("elves", "wolves", "dwarves").
    fn is_creature_subtype(&self, word: &str) -> bool {
        if word.len() < 3 || SUBTYPE_STOPLIST.contains(&word) {
            return false;
        }
        if self.known(word) {
            return true;
        }
        singulars(word).into_iter().any(|stem| {
            stem.len() >= 3 && !SUBTYPE_STOPLIST.contains(&stem.as_str()) && self.known(&stem)
        })
    }
}

/// Candidatos a singular de uma palavra no plural, do mais provável ao menos.
fn singulars(word: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(stem) = word.strip_suffix("ves") {
        out.push(format!("{stem}f"));
        out.push(format!("{stem}fe"));
    }
    if let Some(stem) = word.strip_suffix("es") {
        out.push(stem.to_string());
    }
    if let Some(stem) = word.strip_suffix('s') {
        out.push(stem.to_string());
    }
    out
}

/// Reduz um texto de oráculo à sua forma. A entrada já vem em minúsculas e com
/// o nome próprio trocado por `~` pelo compilador.
pub fn normalize(snippet: &str, vocab: &Vocabulary) -> String {
    let mut out = String::with_capacity(snippet.len());
    let chars: Vec<char> = snippet.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            // Toda corrida de dígito vira um só `N`: "2/2" e "10/10" são a
            // mesma ficha para quem vai implementar.
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            out.push('N');
            continue;
        }
        if c.is_alphabetic() || c == '\'' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '\'') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            out.push_str(&replace_word(&word, vocab));
            continue;
        }
        out.push(c);
        i += 1;
    }

    truncate_on_word(&collapse_spaces(&out), MAX_PATTERN_LEN)
}

fn replace_word(word: &str, vocab: &Vocabulary) -> String {
    let lower = word.to_lowercase();
    if WORD_NUMBERS.contains(&lower.as_str()) {
        return "N".to_string();
    }
    if COLOR_WORDS.contains(&lower.as_str()) {
        return "<cor>".to_string();
    }
    let basic = BASIC_LAND_WORDS.contains(&lower.as_str())
        || singulars(&lower).iter().any(|s| BASIC_LAND_WORDS.contains(&s.as_str()));
    if basic {
        return "<terreno>".to_string();
    }
    if vocab.is_creature_subtype(&lower) {
        return "<tipo>".to_string();
    }
    word.to_string()
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_on_word(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    let cut = head.rfind(' ').unwrap_or(head.len());
    format!("{}…", head[..cut].trim_end())
}

// ---------------------------------------------------------------------------
// Agregação
// ---------------------------------------------------------------------------

/// Uma linha do relatório.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternRow {
    pub cards: u64,
    pub pattern: String,
    pub example: String,
}

/// Quantos pools existem. Fixo, porque a ordem das colunas é a de
/// [`Pool::ALL`] e não pode depender de iteração de mapa.
const POOL_COUNT: usize = Pool::ALL.len();

#[derive(Debug, Clone, Default)]
struct Tally {
    cards: u64,
    /// Menor nome em ordem alfabética entre as cartas do padrão. Escolha
    /// arbitrária mas **estável**: o exemplo não muda quando o Scryfall
    /// reordena o bulk, então o diff do relatório mostra só o que mudou.
    example: String,
    /// Quantas das cartas do balde participam de cada pool, na ordem de
    /// [`Pool::ALL`]. É o que separa "destrava 200 cartas de Un-set" de
    /// "destrava 80 de Pauper".
    pools: [u64; POOL_COUNT],
}

impl Tally {
    fn add(&mut self, name: &str, mask: PoolMask) {
        self.cards += 1;
        if self.example.is_empty() || name < self.example.as_str() {
            self.example = name.to_string();
        }
        for (slot, pool) in self.pools.iter_mut().zip(Pool::ALL) {
            if mask.contains(pool) {
                *slot += 1;
            }
        }
    }

    fn pool(&self, pool: Pool) -> u64 {
        Pool::ALL.iter().position(|p| *p == pool).map_or(0, |i| self.pools[i])
    }
}

/// Uma carta que não compilou, com o pool a que pertence.
#[derive(Debug, Clone)]
struct Blocked {
    /// O que classifica esta carta. Em `pending` é o trecho de oráculo que
    /// travou; em `no_snippet` é o **motivo** que o compilador registrou
    /// (`"substantivo 'enchanted'"`), porque ali não houve trecho nenhum.
    text: String,
    name: String,
    mask: PoolMask,
}

/// Quantas cartas cada pool tem, e quantas dessas são jogáveis.
#[derive(Debug, Clone, Default)]
struct PoolTally {
    seen: [u64; POOL_COUNT],
    playable: [u64; POOL_COUNT],
}

impl PoolTally {
    fn add(&mut self, mask: PoolMask, playable: bool) {
        for (i, pool) in Pool::ALL.iter().enumerate() {
            if mask.contains(*pool) {
                self.seen[i] += 1;
                if playable {
                    self.playable[i] += 1;
                }
            }
        }
    }
}

/// Uma linha da tabela de cobertura por pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolRow {
    pub pool: Pool,
    pub in_catalog: u64,
    pub playable: u64,
}

impl PoolRow {
    pub fn percent(&self) -> f64 {
        percent(self.playable, self.in_catalog)
    }
}

/// Uma linha da tabela de capacidades: um pedaço de trabalho de verdade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRow {
    pub capability: &'static Capability,
    pub cards: u64,
    pub pauper: u64,
    pub standard: u64,
    pub modern: u64,
    pub example: String,
}

/// Junta os textos que não compilaram e devolve os padrões mais frequentes.
///
/// Guarda os trechos crus e só normaliza no fim porque o vocabulário de
/// subtipos só fica completo depois de ler o bulk inteiro. São dezenas de
/// milhares de strings curtas, não o arquivo — a leitura continua em
/// streaming.
#[derive(Debug, Clone, Default)]
pub struct CoverageReport {
    vocab: Vocabulary,
    /// Bloqueio textual: o compilador leu o texto e não entendeu.
    pending: Vec<Blocked>,
    /// Bloqueio que não deixou trecho de texto: o compilador falhou fundo
    /// dentro do parser e registrou só um motivo. NÃO é sinônimo de "layout" —
    /// medindo o bulk, das cartas sem trecho só ~12% são de mais de uma face;
    /// o resto é vocabulário faltando no parser.
    no_snippet: Vec<Blocked>,
    pools: PoolTally,
    /// Quantas cartas trouxeram o campo `legalities`. Zero significa que as
    /// colunas de formato mentiriam, e o relatório diz isso em vez de mostrar
    /// 0% como se fosse medida.
    with_legalities: u64,
    cards_seen: u64,
}

impl CoverageReport {
    pub fn new() -> CoverageReport {
        CoverageReport::default()
    }

    /// Registra uma carta do catálogo: alimenta o vocabulário de subtipos e a
    /// contagem por pool. Tem de ser chamada para **toda** carta que entra no
    /// catálogo, jogável ou não — é ela que forma o denominador.
    pub fn observe_card(
        &mut self,
        type_line: &TypeLine,
        mask: PoolMask,
        playable: bool,
        has_legalities: bool,
    ) {
        self.vocab.observe(type_line);
        self.pools.add(mask, playable);
        self.cards_seen += 1;
        if has_legalities {
            self.with_legalities += 1;
        }
    }

    pub fn add_text_block(&mut self, snippet: &str, card_name: &str, mask: PoolMask) {
        self.pending.push(Blocked {
            text: snippet.to_string(),
            name: card_name.to_string(),
            mask,
        });
    }

    /// Registra uma carta que travou sem deixar trecho de texto. `reason` é o
    /// motivo cru do compilador — é ele que diz se o bloqueio é de layout ou
    /// de vocabulário, e sem ele os dois viram o mesmo número.
    pub fn add_blocked_without_snippet(&mut self, reason: &str, card_name: &str, mask: PoolMask) {
        self.no_snippet.push(Blocked {
            text: reason.to_string(),
            name: card_name.to_string(),
            mask,
        });
    }

    pub fn text_blocked(&self) -> u64 {
        self.pending.len() as u64
    }

    pub fn blocked_without_snippet(&self) -> u64 {
        self.no_snippet.len() as u64
    }

    pub fn vocabulary_size(&self) -> usize {
        self.vocab.len()
    }

    pub fn cards_seen(&self) -> u64 {
        self.cards_seen
    }

    /// `true` quando pelo menos uma carta trouxe `legalities`. Falso significa
    /// que a tabela por formato não pode ser publicada como medida.
    pub fn has_legalities(&self) -> bool {
        self.with_legalities > 0
    }

    pub fn cards_with_legalities(&self) -> u64 {
        self.with_legalities
    }

    /// Cobertura por pool, na ordem fixa de [`Pool::ALL`].
    pub fn pool_rows(&self) -> Vec<PoolRow> {
        Pool::ALL
            .iter()
            .enumerate()
            .map(|(i, pool)| PoolRow {
                pool: *pool,
                in_catalog: self.pools.seen[i],
                playable: self.pools.playable[i],
            })
            .collect()
    }

    /// Agrupa os textos por uma chave derivada do padrão normalizado.
    fn tally_by(&self, key_of: impl Fn(&str) -> String) -> BTreeMap<String, Tally> {
        let mut buckets: BTreeMap<String, Tally> = BTreeMap::new();
        for blocked in &self.pending {
            let key = key_of(&normalize(&blocked.text, &self.vocab));
            if key.is_empty() {
                continue;
            }
            buckets.entry(key).or_default().add(&blocked.name, blocked.mask);
        }
        buckets
    }

    /// Agrupa os travados por **capacidade faltante** — a tabela que responde
    /// "o que implementar agora".
    ///
    /// Agrupar por frase literal produz milhares de linhas de contagem
    /// pequena; agrupar por capacidade produz algumas dezenas, cada uma um
    /// pedaço de trabalho que se faz junto. As cartas sem trecho de texto
    /// entram pelo motivo, via [`coverage::classify_reason`]: elas sumiriam de
    /// um relatório que só olhasse texto, e jogá-las todas no balde do layout
    /// mandaria quase dez mil cartas para o crate errado.
    pub fn capability_rows(&self) -> Vec<CapabilityRow> {
        let mut buckets: BTreeMap<&'static str, (&'static Capability, Tally)> = BTreeMap::new();
        {
            let mut push = |cap: &'static Capability, name: &str, mask: PoolMask| {
                let entry = buckets.entry(cap.id).or_insert((cap, Tally::default()));
                entry.1.add(name, mask);
            };
            for blocked in &self.pending {
                let pattern = normalize(&blocked.text, &self.vocab);
                push(coverage::classify(&pattern), &blocked.name, blocked.mask);
            }
            for blocked in &self.no_snippet {
                push(coverage::classify_reason(&blocked.text), &blocked.name, blocked.mask);
            }
        }

        let mut rows: Vec<CapabilityRow> = buckets
            .into_values()
            .map(|(capability, t)| CapabilityRow {
                capability,
                cards: t.cards,
                pauper: t.pool(Pool::Pauper),
                standard: t.pool(Pool::Standard),
                modern: t.pool(Pool::Modern),
                example: t.example,
            })
            .collect();
        // Empate em contagem desempata pelo id, que é estável — a saída tem de
        // ser byte a byte igual entre execuções sobre o mesmo bulk.
        rows.sort_by(|a, b| b.cards.cmp(&a.cards).then(a.capability.id.cmp(b.capability.id)));
        rows
    }

    /// Empate em contagem é desempatado pelo texto, para a saída ser byte a
    /// byte igual entre execuções sobre o mesmo bulk.
    fn to_rows(buckets: BTreeMap<String, Tally>, top: usize) -> (usize, Vec<PatternRow>) {
        let distinct = buckets.len();
        let mut rows: Vec<PatternRow> = buckets
            .into_iter()
            .map(|(pattern, t)| PatternRow { cards: t.cards, pattern, example: t.example })
            .collect();
        rows.sort_by(|a, b| b.cards.cmp(&a.cards).then(a.pattern.cmp(&b.pattern)));
        rows.truncate(top);
        (distinct, rows)
    }

    /// Padrões distintos e as `top` maiores linhas, pelo texto inteiro.
    pub fn rank(&self, top: usize) -> (usize, Vec<PatternRow>) {
        CoverageReport::to_rows(self.tally_by(|p| p.to_string()), top)
    }

    /// Mesma contagem, agrupando pelas primeiras `words` palavras.
    ///
    /// O texto inteiro é específico demais: os travados se espalham por
    /// milhares de padrões distintos e a maior linha mal passa de uma centena
    /// de cartas. O começo da frase é o que se implementa primeiro — "when ~
    /// enters the battlefield, ..." é um parser só, valha o que valer depois
    /// da vírgula.
    pub fn rank_prefixes(&self, top: usize, words: usize) -> (usize, Vec<PatternRow>) {
        CoverageReport::to_rows(self.tally_by(|p| prefix_of(p, words)), top)
    }
}

/// Primeiras `words` palavras, com reticências quando há mais texto.
fn prefix_of(pattern: &str, words: usize) -> String {
    let all: Vec<&str> = pattern.split_whitespace().collect();
    if all.len() <= words {
        return pattern.to_string();
    }
    format!("{} …", all[..words].join(" "))
}

/// Números da importação que entram no cabeçalho do relatório.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoverageHeader {
    pub total_lines: u64,
    pub rejected: u64,
    pub playable: u64,
    pub unplayable: u64,
    pub elapsed_secs: f64,
    pub db_bytes: u64,
}

pub fn to_markdown(
    report: &CoverageReport,
    header: &CoverageHeader,
    bulk_updated_at: &str,
    top: usize,
) -> String {
    let catalog = header.playable + header.unplayable;
    let mut out = String::new();

    out.push_str("# Cobertura do texto de oráculo\n\n");
    out.push_str("Gerado por `mtg-import sync --coverage`. Não editar à mão.\n\n");
    out.push_str(&format!("- Bulk: `oracle_cards`, `{bulk_updated_at}`\n"));
    out.push_str(&format!("- Linhas lidas: {}\n", header.total_lines));
    out.push_str(&format!("- Descartadas na entrada: {}\n", header.rejected));
    out.push_str(&format!("- No catálogo: {catalog}\n"));
    out.push_str(&format!(
        "- Jogáveis: {} ({:.1}%)\n",
        header.playable,
        percent(header.playable, catalog)
    ));
    out.push_str(&format!(
        "- Não jogáveis: {} ({:.1}%)\n",
        header.unplayable,
        percent(header.unplayable, catalog)
    ));
    out.push_str(&format!(
        "- Travadas com trecho de texto: {} · sem trecho, só com motivo: {}\n",
        report.text_blocked(),
        report.blocked_without_snippet()
    ));
    out.push_str(&format!("- Tempo de importação: {:.1}s\n", header.elapsed_secs));
    out.push_str(&format!("- SQLite gerado: {:.1} MB\n", header.db_bytes as f64 / 1_048_576.0));

    push_pool_section(&mut out, report);
    push_capability_section(&mut out, report);
    push_pattern_sections(&mut out, report, top);
    out
}

/// A tabela que troca "11,7% do catálogo" por "quanto do que se joga está
/// coberto".
fn push_pool_section(out: &mut String, report: &CoverageReport) {
    out.push_str("\n## Cobertura por pool\n\n");
    out.push_str(
        "A porcentagem crua do catálogo mede mal: ninguém monta deck com o catálogo inteiro. \
         O que decide se dá para jogar é quanto de um **pool real** está coberto — e um pool \
         pequeno e coerente como Pauper pode chegar perto de 100% enquanto o catálogo inteiro \
         anda a 12%. Banida não conta como legal: ela inflaria o denominador sem mexer no \
         numerador.\n\n",
    );

    if !report.has_legalities() {
        out.push_str(
            "> **Coluna de formato indisponível.** Nenhuma carta desta importação trouxe o \
             campo `legalities`, então Pauper, Standard e Modern não foram medidos. As linhas \
             abaixo sairiam 0% por ausência de dado, não por ausência de cobertura — e 0% \
             falso é pior que buraco declarado.\n\n",
        );
    }

    out.push_str("| Pool | No catálogo | Jogáveis | % jogável |\n");
    out.push_str("|---|---|---|---|\n");
    for row in report.pool_rows() {
        let measured = row.pool.scryfall_format().is_none() || report.has_legalities();
        let cells = if measured {
            format!("{} | {} | {:.1}%", row.in_catalog, row.playable, row.percent())
        } else {
            "— | — | não medido".to_string()
        };
        out.push_str(&format!("| {} | {} |\n", row.pool.label(), cells));
    }
    if report.has_legalities() && report.cards_with_legalities() < report.cards_seen() {
        out.push_str(&format!(
            "\n{} das {} cartas do catálogo não trouxeram `legalities`; elas contam no \
             catálogo e ficam fora das linhas de formato.\n",
            report.cards_seen() - report.cards_with_legalities(),
            report.cards_seen()
        ));
    }
}

/// A tabela que responde "o que implementar agora", separada por quem pode
/// fazer o trabalho.
fn push_capability_section(out: &mut String, report: &CoverageReport) {
    let rows = report.capability_rows();
    // O balde "não classificado" sai das duas tabelas. Ele não é um pedido de
    // IR nem um padrão de parser: é o buraco da PRÓPRIA taxonomia. Deixá-lo na
    // tabela do IR faria milhares de cartas parecerem trabalho de `mtg-core`
    // quando ninguém sabe ainda de que trabalho elas são.
    let (unknown, known): (Vec<_>, Vec<_>) =
        rows.into_iter().partition(|r| r.capability.id == coverage::UNCLASSIFIED.id);
    let (parser, ir): (Vec<_>, Vec<_>) =
        known.into_iter().partition(|r| r.capability.gap == Gap::Parser);

    out.push_str("\n## O que implementar agora\n\n");
    out.push_str(
        "Agrupado por **capacidade faltante**, não por frase literal: a frase literal espalha \
         o trabalho por milhares de linhas de contagem pequena, e a capacidade junta o que se \
         implementa de uma vez só. As colunas de formato existem porque destravar 200 cartas \
         de Un-set vale menos que destravar 80 de Pauper.\n\n",
    );
    out.push_str(
        "A carta é contada na capacidade do seu **primeiro** bloqueio. É o piso do que \
         voltaria a compilar, não o teto: uma carta pode ter mais de um parágrafo travado, e \
         só some da lista quando o último cair.\n\n",
    );

    out.push_str(&format!(
        "### Falta padrão no parser — {} capacidades, sem tocar em `mtg-core`\n\n",
        parser.len()
    ));
    out.push_str(
        "O vocabulário do IR **já tem** a construção; falta o compilador reconhecer o texto. \
         A coluna \"Construção do IR\" nomeia o que resolve, para a afirmação poder ser \
         conferida em `ir.rs` antes de alguém começar.\n\n",
    );
    push_capability_table(out, &parser, "Construção do IR que resolve");

    out.push_str(&format!(
        "\n### Falta capacidade no IR — {} capacidades, exige `mtg-core`\n\n",
        ir.len()
    ));
    out.push_str(
        "Não sai sem vocabulário novo no motor. São os **pedidos de IR**: enquanto não \
         existirem, estas cartas continuam `Unsupported` de propósito — marcar como jogável \
         algo que o motor não sabe executar quebra a partida em silêncio, que é bem pior que \
         carta ausente.\n\n",
    );
    push_capability_table(out, &ir, "O que falta em `mtg-core`");
    push_unclassified(out, &unknown);
}

/// O que a taxonomia ainda não sabe nomear.
///
/// Esta seção existe para o buraco aparecer com tamanho. Enquanto ela for a
/// maior linha do relatório, a próxima hora de trabalho mais valiosa é
/// classificar, não implementar: não dá para priorizar o que não tem nome.
fn push_unclassified(out: &mut String, rows: &[CapabilityRow]) {
    let Some(row) = rows.first() else { return };
    out.push_str("
### Ainda sem nome: o buraco da própria taxonomia

");
    out.push_str(&format!(
        "**{} cartas** ({} em Pauper, {} em Standard, {} em Modern) travaram num texto que          nenhuma regra de `mtg_oracle::coverage` reconheceu. Elas NÃO são pedido de IR nem          padrão de parser: são cartas de que ainda não se sabe de quem é o trabalho. Exemplo:          {}.

",
        row.cards,
        row.pauper,
        row.standard,
        row.modern,
        escape_cell(&row.example)
    ));
    out.push_str(
        "Enquanto esta for a maior linha do relatório, a hora de trabalho mais valiosa é          **classificar**, não implementar — a tabela de prioridade só vale o que a taxonomia          cobre. As tabelas de padrão literal logo abaixo são a matéria-prima para isso.
",
    );
}

fn push_capability_table(out: &mut String, rows: &[CapabilityRow], need_header: &str) {
    out.push_str(&format!(
        "| # | Capacidade | Cartas | Pauper | Standard | Modern | {need_header} | Exemplo |\n"
    ));
    out.push_str("|---|---|---|---|---|---|---|---|\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            i + 1,
            escape_cell(row.capability.label),
            row.cards,
            row.pauper,
            row.standard,
            row.modern,
            escape_cell(row.capability.need),
            escape_cell(&row.example)
        ));
    }
}

/// As tabelas de frase literal. Continuam no relatório como **evidência** —
/// é onde se confere que uma capacidade agrupou o que devia — mas não é por
/// elas que se escolhe trabalho: a maior linha mal passa de uma centena.
fn push_pattern_sections(out: &mut String, report: &CoverageReport, top: usize) {
    let (distinct, rows) = report.rank(top);

    out.push_str("\n## Evidência: os padrões literais por trás dos números\n\n");
    out.push_str(&format!(
        "`~` é o nome da própria carta, `N` é qualquer número, `<tipo>` é subtipo de criatura, \
         `<cor>` é cor e `<terreno>` é tipo de terreno básico. {distinct} padrões distintos, \
         {} subtipos de criatura no vocabulário. Estas tabelas servem para **conferir** o \
         agrupamento acima, não para priorizar.\n\n",
        report.vocabulary_size()
    ));

    out.push_str(&format!("### {} padrões não suportados mais frequentes\n\n", rows.len()));
    push_table(out, &rows);

    let (prefix_distinct, prefixes) = report.rank_prefixes(top, PREFIX_WORDS);
    out.push_str(&format!(
        "\n### Por começo de frase ({PREFIX_WORDS} primeiras palavras, {prefix_distinct} distintos)\n\n"
    ));
    push_table(out, &prefixes);
}

fn push_table(out: &mut String, rows: &[PatternRow]) {
    out.push_str("| # | Cartas | Padrão normalizado | Exemplo |\n");
    out.push_str("|---|---|---|---|\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str(&format!(
            "| {} | {} | `{}` | {} |\n",
            i + 1,
            row.cards,
            escape_cell(&row.pattern),
            escape_cell(&row.example)
        ));
    }
}

/// Barra vertical fecharia a célula da tabela; crase fecharia o code span.
fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('`', "'")
}

fn percent(part: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    part as f64 * 100.0 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creature_line(subtypes: &[&str]) -> TypeLine {
        TypeLine {
            types: vec![CardType::Creature],
            subtypes: subtypes.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    /// Observa a mesma linha o bastante para os subtipos passarem do limiar.
    fn vocab_with(subtypes: &[&str]) -> Vocabulary {
        let mut v = Vocabulary::default();
        for _ in 0..MIN_SUBTYPE_CARDS {
            v.observe(&creature_line(subtypes));
        }
        v
    }

    /// Máscara de uma carta legal nos formatos dados, com a raridade dada.
    fn mask(rarity: &str, formats: &[&str]) -> PoolMask {
        let owned: Vec<String> = formats.iter().map(|s| s.to_string()).collect();
        coverage::pools_of(rarity, |f| {
            if owned.iter().any(|x| x == f) {
                Some("legal")
            } else {
                Some("not_legal")
            }
        })
    }

    /// Registra uma carta travada por texto já com o vocabulário formado.
    fn report_with(blocks: &[(&str, &str, PoolMask)]) -> CoverageReport {
        let mut r = CoverageReport::new();
        for (snippet, name, m) in blocks {
            r.observe_card(&TypeLine::default(), *m, false, true);
            r.add_text_block(snippet, name, *m);
        }
        r
    }

    #[test]
    fn numbers_collapse_to_n() {
        let v = Vocabulary::default();
        assert_eq!(
            normalize("~ deals 3 damage to any target", &v),
            "~ deals N damage to any target"
        );
        assert_eq!(
            normalize("~ deals 10 damage to any target", &v),
            "~ deals N damage to any target"
        );
        assert_eq!(normalize("put a +1/+1 counter on it", &v), "put a +N/+N counter on it");
        assert_eq!(normalize("draw two cards", &v), "draw N cards");
    }

    #[test]
    fn one_is_not_a_number() {
        // "choose one" é cabeçalho de modal, não contagem. Virar `N` juntaria
        // padrões que não têm nada a ver.
        let v = Vocabulary::default();
        assert_eq!(normalize("choose one of them", &v), "choose one of them");
    }

    #[test]
    fn creature_subtypes_and_colors_collapse() {
        let v = vocab_with(&["Soldier", "Bear", "Elf"]);
        let a = normalize("create a 1/1 white soldier creature token", &v);
        let b = normalize("create a 2/2 green bear creature token", &v);
        assert_eq!(a, "create a N/N <cor> <tipo> creature token");
        assert_eq!(a, b, "duas fichas diferentes são o mesmo trabalho de IR");
        assert_eq!(normalize("elves you control get +1/+0", &v), "<tipo> you control get +N/+N");
    }

    #[test]
    fn non_creature_subtypes_stay_literal() {
        // Vocabulário só tem subtipo de criatura, então Treasure continua ele
        // mesmo: implementar Treasure não é implementar Clue.
        let mut v = Vocabulary::default();
        v.observe(&TypeLine {
            types: vec![CardType::Artifact],
            subtypes: vec!["Treasure".to_string()],
            ..Default::default()
        });
        assert!(v.is_empty());
        assert_eq!(normalize("create a treasure token", &v), "create a treasure token");
    }

    #[test]
    fn basic_lands_collapse() {
        let v = Vocabulary::default();
        let a = normalize("search your library for a plains card", &v);
        let b = normalize("search your library for a forest card", &v);
        assert_eq!(a, b);
        assert_eq!(a, "search your library for a <terreno> card");
    }

    #[test]
    fn long_text_is_cut_on_word_boundary() {
        let v = Vocabulary::default();
        let long = "word ".repeat(80);
        let cut = normalize(&long, &v);
        assert!(cut.chars().count() <= MAX_PATTERN_LEN + 1, "cabe na tabela");
        assert!(cut.ends_with('…'));
        assert!(!cut.contains("wor…"), "corte respeita a palavra");
    }

    #[test]
    fn ranking_is_deterministic_and_by_frequency() {
        let mut r = CoverageReport::new();
        for _ in 0..MIN_SUBTYPE_CARDS {
            r.observe_card(&creature_line(&["Bear"]), PoolMask::empty(), true, true);
        }
        r.add_text_block("create a 2/2 bear creature token", "Zed", PoolMask::empty());
        r.add_text_block("create a 3/3 bear creature token", "Alpha", PoolMask::empty());
        r.add_text_block("something else entirely", "Mid", PoolMask::empty());

        let (distinct, rows) = r.rank(10);
        assert_eq!(distinct, 2);
        assert_eq!(rows[0].cards, 2);
        assert_eq!(rows[0].pattern, "create a N/N <tipo> creature token");
        assert_eq!(rows[0].example, "Alpha", "exemplo é estável, não o primeiro lido");
        assert_eq!(rows[1].cards, 1);

        // Mesma entrada, mesma saída.
        let (_, again) = r.rank(10);
        assert_eq!(rows, again);
    }

    #[test]
    fn rare_subtype_is_noise_not_vocabulary() {
        // "Creature — The Biggest, Baddest, Nastiest," existe (B.F.M.). Sem o
        // limiar, "the" virava subtipo e todo padrão que dissesse "the top
        // card" saía deformado.
        let mut v = Vocabulary::default();
        v.observe(&creature_line(&["The"]));
        assert!(v.is_empty(), "subtipo visto uma vez só é ruído");
        assert_eq!(
            normalize("look at the top 3 cards of your library", &v),
            "look at the top N cards of your library"
        );
    }

    #[test]
    fn stoplisted_subtype_stays_literal() {
        // "Time" é subtipo real (Time Lord) e passa do limiar, mas "time"
        // aparece muito mais como palavra de regra.
        let v = vocab_with(&["Time"]);
        assert_eq!(v.len(), 1, "entrou no vocabulário");
        assert_eq!(normalize("the first time this turn", &v), "the first time this turn");
    }

    #[test]
    fn prefixes_group_what_full_text_splits() {
        let mut r = CoverageReport::new();
        let m = PoolMask::empty();
        r.add_text_block("when ~ enters, draw a card", "Beta", m);
        r.add_text_block("when ~ enters, gain 3 life", "Alpha", m);
        r.add_text_block("when ~ enters, each opponent discards", "Gamma", m);

        let (full, _) = r.rank(10);
        assert_eq!(full, 3, "texto inteiro separa as três");

        // Três palavras é onde as três frases ainda coincidem; na quarta elas
        // já divergem, e o agrupamento se desfaz.
        let (_, rows) = r.rank_prefixes(10, 3);
        assert_eq!(rows[0].cards, 3, "o começo de frase junta as três");
        assert_eq!(rows[0].pattern, "when ~ enters, …");
        assert_eq!(rows[0].example, "Alpha");

        let (_, four) = r.rank_prefixes(10, 4);
        assert_eq!(four[0].cards, 1, "uma palavra a mais separa de novo");
    }

    #[test]
    fn markdown_escapes_table_breakers() {
        let mut r = CoverageReport::new();
        r.add_text_block("a | b `c`", "Pipe Card", PoolMask::empty());
        let md = to_markdown(&r, &CoverageHeader::default(), "2026-08-18", 5);
        assert!(md.contains("a \\| b 'c'"), "pipe e crase não podem quebrar a tabela");
        // Duas tabelas de padrão — texto inteiro e começo de frase.
        assert!(md.contains("padrões não suportados mais frequentes"));
        assert!(md.contains("Por começo de frase"));
    }

    // -----------------------------------------------------------------------
    // Cobertura por pool
    // -----------------------------------------------------------------------

    #[test]
    fn pool_coverage_counts_each_pool_separately() {
        let mut r = CoverageReport::new();
        // Uma comum de Pauper que compila.
        r.observe_card(&TypeLine::default(), mask("common", &["pauper", "modern"]), true, true);
        // Uma comum de Pauper que não compila.
        r.observe_card(&TypeLine::default(), mask("common", &["pauper", "modern"]), false, true);
        // Uma rara de Standard que compila.
        r.observe_card(
            &TypeLine::default(),
            mask("rare", &["standard", "modern"]),
            true,
            true,
        );

        let rows = r.pool_rows();
        let by = |p: Pool| rows.iter().find(|x| x.pool == p).copied().expect("pool na tabela");

        assert_eq!(by(Pool::Catalog).in_catalog, 3);
        assert_eq!(by(Pool::Catalog).playable, 2);

        let pauper = by(Pool::Pauper);
        assert_eq!((pauper.in_catalog, pauper.playable), (2, 1));
        assert!((pauper.percent() - 50.0).abs() < 1e-9, "Pauper mede 50%, não 66%");

        let standard = by(Pool::Standard);
        assert_eq!((standard.in_catalog, standard.playable), (1, 1));
        assert!((standard.percent() - 100.0).abs() < 1e-9);

        // Só as duas comuns entram na espinha; a rara não.
        let backbone = by(Pool::CommonUncommon);
        assert_eq!((backbone.in_catalog, backbone.playable), (2, 1));

        // A ordem das linhas é a de `Pool::ALL`, não a de um mapa.
        let order: Vec<Pool> = rows.iter().map(|x| x.pool).collect();
        assert_eq!(order, Pool::ALL.to_vec());
    }

    #[test]
    fn pool_percent_differs_from_raw_catalog_percent() {
        // O ponto inteiro da tabela: o número cru do catálogo e o do pool
        // divergem. Um teste que só olhasse o catálogo passaria sem afirmar
        // nada sobre o que importa.
        let mut r = CoverageReport::new();
        for _ in 0..9 {
            // Nove raras de Modern que não compilam.
            r.observe_card(&TypeLine::default(), mask("rare", &["modern"]), false, true);
        }
        // Uma comum de Pauper que compila.
        r.observe_card(&TypeLine::default(), mask("common", &["pauper"]), true, true);

        let rows = r.pool_rows();
        let by = |p: Pool| rows.iter().find(|x| x.pool == p).copied().expect("pool na tabela");
        assert!((by(Pool::Catalog).percent() - 10.0).abs() < 1e-9, "catálogo cru: 10%");
        assert!((by(Pool::Pauper).percent() - 100.0).abs() < 1e-9, "Pauper: 100%");
        assert!((by(Pool::Modern).percent() - 0.0).abs() < 1e-9, "Modern: 0%");
    }

    #[test]
    fn missing_legalities_is_declared_not_faked() {
        let mut r = CoverageReport::new();
        // Carta sem o campo: `pools_of` só devolve o catálogo.
        let no_leg = coverage::pools_of("rare", |_| None);
        r.observe_card(&TypeLine::default(), no_leg, true, false);

        assert!(!r.has_legalities());
        let md = to_markdown(&r, &CoverageHeader::default(), "2026-08-18", 5);
        assert!(md.contains("Coluna de formato indisponível"), "a ausência tem de ser dita");
        assert!(md.contains("não medido"), "formato sem dado não vira 0%");
        assert!(
            !md.contains("| Pauper | 0 | 0 | 0.0% |"),
            "0% falso é pior que buraco declarado"
        );
    }

    #[test]
    fn legalities_present_produces_measured_rows() {
        let mut r = CoverageReport::new();
        r.observe_card(&TypeLine::default(), mask("common", &["pauper"]), true, true);
        assert!(r.has_legalities());
        let md = to_markdown(&r, &CoverageHeader::default(), "2026-08-18", 5);
        assert!(!md.contains("Coluna de formato indisponível"));
        assert!(md.contains("| Pauper | 1 | 1 | 100.0% |"));
    }

    // -----------------------------------------------------------------------
    // Capacidades
    // -----------------------------------------------------------------------

    #[test]
    fn capabilities_group_what_literal_patterns_split() {
        // Três frases literais diferentes, um único pedaço de trabalho.
        let r = report_with(&[
            ("scry 2", "Beta", PoolMask::empty()),
            ("surveil 3", "Alpha", PoolMask::empty()),
            ("target player mills 4 cards", "Gamma", PoolMask::empty()),
        ]);

        let (literal, _) = r.rank(10);
        assert_eq!(literal, 3, "por frase literal são três linhas de contagem 1");

        let rows = r.capability_rows();
        assert_eq!(rows.len(), 1, "por capacidade é uma linha só");
        assert_eq!(rows[0].capability.id, "vidente-e-moer");
        assert_eq!(rows[0].cards, 3);
        assert_eq!(rows[0].example, "Alpha", "exemplo estável, não o primeiro lido");
    }

    #[test]
    fn capability_rows_carry_format_counts() {
        // Duas cartas na mesma capacidade: uma de Pauper, uma só de Modern.
        // A coluna é o que separa "80 de Pauper" de "200 de Un-set".
        let r = report_with(&[
            ("scry 2", "Pauper Card", mask("common", &["pauper", "modern"])),
            ("surveil 1", "Fancy Card", mask("mythic", &["modern"])),
            ("mills 3 cards", "Un Card", mask("rare", &[])),
        ]);
        let rows = r.capability_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cards, 3, "três cartas na capacidade");
        assert_eq!(rows[0].pauper, 1, "só uma é legal em Pauper");
        assert_eq!(rows[0].modern, 2);
        assert_eq!(rows[0].standard, 0);
    }

    #[test]
    fn cards_without_a_snippet_are_classified_by_reason() {
        // Carta de duas faces trava antes de o texto ser lido. Se o relatório
        // só olhasse texto, dez mil cartas sumiriam da priorização.
        let mut r = CoverageReport::new();
        let m = mask("common", &["pauper"]);
        r.observe_card(&TypeLine::default(), m, false, true);
        r.add_blocked_without_snippet(
            "layout 'transform': a face frontal depende da outra ('transform')",
            "Delver of Secrets",
            m,
        );

        let rows = r.capability_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].capability.id, "layout-multiface");
        assert_eq!(rows[0].cards, 1);
        assert_eq!(rows[0].pauper, 1, "layout também é medido por formato");
        assert_eq!(rows[0].capability.gap, Gap::Ir);
    }

    #[test]
    fn capability_rows_are_sorted_by_cards_and_deterministic() {
        let r = report_with(&[
            ("scry 1", "A", PoolMask::empty()),
            ("scry 2", "B", PoolMask::empty()),
            ("regenerate ~", "C", PoolMask::empty()),
        ]);
        let rows = r.capability_rows();
        assert_eq!(rows[0].cards, 2, "a maior vem primeiro");
        assert_eq!(rows[0].capability.id, "vidente-e-moer");
        assert_eq!(rows[1].capability.id, "regeneracao");

        for _ in 0..5 {
            assert_eq!(r.capability_rows(), rows, "mesma entrada, mesma ordem");
        }
    }

    #[test]
    fn markdown_splits_parser_work_from_ir_work() {
        // A separação que decide quem pode trabalhar em quê: `scry` é parser
        // (o IR já tem `Effect::Scry`), `regenerate` é IR (falta escudo).
        let r = report_with(&[
            ("scry 2", "Scry Card", mask("common", &["pauper"])),
            ("regenerate ~", "Regen Card", mask("common", &["pauper"])),
        ]);
        let md = to_markdown(&r, &CoverageHeader::default(), "2026-08-18", 5);

        let parser_at = md.find("Falta padrão no parser").expect("seção do parser");
        let ir_at = md.find("Falta capacidade no IR").expect("seção do IR");
        assert!(parser_at < ir_at, "o que dá para fazer hoje vem primeiro");

        let parser_block = &md[parser_at..ir_at];
        assert!(parser_block.contains("Scry Card"), "scry é trabalho de parser");
        assert!(!parser_block.contains("Regen Card"), "regenerate não é trabalho de parser");

        // A seção do IR termina onde começam as tabelas de evidência, que
        // listam os dois exemplos de novo. Sem esse limite o teste passaria
        // por acidente em vez de afirmar a separação.
        let evidence_at = md.find("## Evid").expect("seção de evidência");
        assert!(ir_at < evidence_at);
        let ir_block = &md[ir_at..evidence_at];
        assert!(ir_block.contains("Regen Card"));
        assert!(!ir_block.contains("Scry Card"));
    }

    #[test]
    fn markdown_is_byte_identical_across_runs() {
        let r = report_with(&[
            ("scry 2", "Alpha", mask("common", &["pauper", "modern"])),
            ("regenerate ~", "Beta", mask("rare", &["modern"])),
            ("look at the top 3 cards of your library", "Gamma", mask("uncommon", &["standard"])),
        ]);
        let header = CoverageHeader {
            total_lines: 10,
            rejected: 1,
            playable: 4,
            unplayable: 5,
            elapsed_secs: 1.25,
            db_bytes: 2_097_152,
        };
        let first = to_markdown(&r, &header, "2026-08-18", 10);
        for _ in 0..5 {
            assert_eq!(to_markdown(&r, &header, "2026-08-18", 10), first);
        }
        // E os números do pool aparecem de fato no texto gerado.
        assert!(first.contains("## Cobertura por pool"));
        assert!(first.contains("| Pauper | 1 | 0 | 0.0% |"));
    }
}
