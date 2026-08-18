//! Normalização do texto de oracle.
//!
//! Duas formas saem daqui e servem a propósitos diferentes:
//!   `norm`    — minúsculo, sem texto lembrete, nome próprio trocado por `~`.
//!               É sobre isto que o casamento de padrão roda.
//!   `pattern` — o mesmo, com números também trocados por `N`. Serve para
//!               agrupar famílias de texto e contar frequência do que falta.

/// Uma linha do texto de oracle, no original e na forma normalizada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleLine {
    /// Linha como impressa, só com espaços colapsados.
    pub raw: String,
    /// Linha normalizada para casamento de padrão.
    pub norm: String,
}

/// Palavras que contam como número ao gerar `pattern`. "a"/"an" ficam de fora:
/// são artigo com muito mais frequência do que quantidade, e trocá-las
/// destruiria o agrupamento em vez de ajudá-lo.
const NUMBER_WORDS_IN_PATTERN: [&str; 10] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];

/// Palavras aceitas como quantidade ao interpretar um efeito.
const NUMBER_WORDS: [(&str, i32); 12] = [
    ("a", 1),
    ("an", 1),
    ("one", 1),
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
];

/// Interpreta `"3"`, `"a"` ou `"three"` como quantidade.
pub fn parse_count(word: &str) -> Option<i32> {
    let w = word.trim();
    if w.is_empty() {
        return None;
    }
    if w.chars().all(|c| c.is_ascii_digit()) {
        return w.parse::<i32>().ok();
    }
    NUMBER_WORDS.iter().find(|(k, _)| *k == w).map(|(_, n)| *n)
}

/// Interpreta `"+3"` ou `"-2"` como delta de P/T.
pub fn parse_signed(word: &str) -> Option<i32> {
    let w = word.trim();
    if let Some(digits) = w.strip_prefix('+') {
        return digits.parse::<i32>().ok();
    }
    if let Some(digits) = w.strip_prefix('-') {
        return digits.parse::<i32>().ok().map(|n| -n);
    }
    None
}

fn fold(c: char) -> char {
    match c {
        '\u{2014}' | '\u{2013}' | '\u{2212}' => '-',
        '\u{2018}' | '\u{2019}' => '\'',
        '\u{201c}' | '\u{201d}' => '"',
        '\u{00a0}' => ' ',
        other => other,
    }
}

/// Remove texto lembrete. Lembrete não é texto de regras: mantê-lo faria
/// terreno básico (cujo texto é lembrete inteiro) parecer ter texto próprio.
fn strip_reminder(s: &str) -> String {
    let mut out = String::new();
    let mut depth: usize = 0;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Troca `needle` por `~` só onde ele é palavra inteira.
///
/// `str::replace` cru transformaria "Flying" no texto de uma carta chamada
/// "Fly" em "~ing", e o reconhecedor de palavra-chave perderia a carta sem
/// dizer por quê. Fronteira de palavra é o que separa autorreferência de
/// coincidência de substring.
fn replace_whole_word(haystack: &str, needle: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    let bytes = haystack.as_bytes();
    let mut out = String::with_capacity(haystack.len());
    let mut cursor = 0;
    while let Some(hit) = haystack[cursor..].find(needle) {
        let start = cursor + hit;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_word_byte(bytes[end]);
        out.push_str(&haystack[cursor..start]);
        if before_ok && after_ok {
            out.push('~');
        } else {
            out.push_str(needle);
        }
        cursor = end;
    }
    out.push_str(&haystack[cursor..]);
    out
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}

/// Autorreferência na redação nova. Desde 2024 o oracle do Scryfall troca o
/// nome próprio por "this creature"/"this land"/"this spell" — a mesma coisa
/// que `ObjRef::SelfObject`, escrita de outro jeito. Sem isto, toda reimpressão
/// recente vira `Unsupported` e o catálogo fica preso na redação antiga.
const SELF_REFERENCES: [&str; 12] = [
    "this creature",
    "this land",
    "this artifact",
    "this enchantment",
    "this planeswalker",
    "this equipment",
    "this vehicle",
    "this aura",
    "this token",
    "this permanent",
    "this spell",
    "this card",
];

/// Quebra o texto de oracle em linhas normalizadas, descartando as que só
/// tinham lembrete.
pub fn normalize_lines(oracle_text: &str, card_name: &str) -> Vec<OracleLine> {
    let name = card_name.trim().to_lowercase();
    // Nome curto de lendária ("Jace, the Mind Sculptor" -> "jace"): o texto
    // impresso se autorreferencia pelo nome curto.
    let short = name
        .split_once(',')
        .map(|(head, _)| head.trim().to_string())
        .filter(|s| s.chars().count() >= 3);

    let mut out = Vec::new();
    for source_line in oracle_text.split('\n') {
        let raw = collapse_ws(source_line);
        if raw.is_empty() {
            continue;
        }
        let folded: String = raw.chars().map(fold).collect();
        let mut norm = collapse_ws(&strip_reminder(&folded.to_lowercase()));
        if !name.is_empty() {
            norm = replace_whole_word(&norm, &name);
        }
        if let Some(s) = &short {
            norm = replace_whole_word(&norm, s);
        }
        for reference in SELF_REFERENCES {
            norm = replace_whole_word(&norm, reference);
        }
        norm = collapse_ws(&norm);
        if norm.is_empty() {
            continue;
        }
        out.push(OracleLine { raw, norm });
    }
    out
}

/// Troca todo número por `N` para que textos da mesma família agrupem.
pub fn pattern_of(normalized: &str) -> String {
    let chars: Vec<char> = normalized.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            out.push('N');
        } else if c.is_ascii_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if NUMBER_WORDS_IN_PATTERN.contains(&word.as_str()) {
                out.push('N');
            } else {
                out.push_str(&word);
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Padrão do texto inteiro de uma carta — o que o importador conta por
/// frequência para saber que família de texto compensa implementar em seguida.
pub fn oracle_pattern(oracle_text: &str, card_name: &str) -> String {
    let joined = normalize_lines(oracle_text, card_name)
        .into_iter()
        .map(|l| l.norm)
        .collect::<Vec<_>>()
        .join("\n");
    pattern_of(&joined)
}
