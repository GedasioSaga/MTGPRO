//! Camada de script em Lua para definição de cartas.
//!
//! Papel do Lua aqui: **autoria**, não runtime. O script roda uma vez no
//! carregamento e produz a árvore de `Effect` (o IR de `mtg-core`). Durante a
//! partida o motor interpreta IR puro — nenhuma chamada de Lua no laço quente.
//! Assim escrever carta fica fácil sem custar performance nem determinismo.
//!
//! A ponte é o `serde`: a tabela Lua é desserializada direto em `CardDef` pelo
//! mesmo caminho que o banco usa. Não há código de conversão para manter.
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue};
use mtg_core::card::{CardDatabase, CardDef};
use mtg_core::mana::{Color, ManaSymbol};
use mtg_core::types::{CardType, Supertype, TypeLine};

/// Prelúdio em Lua com os construtores ergonômicos de carta e efeito.
const PRELUDE: &str = include_str!("../lua/prelude.lua");

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("erro de Lua em {file}: {source}")]
    Lua {
        file: String,
        #[source]
        source: mlua::Error,
    },
    #[error("carta inválida em {file}: {reason}")]
    InvalidCard { file: String, reason: String },
    #[error("erro de leitura em {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Interpretador Lua isolado, com o prelúdio já carregado.
pub struct CardScriptHost {
    lua: Lua,
    /// Diretórios varridos, guardados para permitir recarga a quente.
    roots: Vec<PathBuf>,
}

impl CardScriptHost {
    pub fn new() -> Result<CardScriptHost, ScriptError> {
        let lua = Lua::new();
        sandbox(&lua)?;
        register_builtins(&lua)?;
        lua.load(PRELUDE)
            .set_name("prelude.lua")
            .exec()
            .map_err(|source| ScriptError::Lua { file: "prelude.lua".into(), source })?;
        Ok(CardScriptHost { lua, roots: Vec::new() })
    }

    /// Executa um chunk avulso dentro do interpretador JÁ isolado por
    /// `sandbox` — sem `io`, `os`, `package`, `require` nem `load`, o script não
    /// alcança o sistema. É esse isolamento que torna seguro rodar arquivo de
    /// carta vindo de terceiro. Cartas declaradas com `card{...}` entram no
    /// registro interno e saem em `build_database`.
    pub fn eval(&self, source: &str, name: &str) -> Result<(), ScriptError> {
        self.lua
            .load(source)
            .set_name(name)
            .exec()
            .map_err(|source| ScriptError::Lua { file: name.to_string(), source })
    }

    /// Carrega todos os `.lua` de um diretório, em ordem alfabética para o
    /// resultado não depender da ordem do sistema de arquivos.
    pub fn load_dir(&mut self, dir: &Path) -> Result<usize, ScriptError> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|source| ScriptError::Io { path: dir.display().to_string(), source })?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "lua"))
            .collect();
        files.sort();

        let before = self.card_count()?;
        for path in &files {
            let src = std::fs::read_to_string(path)
                .map_err(|source| ScriptError::Io { path: path.display().to_string(), source })?;
            let name = path.file_name().map(|n| n.to_string_lossy().to_string());
            self.eval(&src, name.as_deref().unwrap_or("<script>"))?;
        }
        if !self.roots.contains(&dir.to_path_buf()) {
            self.roots.push(dir.to_path_buf());
        }
        Ok(self.card_count()? - before)
    }

    /// Recarrega tudo do zero. Serve para edição a quente durante o
    /// desenvolvimento: salvou o `.lua`, o catálogo muda sem reiniciar.
    pub fn reload(&mut self) -> Result<usize, ScriptError> {
        self.clear_registry()?;
        let roots = std::mem::take(&mut self.roots);
        let mut total = 0;
        for root in &roots {
            total += self.load_dir(root)?;
        }
        self.roots = roots;
        Ok(total)
    }

    fn registry(&self) -> Result<Table, ScriptError> {
        self.lua
            .globals()
            .get::<Table>("__cards")
            .map_err(|source| ScriptError::Lua { file: "prelude.lua".into(), source })
    }

    fn card_count(&self) -> Result<usize, ScriptError> {
        Ok(self.registry()?.raw_len())
    }

    fn clear_registry(&self) -> Result<(), ScriptError> {
        self.lua
            .load("__cards = {}")
            .exec()
            .map_err(|source| ScriptError::Lua { file: "reset".into(), source })
    }

    /// Converte o registro Lua em `CardDatabase`, já reindexado.
    pub fn build_database(&self) -> Result<CardDatabase, ScriptError> {
        let table = self.registry()?;
        let mut cards: Vec<CardDef> = Vec::with_capacity(table.raw_len());
        for (i, entry) in table.sequence_values::<LuaValue>().enumerate() {
            let value = entry.map_err(|source| ScriptError::Lua { file: "registro".into(), source })?;
            let card: CardDef = self.lua.from_value(value).map_err(|e| ScriptError::InvalidCard {
                file: format!("carta #{}", i + 1),
                reason: e.to_string(),
            })?;
            cards.push(card);
        }
        let mut db = CardDatabase { cards };
        db.reindex();
        Ok(db)
    }
}

