//! Catálogo: carrega as cartas escritas em Lua e entrega um `CardDatabase`.
//!
//! Carta é conteúdo, não código Rust. Os arquivos vivem em `cards/*.lua` e são
//! interpretados uma vez, no carregamento, por `mtg-script`. Este crate só faz
//! três coisas: achar os arquivos, mandá-los na ordem certa para o interpretador
//! e validar o resultado.
//!
//! # Onde os arquivos são procurados
//!
//! Na ordem, parando no primeiro que existir:
//!
//! 1. `$MTG_CARDS_DIR` — override explícito. Se a variável estiver definida e
//!    não apontar para um diretório, isso é **erro**, não motivo para cair no
//!    próximo caso: configuração errada tem que aparecer.
//! 2. `CARGO_MANIFEST_DIR/../../cards` — a árvore de fontes, para `cargo test`
//!    e desenvolvimento, independente do diretório de trabalho.
//! 3. `./cards` — relativo ao diretório de trabalho do processo.
//! 4. Cópia embutida no binário via `include_str!`.
//!
//! O passo 4 é o que permite ao servidor rodar de qualquer lugar, sem levar a
//! pasta `cards/` junto. Os passos 1–3 é que permitem editar carta e recarregar
//! sem recompilar; por isso o disco ganha do embutido quando existe.
#![forbid(unsafe_code)]

mod decks;

pub use decks::{deck_by_name, decks, DeckList};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mtg_core::card::CardDatabase;
use mtg_script::{CardScriptHost, ScriptError};

/// Ordem de carga dos arquivos do catálogo.
///
/// Não é alfabética de propósito: `tokens.lua` define as plantas de ficha
/// usadas pelas cartas, então precisa rodar antes de qualquer arquivo de carta.
/// Depender da ordem do sistema de arquivos aqui daria erro que só aparece em
/// uma das máquinas.
pub const CARD_FILES: [&str; 8] = [
    "tokens.lua",
    "artifacts.lua",
    "lands.lua",
    "white.lua",
    "blue.lua",
    "black.lua",
    "red.lua",
    "green.lua",
];

/// Cópia dos scripts embutida no binário, na mesma ordem de `CARD_FILES`.
const EMBEDDED: [(&str, &str); 8] = [
    ("tokens.lua", include_str!("../../../cards/tokens.lua")),
    ("artifacts.lua", include_str!("../../../cards/artifacts.lua")),
    ("lands.lua", include_str!("../../../cards/lands.lua")),
    ("white.lua", include_str!("../../../cards/white.lua")),
    ("blue.lua", include_str!("../../../cards/blue.lua")),
    ("black.lua", include_str!("../../../cards/black.lua")),
    ("red.lua", include_str!("../../../cards/red.lua")),
    ("green.lua", include_str!("../../../cards/green.lua")),
];

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("erro no script de carta: {0}")]
    Script(#[from] ScriptError),
    #[error("erro de leitura em {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("MTG_CARDS_DIR aponta para {0}, que não é um diretório")]
    BadCardsDir(String),
    #[error("catálogo vazio: nenhuma carta foi carregada")]
    Empty,
    #[error("nome de carta duplicado no catálogo: {0}")]
    DuplicateName(String),
}

/// De onde o catálogo vai ser lido nesta execução.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardSource {
    Dir(PathBuf),
    Embedded,
}

/// Resolve a origem dos scripts segundo a ordem documentada no topo do módulo.
pub fn card_source() -> Result<CardSource, LoadError> {
    if let Some(raw) = std::env::var_os("MTG_CARDS_DIR") {
        let dir = PathBuf::from(&raw);
        if !dir.is_dir() {
            return Err(LoadError::BadCardsDir(dir.display().to_string()));
        }
        return Ok(CardSource::Dir(dir));
    }
    let from_manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("cards");
    if from_manifest.is_dir() {
        return Ok(CardSource::Dir(from_manifest));
    }
    let from_cwd = PathBuf::from("cards");
    if from_cwd.is_dir() {
        return Ok(CardSource::Dir(from_cwd));
    }
    Ok(CardSource::Embedded)
}

/// Diretório efetivo do catálogo, ou `None` quando a cópia embutida é a fonte.
pub fn cards_dir() -> Option<PathBuf> {
    match card_source() {
        Ok(CardSource::Dir(d)) => Some(d),
        _ => None,
    }
}

