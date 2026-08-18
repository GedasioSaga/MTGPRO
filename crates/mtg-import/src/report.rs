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

#[derive(Debug, Clone, Default)]
struct Tally {
    cards: u64,
    /// Menor nome em ordem alfabética entre as cartas do padrão. Escolha
    /// arbitrária mas **estável**: o exemplo não muda quando o Scryfall
    /// reordena o bulk, então o diff do relatório mostra só o que mudou.
    example: String,
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
    pending: Vec<(String, String)>,
    /// Cartas não jogáveis cujo bloqueio não é textual (layout, duas faces).
    structural: u64,
}

impl CoverageReport {
    pub fn new() -> CoverageReport {
        CoverageReport::default()
    }

    pub fn observe_type_line(&mut self, type_line: &TypeLine) {
        self.vocab.observe(type_line);
    }

    pub fn add_text_block(&mut self, snippet: &str, card_name: &str) {
        self.pending.push((snippet.to_string(), card_name.to_string()));
    }

    pub fn add_structural_block(&mut self) {
        self.structural += 1;
    }

    pub fn text_blocked(&self) -> u64 {
        self.pending.len() as u64
    }

    pub fn structural_blocked(&self) -> u64 {
        self.structural
    }

    pub fn vocabulary_size(&self) -> usize {
        self.vocab.len()
    }

    /// Agrupa os textos por uma chave derivada do padrão normalizado.
    fn tally_by(&self, key_of: impl Fn(&str) -> String) -> BTreeMap<String, Tally> {
        let mut buckets: BTreeMap<String, Tally> = BTreeMap::new();
        for (snippet, name) in &self.pending {
            let key = key_of(&normalize(snippet, &self.vocab));
            if key.is_empty() {
                continue;
            }
            let entry = buckets.entry(key).or_default();
            entry.cards += 1;
            if entry.example.is_empty() || name.as_str() < entry.example.as_str() {
                entry.example = name.clone();
            }
        }
        buckets
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
    /// da vírgula. É esta tabela que responde "o que destrava 400 cartas".
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
    let (distinct, rows) = report.rank(top);
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
        "- Bloqueio textual: {} · bloqueio estrutural (layout, duas faces): {}\n",
        report.text_blocked(),
        report.structural_blocked()
    ));
    out.push_str(&format!(
        "- Padrões distintos: {distinct} · subtipos de criatura no vocabulário: {}\n",
        report.vocabulary_size()
    ));
    out.push_str(&format!("- Tempo de importação: {:.1}s\n", header.elapsed_secs));
    out.push_str(&format!("- SQLite gerado: {:.1} MB\n", header.db_bytes as f64 / 1_048_576.0));

    out.push_str("\n## Como ler\n\n");
    out.push_str(
        "`~` é o nome da própria carta, `N` é qualquer número, `<tipo>` é subtipo de criatura, \
         `<cor>` é cor e `<terreno>` é tipo de terreno básico. A coluna **Cartas** é quantas \
         cartas têm esse padrão como **primeiro** bloqueio — é o piso do que voltaria a \
         compilar, não o teto, porque uma carta pode ter mais de um parágrafo travado.\n\n",
    );

    out.push_str(&format!("## {} padrões não suportados mais frequentes\n\n", rows.len()));
    push_table(&mut out, &rows);

    let (prefix_distinct, prefixes) = report.rank_prefixes(top, PREFIX_WORDS);
    out.push_str(&format!(
        "\n## Por começo de frase ({PREFIX_WORDS} primeiras palavras, {prefix_distinct} distintos)\n\n"
    ));
    out.push_str(
        "O texto inteiro é específico demais para priorizar: quase toda carta tem a sua \
         variação. Agrupando pelo começo da frase aparece o parser que se escreve **uma vez** \
         e cobre a família inteira. É por esta tabela que se escolhe o próximo trabalho.\n\n",
    );
    push_table(&mut out, &prefixes);
    out
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
            r.observe_type_line(&creature_line(&["Bear"]));
        }
        r.add_text_block("create a 2/2 bear creature token", "Zed");
        r.add_text_block("create a 3/3 bear creature token", "Alpha");
        r.add_text_block("something else entirely", "Mid");

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
        r.add_text_block("when ~ enters, draw a card", "Beta");
        r.add_text_block("when ~ enters, gain 3 life", "Alpha");
        r.add_text_block("when ~ enters, each opponent discards", "Gamma");

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
        r.add_text_block("a | b `c`", "Pipe Card");
        let md = to_markdown(&r, &CoverageHeader::default(), "2026-08-18", 5);
        assert!(md.contains("a \\| b 'c'"), "pipe e crase não podem quebrar a tabela");
        // Duas tabelas — texto inteiro e começo de frase — logo dois "| 1 |".
        assert_eq!(md.lines().filter(|l| l.starts_with("| 1 |")).count(), 2);
    }
}