/// Remove do ambiente tudo que dá acesso ao sistema. Script de carta é
/// conteúdo, e conteúdo eventualmente vem de terceiro — o sandbox é requisito,
/// não zelo. `load`/`loadstring` saem junto: sem eles não há como reconstruir
/// as bibliotecas removidas a partir de bytecode.
fn sandbox(lua: &Lua) -> Result<(), ScriptError> {
    let globals = lua.globals();
    for name in [
        "io", "os", "package", "require", "dofile", "loadfile", "load", "loadstring", "debug",
        "collectgarbage", "rawset", "rawget", "setmetatable", "getmetatable",
    ] {
        globals
            .set(name, LuaValue::Nil)
            .map_err(|source| ScriptError::Lua { file: "sandbox".into(), source })?;
    }
    Ok(())
}

/// Funções implementadas em Rust e expostas ao Lua. Parsing fica aqui porque
/// erro de custo de mana precisa de mensagem boa, e Lua não dá isso barato.
fn register_builtins(lua: &Lua) -> Result<(), ScriptError> {
    let globals = lua.globals();
    let err = |source| ScriptError::Lua { file: "builtins".into(), source };

    // mana("{2}{W}{W}") -> tabela desserializável em ManaCost
    let mana = lua
        .create_function(|lua, spec: String| {
            let symbols = parse_mana(&spec)
                .map_err(|e| mlua::Error::RuntimeError(format!("custo de mana inválido {spec:?}: {e}")))?;
            lua.to_value(&symbols)
        })
        .map_err(err)?;
    globals.set("mana", mana).map_err(err)?;

    // typeline("Legendary Creature — Human Soldier") -> tabela TypeLine
    let typeline = lua
        .create_function(|lua, spec: String| {
            let tl = parse_type_line(&spec)
                .map_err(|e| mlua::Error::RuntimeError(format!("linha de tipo inválida {spec:?}: {e}")))?;
            lua.to_value(&tl)
        })
        .map_err(err)?;
    globals.set("typeline", typeline).map_err(err)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Aceita `{2}{W}{W}`, `{X}{R}`, `{W/U}`, `{2/W}`, `{W/P}`, `{C}`, `{S}`.
/// Também aceita a forma sem chaves para custo simples: `2WW`.
pub fn parse_mana(spec: &str) -> Result<Vec<ManaSymbol>, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();

    if !spec.contains('{') {
        // Forma curta: dígitos viram genérico, letras viram cor.
        let mut digits = String::new();
        for ch in spec.chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
                continue;
            }
            if !digits.is_empty() {
                out.push(ManaSymbol::Generic(digits.parse::<u8>().map_err(|e| e.to_string())?));
                digits.clear();
            }
            match ch.to_ascii_uppercase() {
                'X' => out.push(ManaSymbol::X),
                'C' => out.push(ManaSymbol::Colorless),
                'S' => out.push(ManaSymbol::Snow),
                c => out.push(ManaSymbol::Colored(
                    Color::from_letter(c).ok_or_else(|| format!("símbolo desconhecido {c:?}"))?,
                )),
            }
        }
        if !digits.is_empty() {
            out.push(ManaSymbol::Generic(digits.parse::<u8>().map_err(|e| e.to_string())?));
        }
        return Ok(out);
    }

    for raw in spec.split('}') {
        let token = raw.trim_start_matches('{').trim();
        if token.is_empty() {
            continue;
        }
        out.push(parse_symbol(token)?);
    }
    Ok(out)
}

fn parse_symbol(token: &str) -> Result<ManaSymbol, String> {
    if let Ok(n) = token.parse::<u8>() {
        return Ok(ManaSymbol::Generic(n));
    }
    if let Some((a, b)) = token.split_once('/') {
        // {2/W} monocolor híbrido, {W/P} phyrexiano, {W/U} híbrido.
        if a == "2" {
            let c = single_color(b)?;
            return Ok(ManaSymbol::MonoHybrid(c));
        }
        if b.eq_ignore_ascii_case("P") {
            return Ok(ManaSymbol::Phyrexian(single_color(a)?));
        }
        return Ok(ManaSymbol::Hybrid(single_color(a)?, single_color(b)?));
    }
    match token.to_ascii_uppercase().as_str() {
        "X" => Ok(ManaSymbol::X),
        "C" => Ok(ManaSymbol::Colorless),
        "S" => Ok(ManaSymbol::Snow),
        other => single_color(other).map(ManaSymbol::Colored),
    }
}