/// Carrega o catálogo completo — o ponto de entrada de todo o resto do sistema.
pub fn build_database() -> Result<CardDatabase, LoadError> {
    match card_source()? {
        CardSource::Dir(dir) => build_database_from_dir(&dir),
        CardSource::Embedded => build_database_embedded(),
    }
}

/// Carrega de um diretório específico. Útil para teste e para conteúdo de
/// terceiro, que roda no mesmo sandbox de Lua das cartas oficiais.
pub fn build_database_from_dir(dir: &Path) -> Result<CardDatabase, LoadError> {
    let host = CardScriptHost::new()?;
    for name in CARD_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let src = read(&path)?;
        host.eval(&src, name)?;
    }
    // Arquivo extra que alguém tenha adicionado ao diretório: entra depois dos
    // canônicos, em ordem alfabética, para o resultado não variar com o
    // sistema de arquivos.
    for path in extra_scripts(dir)? {
        let src = read(&path)?;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string());
        host.eval(&src, name.as_deref().unwrap_or("<script>"))?;
    }
    finish(host)
}

/// Carrega a cópia embutida no binário, sem tocar no sistema de arquivos.
pub fn build_database_embedded() -> Result<CardDatabase, LoadError> {
    let host = CardScriptHost::new()?;
    for (name, src) in EMBEDDED {
        host.eval(src, name)?;
    }
    finish(host)
}

fn finish(host: CardScriptHost) -> Result<CardDatabase, LoadError> {
    let db = host.build_database()?;
    if db.is_empty() {
        return Err(LoadError::Empty);
    }
    let mut seen: HashMap<String, ()> = HashMap::with_capacity(db.len());
    for card in &db.cards {
        let key = card.name.to_ascii_lowercase();
        if seen.insert(key, ()).is_some() {
            // Nome duplicado quebra `by_name`, e `by_name` é como os decks
            // referenciam carta — o erro precisa aparecer na carga, não numa
            // partida meia hora depois.
            return Err(LoadError::DuplicateName(card.name.clone()));
        }
    }
    Ok(db)
}