fn single_color(token: &str) -> Result<Color, String> {
    token
        .chars()
        .next()
        .and_then(Color::from_letter)
        .ok_or_else(|| format!("cor desconhecida {token:?}"))
}

/// Aceita travessão (—), hífen ou " - " separando tipos de subtipos.
pub fn parse_type_line(spec: &str) -> Result<TypeLine, String> {
    let normalized = spec.replace('—', "|").replace(" - ", "|");
    let (left, right) = match normalized.split_once('|') {
        Some((l, r)) => (l.trim().to_string(), r.trim().to_string()),
        None => (normalized.trim().to_string(), String::new()),
    };

    let mut tl = TypeLine::default();
    for word in left.split_whitespace() {
        if let Some(sup) = parse_supertype(word) {
            tl.supertypes.push(sup);
            continue;
        }
        let ct = parse_card_type(word)
            .ok_or_else(|| format!("tipo desconhecido {word:?} em {spec:?}"))?;
        tl.types.push(ct);
    }
    if tl.types.is_empty() {
        return Err(format!("linha de tipo sem tipo principal: {spec:?}"));
    }
    tl.subtypes = right.split_whitespace().map(|s| s.to_string()).collect();
    Ok(tl)
}

fn parse_supertype(word: &str) -> Option<Supertype> {
    match word.to_ascii_lowercase().as_str() {
        "basic" => Some(Supertype::Basic),
        "legendary" => Some(Supertype::Legendary),
        "snow" => Some(Supertype::Snow),
        "world" => Some(Supertype::World),
        _ => None,
    }
}

fn parse_card_type(word: &str) -> Option<CardType> {
    match word.to_ascii_lowercase().as_str() {
        "artifact" => Some(CardType::Artifact),
        "battle" => Some(CardType::Battle),
        "creature" => Some(CardType::Creature),
        "enchantment" => Some(CardType::Enchantment),
        "instant" => Some(CardType::Instant),
        "land" => Some(CardType::Land),
        "planeswalker" => Some(CardType::Planeswalker),
        "sorcery" => Some(CardType::Sorcery),
        "kindred" | "tribal" => Some(CardType::Kindred),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custo_com_chaves_e_sem_chaves_dao_o_mesmo() {
        assert_eq!(parse_mana("{2}{W}{W}").unwrap(), parse_mana("2WW").unwrap());
    }

    #[test]
    fn hibrido_phyrexiano_e_monohibrido() {
        assert_eq!(parse_symbol("W/U").unwrap(), ManaSymbol::Hybrid(Color::White, Color::Blue));
        assert_eq!(parse_symbol("W/P").unwrap(), ManaSymbol::Phyrexian(Color::White));
        assert_eq!(parse_symbol("2/W").unwrap(), ManaSymbol::MonoHybrid(Color::White));
    }

    #[test]
    fn linha_de_tipo_com_travessao() {
        let tl = parse_type_line("Legendary Creature — Human Soldier").unwrap();
        assert_eq!(tl.supertypes, vec![Supertype::Legendary]);
        assert_eq!(tl.types, vec![CardType::Creature]);
        assert_eq!(tl.subtypes, vec!["Human".to_string(), "Soldier".to_string()]);
    }

    #[test]
    fn sandbox_remove_acesso_ao_sistema() {
        let host = CardScriptHost::new().unwrap();
        assert!(host.eval("return io.open('x')", "t").is_err());
        assert!(host.eval("return os.getenv('PATH')", "t").is_err());
        assert!(host.eval("return require('socket')", "t").is_err());
    }

    #[test]
    fn carta_de_lua_vira_carddef() {
        let host = CardScriptHost::new().unwrap();
        host.eval(
            r#"
            card {
              name = "Grizzly Bears",
              cost = "{1}{G}",
              type = "Creature — Bear",
              pt = { 2, 2 },
              rarity = "Common",
              set = "LEA",
              text = "",
            }
            "#,
            "teste.lua",
        )
        .unwrap();
        let db = host.build_database().unwrap();
        assert_eq!(db.len(), 1);
        let bears = db.by_name("Grizzly Bears").expect("carta registrada");
        assert_eq!(bears.power, Some(2));
        assert_eq!(bears.mana_value(), 2);
        assert!(bears.type_line.has_subtype("Bear"));
    }
}