fn read(path: &Path) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn extra_scripts(dir: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let entries = std::fs::read_dir(dir).map_err(|source| LoadError::Io {
        path: dir.display().to_string(),
        source,
    })?;
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "lua"))
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            !CARD_FILES.contains(&name.as_str())
        })
        .collect();
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::card::Ability;
    use mtg_core::ir::Effect;
    use mtg_core::types::CardType;

    fn db() -> CardDatabase {
        let db = build_database().expect("catálogo carrega");
        assert!(!db.cards.is_empty(), "catálogo vazio: varredura não afirmaria nada");
        db
    }

    #[test]
    fn catalogo_carrega_com_no_minimo_150_cartas_e_os_cinco_basicos() {
        let db = db();
        assert!(db.len() >= 155, "catálogo pequeno demais: {}", db.len());
        for basic in ["Plains", "Island", "Swamp", "Mountain", "Forest"] {
            let card = db.by_name(basic).unwrap_or_else(|| panic!("falta {basic}"));
            assert!(card.type_line.is_land(), "{basic} deveria ser terreno");
            assert_eq!(card.mana_abilities().count(), 1, "{basic} sem habilidade de mana");
        }
    }

    #[test]
    fn a_copia_embutida_e_igual_a_do_disco() {
        // Se divergirem, o servidor roda com um catálogo e o desenvolvedor
        // depura outro.
        let disco = db();
        let embutido = build_database_embedded().expect("catálogo embutido carrega");
        assert_eq!(disco.len(), embutido.len());
        assert!(
            !disco.cards.is_empty(),
            "catálogo vazio: a comparação carta a carta não afirmaria nada"
        );
        for (a, b) in disco.cards.iter().zip(embutido.cards.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn nenhum_nome_duplicado() {
        let db = db();
        let mut nomes: Vec<String> = db.cards.iter().map(|c| c.name.to_ascii_lowercase()).collect();
        nomes.sort();
        let antes = nomes.len();
        nomes.dedup();
        assert_eq!(antes, nomes.len(), "há nome de carta repetido");
    }

    #[test]
    fn toda_criatura_tem_poder_e_resistencia() {
        for card in &db().cards {
            if card.type_line.has_type(CardType::Creature) {
                assert!(card.power.is_some(), "{} sem poder", card.name);
                assert!(card.toughness.is_some(), "{} sem resistência", card.name);
            } else {
                assert!(card.power.is_none(), "{} não é criatura mas tem poder", card.name);
            }
        }
    }

    #[test]
    fn nao_criatura_permanente_nunca_tem_pt_e_terreno_nao_tem_custo() {
        for card in &db().cards {
            if card.type_line.is_land() {
                assert!(card.mana_cost.is_free(), "{} é terreno com custo", card.name);
            }
        }
    }

    /// Cada `ObjRef::Target(i)` precisa de um `TargetSpec` no mesmo escopo:
    /// mágica olha `spell_targets`, habilidade olha os alvos dela própria.
    /// Alvo sem especificação vira índice fora de faixa na resolução.
    #[test]
    fn todo_alvo_referenciado_tem_especificacao() {
        let mut checados = 0usize;
        for card in &db().cards {
            if let Some(effect) = &card.spell_effect {
                check(&card.name, "mágica", effect, card.spell_targets.len(), &mut checados);
            }
            for ability in &card.abilities {
                match ability {
                    Ability::Activated(a) => {
                        check(&card.name, "ativada", &a.effect, a.targets.len(), &mut checados)
                    }
                    Ability::Triggered(a) => {
                        check(&card.name, "disparada", &a.effect, a.targets.len(), &mut checados)
                    }
                    Ability::Replacement(a) => {
                        check(&card.name, "substituição", &a.replacement, 0, &mut checados)
                    }
                    _ => {}
                }
            }
        }
        assert!(
            checados > 0,
            "nenhum efeito do catálogo referencia alvo: o teste não afirmou nada"
        );

        fn check(
            card: &str,
            escopo: &str,
            effect: &Effect,
            declarados: usize,
            checados: &mut usize,
        ) {
            let Some(max) = effect.max_target_index() else {
                return;
            };
            *checados += 1;
            assert!(
                (max as usize) < declarados,
                "{card}: efeito de {escopo} usa alvo #{} mas só {declarados} foram declarados",
                max + 1
            );
        }
    }

    /// O inverso do teste acima: todo alvo declarado tem que ser usado. Alvo
    /// declarado e nunca referenciado obriga o jogador a escolher algo que não
    /// faz nada — e a IA gasta busca nisso.
    ///
    /// A varredura é feita sobre o JSON porque um alvo pode ser referenciado
    /// como `ObjRef::Target`, como `PlayerRef::Target` ou dentro do
    /// `owner_scope` de um seletor (o caso de `Sleep`). Os três serializam como
    /// `{"Target":i}`, então uma busca textual cobre tudo sem duplicar em Rust
    /// a travessia que o serde já faz.
    #[test]
    fn todo_alvo_declarado_e_usado() {
        let mut declarantes = 0usize;
        for card in &db().cards {
            if !card.spell_targets.is_empty() {
                declarantes += 1;
            }
            if !card.spell_targets.is_empty() {
                let effect = card
                    .spell_effect
                    .as_ref()
                    .unwrap_or_else(|| panic!("{}: declara alvo sem efeito", card.name));
                check(&card.name, "mágica", effect, card.spell_targets.len());
            }
            for ability in &card.abilities {
                match ability {
                    Ability::Activated(a) if !a.targets.is_empty() => {
                        check(&card.name, "ativada", &a.effect, a.targets.len())
                    }
                    Ability::Triggered(a) if !a.targets.is_empty() => {
                        check(&card.name, "disparada", &a.effect, a.targets.len())
                    }
                    _ => {}
                }
            }
        }
        assert!(
            declarantes > 0,
            "nenhuma mágica do catálogo declara alvo: o teste não afirmou nada"
        );

        fn check(card: &str, escopo: &str, effect: &Effect, declarados: usize) {
            let json = serde_json::to_string(effect).expect("efeito serializa");
            for i in 0..declarados {
                assert!(
                    json.contains(&format!("{{\"Target\":{i}}}")),
                    "{card}: efeito de {escopo} declara o alvo #{} mas nunca o referencia",
                    i + 1
                );
            }
        }
    }

    #[test]
    fn oracle_text_preenchido_em_carta_com_habilidade() {
        for card in &db().cards {
            if !card.abilities.is_empty() || card.spell_effect.is_some() {
                assert!(
                    !card.oracle_text.trim().is_empty(),
                    "{} tem regra mas oracle_text vazio",
                    card.name
                );
            }
            assert_eq!(
                card.art_key.as_deref(),
                Some(card.name.as_str()),
                "{}: art_key deve ser o nome exato da carta",
                card.name
            );
        }
    }
}
