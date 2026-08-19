//! Compilador de texto de oráculo para o IR de `mtg-core`.
//!
//! A regra que governa tudo: **ou a carta vira IR fiel, ou ela não é jogável.**
//! Não existe compilação parcial — importar metade de "Wrath of God" seria pior
//! que não importar, porque o bot jogaria a carta errada e o log de partida
//! mentiria. Por isso `compile_card` devolve `playable: false` com o motivo
//! textual assim que um parágrafo do texto não couber no IR.
//!
//! O motivo é o produto secundário mais útil daqui: agregado, ele diz qual
//! construção de texto compraria mais cartas jogáveis por hora de trabalho.
use mtg_core::card::{
    Ability, ActivatedAbility, CardDef, ManaAbility, ManaProduction, ReplacementAbility,
    ReplacementEvent, StaticAbility, StaticMod, TriggerCondition, TriggeredAbility,
};
use mtg_core::ids::CardDefId;
use mtg_core::ir::{
    Condition, Cost, Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind,
    TargetSpec, TimingRestriction, TokenSpec, Value, ZoneScope,
};
use mtg_core::mana::{Color, ManaCost, ManaSymbol};
use mtg_core::types::{CardType, CounterKind, Supertype, TypeLine};

use crate::parse::{
    color_letters, parse_color_set, parse_mana_cost, parse_printed_number,
    parse_rarity, parse_type_line, parse_word_number, Parsed, Unsupported,
};
use crate::scryfall::ScryfallCard;
use mtg_oracle::layouts::{self, LayoutVerdict};

/// Resultado da compilação de uma carta.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub def: CardDef,
    /// `true` só quando todo o texto virou IR.
    pub playable: bool,
    /// O que impediu a carta de ser jogável. `None` quando `playable`.
    pub reason: Option<String>,
    /// Texto de oráculo normalizado que travou a compilação, quando o
    /// bloqueio foi textual. É o insumo do relatório de cobertura: agregado,
    /// diz qual construção compraria mais cartas jogáveis por hora de
    /// trabalho. `None` quando o bloqueio foi estrutural (layout, duas faces).
    pub pattern: Option<String>,
    /// Cores da identidade, em ordem WUBRG, para índice no banco.
    pub color_identity: String,
    pub colors: String,
    /// `true` quando o comportamento veio da segunda passada
    /// (`mtg_oracle::compile`) e não do compilador deste crate. Existe para
    /// que o relatório possa dizer quantas cartas cada compilador comprou, e
    /// para que a amostra de fidelidade saiba exatamente o que auditar.
    pub second_pass: bool,
}

/// Escolhe a face que o compilador lê e diz se o layout tem representação fiel
/// no IR.
///
/// O problema de layout volta na frente de qualquer outro porque ele torna
/// irrelevante o que o texto diga: um `split` continuaria fora do jogo mesmo
/// com as duas metades compilando. `None` significa "siga com esta face".
fn layout_gate(card: &ScryfallCard) -> (Option<usize>, Option<Unsupported>) {
    let layout = card.layout.as_deref().unwrap_or("normal");
    let plan = layouts::plan_for(layout, card.face_count());
    let problem = match plan.verdict {
        LayoutVerdict::Blocked(why) => Some(Unsupported::new(why)),
        LayoutVerdict::Unknown => Some(Unsupported::new(format!("layout desconhecido '{layout}'"))),
        // Carta de uma face: não há outra face de que depender.
        LayoutVerdict::Compile if card.face_count() < 2 => None,
        // Mais de uma face, e só a frontal é lançável. Ela vale por si apenas
        // quando não fala da outra — nem por texto, nem por regra de tipo.
        LayoutVerdict::Compile => {
            let text = card.face_oracle_text(plan.face);
            let type_line = card.face_type_line(plan.face).unwrap_or("");
            layouts::references_other_face(text)
                .or_else(|| layouts::type_depends_on_other_face(type_line))
                .map(|mark| {
                    Unsupported::new(format!(
                        "layout '{layout}': a face frontal depende da outra ('{mark}')"
                    ))
                })
        }
    };
    (plan.face, problem)
}

pub fn compile_card(card: &ScryfallCard, id: u32) -> Compiled {
    // O nome da CARTA é a identidade no catálogo e a chave única da tabela, e
    // continua sendo o da raiz: "Delver of Secrets // Insectile Aberration".
    // O nome da FACE tem outro papel — é como o texto de oráculo fala de si, e
    // é ele que vira `~` na normalização. Trocar um pelo outro deixaria a
    // autorreferência sem casar em toda carta de duas faces.
    let name = card.name.clone().unwrap_or_default();
    let mut problems: Vec<Unsupported> = Vec::new();

    let (face, layout_problem) = layout_gate(card);
    let compiles_from_face = layout_problem.is_none();
    if let Some(what) = layout_problem {
        problems.push(what);
    }
    let face_name = card.face_name(face).to_string();

    let type_line = match card.face_type_line(face).map(parse_type_line) {
        Some(Ok(t)) => t,
        Some(Err(what)) => {
            problems.push(what);
            TypeLine::default()
        }
        None => {
            problems.push(Unsupported::new("sem linha de tipo"));
            TypeLine::default()
        }
    };

    let mana_cost = match card.face_mana_cost(face).map(parse_mana_cost) {
        Some(Ok(c)) => c,
        Some(Err(what)) => {
            problems.push(what);
            ManaCost::FREE
        }
        None => ManaCost::FREE,
    };

    let power = printed(card.face_power(face), &mut problems);
    let toughness = printed(card.face_toughness(face), &mut problems);
    let loyalty = printed(card.face_loyalty(face), &mut problems);

    // O texto que o compilador LÊ é sempre o da face escolhida. O texto que o
    // catálogo GUARDA é o da carta inteira quando o layout foi barrado: aí não
    // há IR nenhuma com que ser coerente, e mostrar meia carta na tela seria
    // esconder do jogador metade do que ele tem na mão.
    let read_text = card.face_oracle_text(face).to_string();
    let oracle_text = if compiles_from_face { read_text.clone() } else { all_faces_text(card) };
    let mut abilities = Vec::new();
    let mut spell_effect = None;
    let mut spell_targets = Vec::new();
    let mut second_pass = false;

    match compile_text(&read_text, &face_name, &type_line) {
        Ok(out) => {
            abilities = out.abilities;
            spell_effect = out.spell_effect;
            spell_targets = out.spell_targets;
        }
        // Segunda passada: `mtg-oracle` é o outro compilador do repositório e
        // reconhece famílias de texto que este aqui não tem. Ele só entra
        // quando NADA MAIS reprovou a carta — layout, linha de tipo, custo e
        // P/T já foram julgados acima e continuam valendo. Assim a segunda
        // passada só pode transformar `Unsupported` em jogável, nunca o
        // contrário: nenhuma carta que já compilava muda de IR por causa dela.
        Err(what) => match oracle_second_pass(
            card,
            face,
            &face_name,
            &type_line,
            problems.is_empty(),
        ) {
            Some(out) => {
                abilities = out.abilities;
                spell_effect = out.spell_effect;
                spell_targets = out.spell_targets;
                second_pass = true;
            }
            None => problems.push(what),
        },
    }

    let colors = card.face_colors(face).map(|c| parse_color_set(c)).unwrap_or_default();
    let identity = card.color_identity.as_ref().map(|c| parse_color_set(c)).unwrap_or_default();
    // O indicador de cor só é necessário quando a cor impressa não sai do
    // custo — carta incolor de custo colorido não existe, mas Devoid sim.
    let color_override = if colors != mana_cost.colors() { Some(colors) } else { None };

    let def = CardDef {
        id: CardDefId(id),
        name,
        mana_cost,
        type_line,
        color_override,
        power,
        toughness,
        loyalty,
        abilities,
        spell_effect,
        spell_targets,
        oracle_text,
        flavor_text: None,
        rarity: parse_rarity(card.rarity.as_deref()),
        set_code: card.set.clone().unwrap_or_default(),
        collector_number: card.collector_number.clone().unwrap_or_default(),
        artist: card.face_artist(face).map(|s| s.to_string()),
        art_key: card.face_image(face, "normal").map(|s| s.to_string()),
    };

    let playable = problems.is_empty();
    // Motivo e padrão saem do MESMO problema — o primeiro. Se o que bloqueia
    // primeiro é estrutural (duas faces), o padrão textual não conta: mesmo
    // compilando o texto, a carta continuaria fora.
    let first = problems.into_iter().next();
    let (reason, pattern) = match first {
        Some(u) => (Some(u.reason), u.snippet),
        None => (None, None),
    };
    Compiled {
        def,
        playable,
        reason,
        pattern,
        color_identity: color_letters(identity),
        colors: color_letters(colors),
        second_pass: playable && second_pass,
    }
}

/// Segunda passada de compilação de texto, delegada a `mtg_oracle::compile`.
///
/// Os dois compiladores nasceram separados e cobrem famílias de texto
/// diferentes; medido sobre o bulk inteiro, o de `mtg-oracle` aceita cartas que
/// este não aceita. Em vez de duplicar o vocabulário, a carta que este
/// compilador reprovou **por texto** é reoferecida àquele.
///
/// `clean` é a condição de segurança: só há delegação quando nenhum outro
/// problema foi registrado. Carta barrada por layout, por linha de tipo, por
/// custo ou por P/T variável continua barrada, mesmo que o texto da face
/// compile — senão "Fire // Ice" entraria no jogo como metade de si mesma.
///
/// O `CardDef` devolvido por `mtg-oracle` é usado **apenas** pelos três campos
/// de comportamento. Nome, cores, arte e identidade continuam vindo daqui, que
/// é quem enxerga a carta multiface inteira.
fn oracle_second_pass(
    card: &ScryfallCard,
    face: Option<usize>,
    face_name: &str,
    importer_type_line: &TypeLine,
    clean: bool,
) -> Option<CompiledText> {
    if !clean {
        return None;
    }
    let oracle_card = mtg_oracle::OracleCard {
        name: face_name.to_string(),
        mana_cost: card.face_mana_cost(face).unwrap_or_default().to_string(),
        type_line: card.face_type_line(face).unwrap_or_default().to_string(),
        oracle_text: card.face_oracle_text(face).to_string(),
        power: card.face_power(face).map(str::to_string),
        toughness: card.face_toughness(face).map(str::to_string),
        loyalty: card.face_loyalty(face).map(str::to_string),
        rarity: card.rarity.clone().unwrap_or_default(),
        set_code: card.set.clone().unwrap_or_default(),
        collector_number: card.collector_number.clone().unwrap_or_default(),
        artist: card.face_artist(face).map(str::to_string),
        flavor_text: None,
        art_key: None,
        layout: card.layout.clone().unwrap_or_default(),
    };
    let def = mtg_oracle::compile(&oracle_card).card()?.clone();
    // Os dois compiladores leem a linha de tipo por conta propria, e e ela que
    // decide se o texto vira efeito de magica ou habilidade de permanente.
    // Discordando, o comportamento importado ficaria pendurado no tipo errado
    // — feitico virando criatura muda. Divergiu, nao entra.
    if def.type_line != *importer_type_line {
        return None;
    }
    Some(CompiledText {
        abilities: def.abilities,
        spell_effect: def.spell_effect,
        spell_targets: def.spell_targets,
    })
}

/// Texto de oráculo da carta INTEIRA, para o catálogo de uma carta cujo layout
/// foi barrado.
///
/// Sem isto "Fire // Ice" aparecia na tela só como "Fire deals 2 damage divided
/// as you choose": o jogador via metade da carta sem ter como saber que faltava
/// a outra. Cada face entra com nome e custo, na ordem em que o Scryfall
/// entrega, separadas pelo mesmo `//` impresso na carta.
///
/// Carta de uma face cai no texto da raiz — não há nada a juntar.
fn all_faces_text(card: &ScryfallCard) -> String {
    if card.face_count() < 2 {
        return card.face_oracle_text(None).to_string();
    }
    let mut out = String::new();
    for i in 0..card.face_count() {
        if i > 0 {
            out.push_str("\n//\n");
        }
        let head = match card.face_mana_cost(Some(i)).filter(|c| !c.is_empty()) {
            Some(cost) => format!("{} {cost}", card.face_name(Some(i))),
            None => card.face_name(Some(i)).to_string(),
        };
        out.push_str(head.trim());
        if let Some(t) = card.face_type_line(Some(i)).filter(|t| !t.is_empty()) {
            out.push('\n');
            out.push_str(t);
        }
        let body = card.face_oracle_text(Some(i));
        if !body.is_empty() {
            out.push('\n');
            out.push_str(body);
        }
    }
    out
}

fn printed(text: Option<&str>, problems: &mut Vec<Unsupported>) -> Option<i32> {
    match text {
        None => None,
        Some(t) => match parse_printed_number(t) {
            Ok(n) => Some(n),
            Err(what) => {
                problems.push(what);
                None
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Normalização do texto
// ---------------------------------------------------------------------------

/// Remove texto-lembrete entre parênteses. Conta aninhamento porque existe
/// lembrete dentro de lembrete.
fn strip_reminders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Perífrases que o oráculo moderno usa no lugar do nome próprio. Desde 2023 a
/// carta fala de si como "this creature"; sem esta troca, um terço do catálogo
/// vira texto irreconhecível.
const SELF_PHRASES: [&str; 10] = [
    "this creature",
    "this permanent",
    "this land",
    "this artifact",
    "this enchantment",
    "this planeswalker",
    "this Equipment",
    "this Vehicle",
    "this Aura",
    "this Saga",
];

/// Palavras de habilidade: prefixo puramente decorativo (CR 207.2c). Só a lista
/// fechada é removida — um travessão qualquer no começo pode ser modal, e comer
/// isso silenciosamente daria carta errada.
const ABILITY_WORDS: [&str; 24] = [
    "landfall",
    "metalcraft",
    "threshold",
    "delirium",
    "constellation",
    "revolt",
    "raid",
    "ferocious",
    "formidable",
    "morbid",
    "battalion",
    "heroic",
    "spell mastery",
    "enrage",
    "addendum",
    "adamant",
    "undergrowth",
    "magecraft",
    "alliance",
    "corrupted",
    "descend",
    "domain",
    "valiant",
    "pack tactics",
];

/// Troca o nome próprio por `~`. O oráculo usa o nome completo e também o nome
/// curto (antes da vírgula) para lendárias: "Serra, the Benevolent" fala de si
/// como "Serra".
fn substitute_name(text: &str, name: &str) -> String {
    let mut out = text.to_string();
    if !name.is_empty() {
        out = out.replace(name, "~");
        if let Some((short, _)) = name.split_once(", ") {
            if !short.is_empty() {
                out = out.replace(short, "~");
            }
        }
    }
    for phrase in SELF_PHRASES {
        out = out.replace(phrase, "~");
        out = out.replace(&capitalize(phrase), "~");
    }
    out
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Remove a palavra de habilidade do começo do parágrafo, se houver.
fn strip_ability_word(par: &str) -> &str {
    let Some((head, tail)) = par.split_once(" \u{2014} ") else {
        return par;
    };
    if ABILITY_WORDS.contains(&head.to_lowercase().as_str()) {
        return tail.trim();
    }
    par
}

fn paragraphs(text: &str, name: &str) -> Vec<String> {
    substitute_name(&strip_reminders(text), name)
        .split('\n')
        .map(|p| strip_ability_word(p.trim()).trim_end_matches('.').trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Coletor de alvos
// ---------------------------------------------------------------------------

/// Acumula os alvos exigidos por uma habilidade, na ordem em que aparecem no
/// texto. O índice devolvido é o que o IR usa em `ObjRef::Target`.
#[derive(Debug, Default)]
struct Targets {
    specs: Vec<TargetSpec>,
}

impl Targets {
    fn push(&mut self, kind: TargetKind, description: &str) -> Parsed<u8> {
        let index = u8::try_from(self.specs.len())
            .map_err(|_| Unsupported::new("habilidade com alvos demais"))?;
        self.specs.push(TargetSpec { kind, description: description.to_string() });
        Ok(index)
    }
    fn object(&mut self, selector: Selector, description: &str) -> Parsed<ObjRef> {
        let i = self.push(TargetKind::Object(selector), description)?;
        Ok(ObjRef::Target(i))
    }
    fn player(&mut self, who: PlayerRef, description: &str) -> Parsed<PlayerRef> {
        let i = self.push(TargetKind::Player(who), description)?;
        Ok(PlayerRef::Target(i))
    }
    fn any_target(&mut self, description: &str) -> Parsed<ObjRef> {
        let objects = Selector::battlefield(Filter::Or(vec![
            Filter::HasType(CardType::Creature),
            Filter::HasType(CardType::Planeswalker),
        ]));
        let i = self.push(TargetKind::ObjectOrPlayer(objects, PlayerRef::Each), description)?;
        Ok(ObjRef::Target(i))
    }
    fn spell(&mut self, filter: Filter, description: &str) -> Parsed<ObjRef> {
        let i = self.push(TargetKind::SpellOnStack(filter), description)?;
        Ok(ObjRef::Target(i))
    }
}

// ---------------------------------------------------------------------------
// Compilação do texto inteiro
// ---------------------------------------------------------------------------

struct CompiledText {
    abilities: Vec<Ability>,
    spell_effect: Option<Effect>,
    spell_targets: Vec<TargetSpec>,
}

fn compile_text(text: &str, name: &str, type_line: &TypeLine) -> Parsed<CompiledText> {
    let is_spell = type_line.types.iter().all(|t| t.is_spell_only()) && !type_line.types.is_empty();
    let mut abilities = intrinsic_abilities(type_line);
    let mut spell_effects: Vec<Effect> = Vec::new();
    let mut spell_targets = Targets::default();

    let pars = paragraphs(text, name);
    let mut i = 0usize;
    while i < pars.len() {
        let par = &pars[i];
        let lower = par.to_lowercase();
        i += 1;
        if let Some(kws) = parse_keyword_line(&lower) {
            abilities.extend(kws.into_iter().map(Ability::Keyword));
            continue;
        }
        if let Some(choose) = modal_header(&lower) {
            let (effect, consumed) = parse_modal(choose, &pars[i..])?;
            i += consumed;
            if is_spell {
                spell_effects.push(effect);
            } else {
                return Err(Unsupported::new("modal fora de mágica"));
            }
            continue;
        }
        if is_spell {
            spell_effects.push(parse_effect_block(&lower, &mut spell_targets)?);
        } else {
            abilities.extend(parse_permanent_paragraph(&lower, par)?);
        }
    }

    let spell_effect = match spell_effects.len() {
        0 => None,
        1 => spell_effects.pop(),
        _ => Some(Effect::Sequence(spell_effects)),
    };
    if is_spell && spell_effect.is_none() {
        return Err(Unsupported::new("mágica sem efeito reconhecido"));
    }
    Ok(CompiledText { abilities, spell_effect, spell_targets: spell_targets.specs })
}

/// "Choose one —" e "Choose two —". `None` quando não é cabeçalho de modal.
fn modal_header(lower: &str) -> Option<u8> {
    let head = lower.trim_end_matches(['\u{2014}', '-', ' ']).trim();
    match head {
        "choose one" => Some(1),
        "choose two" => Some(2),
        "choose three" => Some(3),
        _ => None,
    }
}

/// Monta o `Modal` a partir das linhas de bala que vêm depois do cabeçalho.
///
/// Modo com alvo não passa: `CardDef` guarda uma lista de alvos por habilidade,
/// não por modo, então "destrua alvo de artefato **ou** alvo de encantamento"
/// viraria "escolha os dois alvos". Isso é infidelidade silenciosa, e o preço
/// dela é o bot jogando a carta errada — melhor ficar como não jogável.
fn parse_modal(choose: u8, rest: &[String]) -> Parsed<(Effect, usize)> {
    let mut options = Vec::new();
    let mut consumed = 0usize;
    for par in rest {
        let Some(body) = par.trim().strip_prefix('\u{2022}') else {
            break;
        };
        consumed += 1;
        let body = body.trim();
        let mut targets = Targets::default();
        let effect = parse_effect_block(&body.to_lowercase(), &mut targets)?;
        if !targets.specs.is_empty() {
            return Err(Unsupported::new("modal com alvo por modo"));
        }
        options.push((body.to_string(), effect));
    }
    if options.len() < choose as usize {
        return Err(Unsupported::new("modal sem modos suficientes"));
    }
    Ok((Effect::Modal { choose, options }, consumed))
}

/// Habilidade que não está escrita na carta porque as regras a dão de graça:
/// terreno básico produz o mana do seu subtipo (CR 305.6).
fn intrinsic_abilities(type_line: &TypeLine) -> Vec<Ability> {
    // CR 305.6 amarra a habilidade ao SUBTIPO, não ao supertipo Basic: Dryad
    // Arbor e os duais originais também viram mana sem ter texto impresso.
    if !type_line.is_land() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for subtype in &type_line.subtypes {
        let symbol = match subtype.as_str() {
            "Plains" => ManaSymbol::Colored(Color::White),
            "Island" => ManaSymbol::Colored(Color::Blue),
            "Swamp" => ManaSymbol::Colored(Color::Black),
            "Mountain" => ManaSymbol::Colored(Color::Red),
            "Forest" => ManaSymbol::Colored(Color::Green),
            "Wastes" => ManaSymbol::Colorless,
            _ => continue,
        };
        out.push(Ability::Mana(ManaAbility {
            cost: Cost::Tap,
            production: ManaProduction::Fixed(vec![symbol]),
            restriction: Condition::Always,
            text: format!("{{T}}: Add mana of {subtype}"),
        }));
    }
    out
}

// ---------------------------------------------------------------------------
// Palavras-chave
// ---------------------------------------------------------------------------

/// Uma linha só de palavras-chave separadas por vírgula. `None` quando qualquer
/// item não é palavra-chave — meio reconhecido não serve.
fn parse_keyword_line(lower: &str) -> Option<Vec<Keyword>> {
    if lower.is_empty() || lower.contains('\n') {
        return None;
    }
    let mut out = Vec::new();
    for part in lower.split(", ") {
        out.push(parse_keyword(part.trim())?);
    }
    Some(out)
}

fn parse_keyword(s: &str) -> Option<Keyword> {
    let simple = match s {
        "flying" => Keyword::Flying,
        "reach" => Keyword::Reach,
        "trample" => Keyword::Trample,
        "first strike" => Keyword::FirstStrike,
        "double strike" => Keyword::DoubleStrike,
        "deathtouch" => Keyword::Deathtouch,
        "lifelink" => Keyword::Lifelink,
        "vigilance" => Keyword::Vigilance,
        "haste" => Keyword::Haste,
        "menace" => Keyword::Menace,
        "defender" => Keyword::Defender,
        "flash" => Keyword::Flash,
        "hexproof" => Keyword::Hexproof,
        "shroud" => Keyword::Shroud,
        "indestructible" => Keyword::Indestructible,
        "prowess" => Keyword::Prowess,
        "intimidate" => Keyword::Intimidate,
        "fear" => Keyword::Fear,
        "skulk" => Keyword::Skulk,
        "exalted" => Keyword::Exalted,
        "riot" => Keyword::Riot,
        "convoke" => Keyword::Convoke,
        "delve" => Keyword::Delve,
        "cascade" => Keyword::Cascade,
        "storm" => Keyword::Storm,
        _ => return parse_parametric_keyword(s),
    };
    Some(simple)
}

fn parse_parametric_keyword(s: &str) -> Option<Keyword> {
    if let Some(color) = s.strip_prefix("protection from ") {
        return color_by_name(color).map(Keyword::Protection);
    }
    for (prefix, build) in [
        ("ward ", &(Keyword::Ward) as &dyn Fn(Box<Cost>) -> Keyword),
        ("flashback ", &Keyword::Flashback),
        ("kicker ", &Keyword::Kicker),
        ("cycling ", &Keyword::Cycling),
        ("equip ", &Keyword::Equip),
    ] {
        if let Some(cost) = s.strip_prefix(prefix) {
            return parse_mana_cost(cost).ok().map(|c| build(Box::new(Cost::Mana(c.symbols))));
        }
    }
    if let Some(what) = s.strip_prefix("enchant ") {
        return parse_filter_phrase(what).ok().map(|f| Keyword::Enchant(Box::new(f.filter)));
    }
    if let Some(n) = s.strip_prefix("annihilator ") {
        return parse_word_number(n).and_then(|v| u8::try_from(v).ok()).map(Keyword::Annihilator);
    }
    if let Some(n) = s.strip_prefix("afflict ") {
        return parse_word_number(n).and_then(|v| u8::try_from(v).ok()).map(Keyword::Afflict);
    }
    for (land, walk) in
        [("plains", "Plains"), ("island", "Island"), ("swamp", "Swamp"), ("mountain", "Mountain"), ("forest", "Forest")]
    {
        if s == format!("{land}walk") {
            return Some(Keyword::Landwalk(walk.to_string()));
        }
    }
    None
}

fn color_by_name(name: &str) -> Option<Color> {
    match name.trim() {
        "white" => Some(Color::White),
        "blue" => Some(Color::Blue),
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Parágrafo de permanente
// ---------------------------------------------------------------------------

fn parse_permanent_paragraph(lower: &str, original: &str) -> Parsed<Vec<Ability>> {
    if let Some(a) = parse_mana_ability(lower, original)? {
        return Ok(vec![a]);
    }
    if let Some(a) = parse_triggered(lower, original)? {
        return Ok(vec![a]);
    }
    if let Some(a) = parse_activated(lower, original)? {
        return Ok(vec![a]);
    }
    if let Some(a) = parse_static(lower, original)? {
        return Ok(vec![a]);
    }
    if let Some(a) = parse_replacement(lower, original) {
        return Ok(vec![a]);
    }
    Err(Unsupported::text(format!("texto '{}'", trim_for_reason(lower)), lower))
}

/// Trecho curto e estável do texto que não compilou. Serve para agrupar
/// motivos; texto inteiro faria um bucket por carta e não agregaria nada.
fn trim_for_reason(text: &str) -> String {
    let mut out: String = text.split_whitespace().take(6).collect::<Vec<_>>().join(" ");
    if out.len() > 60 {
        out.truncate(60);
    }
    out
}

// ---- habilidade de mana -----------------------------------------------------

fn parse_mana_ability(lower: &str, original: &str) -> Parsed<Option<Ability>> {
    let Some((cost_part, effect_part)) = split_activation(lower) else {
        return Ok(None);
    };
    let Some(rest) = effect_part.strip_prefix("add ") else {
        return Ok(None);
    };
    let cost = match parse_cost(cost_part) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let production = match parse_mana_production(rest) {
        Some(p) => p,
        None => return Ok(None),
    };
    Ok(Some(Ability::Mana(ManaAbility {
        cost,
        production,
        restriction: Condition::Always,
        text: original.to_string(),
    })))
}

fn parse_mana_production(rest: &str) -> Option<ManaProduction> {
    let rest = rest.trim();
    if rest == "one mana of any color" {
        return Some(ManaProduction::AnyColor(1));
    }
    if rest == "two mana of any one color" || rest == "two mana of any color" {
        return Some(ManaProduction::AnyColor(2));
    }
    if let Some(list) = rest.split(" or ").collect::<Vec<_>>().split_first() {
        let (first, others) = list;
        if !others.is_empty() {
            let mut symbols = Vec::new();
            for part in std::iter::once(first).chain(others.iter()) {
                let cost = parse_mana_cost(part).ok()?;
                if cost.symbols.len() != 1 {
                    return None;
                }
                symbols.extend(cost.symbols);
            }
            return Some(ManaProduction::OneOf(symbols));
        }
    }
    let cost = parse_mana_cost(rest).ok()?;
    if cost.symbols.is_empty() {
        return None;
    }
    Some(ManaProduction::Fixed(cost.symbols))
}

/// Separa "custo: efeito" quando o texto realmente é uma ativação. O dois
/// pontos só conta quando vem antes da primeira frase.
fn split_activation(lower: &str) -> Option<(&str, &str)> {
    let colon = lower.find(": ")?;
    let sentence_end = lower.find(". ").unwrap_or(lower.len());
    if colon > sentence_end {
        return None;
    }
    Some((lower[..colon].trim(), lower[colon + 2..].trim()))
}

// ---- habilidade ativada -----------------------------------------------------

fn parse_activated(lower: &str, original: &str) -> Parsed<Option<Ability>> {
    let Some((cost_part, effect_part)) = split_activation(lower) else {
        return Ok(None);
    };
    let cost = match parse_cost(cost_part) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let mut targets = Targets::default();
    let (body, timing) = match effect_part.split_once(". activate only as a sorcery") {
        // Só a restrição de tempo isolada é representável; "e só uma vez por
        // turno" continuaria escrito na carta e ausente do IR.
        Some((b, tail)) if tail.trim().trim_end_matches('.').is_empty() => {
            (b, TimingRestriction::Sorcery)
        }
        Some(_) => return Ok(None),
        None => (effect_part, TimingRestriction::Instant),
    };
    let effect = parse_effect_block(body, &mut targets)?;
    Ok(Some(Ability::Activated(ActivatedAbility {
        cost,
        targets: targets.specs,
        effect,
        timing,
        restriction: Condition::Always,
        uses_per_turn: None,
        loyalty_change: None,
        text: original.to_string(),
    })))
}

fn parse_cost(text: &str) -> Parsed<Cost> {
    let mut parts = Vec::new();
    for raw in text.split(", ") {
        parts.push(parse_cost_part(raw.trim())?);
    }
    match parts.len() {
        0 => Err(Unsupported::new("custo vazio")),
        1 => parts.pop().ok_or_else(|| Unsupported::new("custo vazio")),
        _ => Ok(Cost::Composite(parts)),
    }
}

fn parse_cost_part(s: &str) -> Parsed<Cost> {
    if s == "{t}" {
        return Ok(Cost::Tap);
    }
    if s == "{q}" {
        return Err(Unsupported::new("custo de desvirar {Q}"));
    }
    if s.starts_with('{') {
        return parse_mana_cost(s).map(|c| Cost::Mana(c.symbols));
    }
    if s == "sacrifice ~" {
        return Ok(Cost::Sacrifice(1, Filter::IsSelf));
    }
    if let Some(what) = s.strip_prefix("sacrifice ") {
        let (count, phrase) = split_count(what);
        let filter = parse_filter_phrase(phrase)?;
        return Ok(Cost::Sacrifice(count, filter.filter));
    }
    if s == "discard a card" {
        return Ok(Cost::Discard(1, Filter::Any));
    }
    if let Some(n) = s.strip_prefix("pay ").and_then(|r| r.strip_suffix(" life")) {
        let amount = parse_word_number(n)
            .ok_or_else(|| Unsupported::new(format!("custo de vida '{s}'")))?;
        return Ok(Cost::PayLife(Value::Const(amount)));
    }
    if s == "remove a +1/+1 counter from ~" {
        return Ok(Cost::RemoveCounters(1, CounterKind::PlusOnePlusOne));
    }
    Err(Unsupported::new(format!("custo '{s}'")))
}

/// "two creatures" -> (2, "creatures"); "a creature" -> (1, "creature").
fn split_count(phrase: &str) -> (u8, &str) {
    match phrase.split_once(' ') {
        Some((head, rest)) => match parse_word_number(head).and_then(|n| u8::try_from(n).ok()) {
            Some(n) => (n, rest),
            None => (1, phrase),
        },
        None => (1, phrase),
    }
}

// ---- habilidade disparada ---------------------------------------------------

fn parse_triggered(lower: &str, original: &str) -> Parsed<Option<Ability>> {
    let Some((trigger, rest, from_graveyard)) = parse_trigger_condition(lower) else {
        return Ok(None);
    };
    let mut targets = Targets::default();
    let (optional, body) = match rest.strip_prefix("you may ") {
        Some(b) => (true, b),
        None => (false, rest),
    };
    let effect = parse_effect_block(body, &mut targets)?;
    Ok(Some(Ability::Triggered(TriggeredAbility {
        trigger,
        intervening_if: Condition::Always,
        targets: targets.specs,
        effect,
        optional,
        once_per_turn: false,
        triggers_from_graveyard: from_graveyard,
        text: original.to_string(),
    })))
}

/// Reconhece o gatilho e devolve também o resto da frase e se ele funciona do
/// cemitério (gatilho de morte precisa disso).
fn parse_trigger_condition(lower: &str) -> Option<(TriggerCondition, &str, bool)> {
    let self_sel = || Selector::battlefield(Filter::IsSelf);
    let after = |prefix: &str| lower.strip_prefix(prefix).map(str::trim);

    for prefix in ["when ~ enters the battlefield, ", "when ~ enters, "] {
        if let Some(rest) = after(prefix) {
            return Some((TriggerCondition::EntersBattlefield(self_sel()), rest, false));
        }
    }
    if let Some(rest) = after("when ~ dies, ") {
        return Some((TriggerCondition::Dies(self_sel()), rest, true));
    }
    if let Some(rest) = after("when ~ leaves the battlefield, ") {
        return Some((TriggerCondition::LeavesBattlefield(self_sel()), rest, true));
    }
    if let Some(rest) = after("whenever ~ attacks, ") {
        return Some((TriggerCondition::Attacks(self_sel()), rest, false));
    }
    if let Some(rest) = after("whenever ~ blocks, ") {
        return Some((TriggerCondition::Blocks(self_sel()), rest, false));
    }
    if let Some(rest) = after("whenever ~ becomes blocked, ") {
        return Some((TriggerCondition::BecomesBlocked(self_sel()), rest, false));
    }
    if let Some(rest) = after("whenever ~ deals combat damage to a player, ") {
        return Some((TriggerCondition::DealsCombatDamageToPlayer(self_sel()), rest, false));
    }
    if let Some(rest) = after("whenever ~ becomes tapped, ") {
        return Some((TriggerCondition::Taps(self_sel()), rest, false));
    }
    for (prefix, who) in [
        ("at the beginning of your upkeep, ", PlayerRef::You),
        ("at the beginning of each upkeep, ", PlayerRef::Each),
        ("at the beginning of each player's upkeep, ", PlayerRef::Each),
    ] {
        if let Some(rest) = after(prefix) {
            return Some((TriggerCondition::BeginningOfUpkeep(who), rest, false));
        }
    }
    if let Some(rest) = after("at the beginning of your end step, ") {
        return Some((TriggerCondition::BeginningOfEndStep(PlayerRef::You), rest, false));
    }
    if let Some(rest) = after("at the beginning of combat on your turn, ") {
        return Some((TriggerCondition::BeginningOfCombat(PlayerRef::You), rest, false));
    }
    if let Some(rest) = after("at the beginning of each end step, ") {
        return Some((TriggerCondition::BeginningOfEndStep(PlayerRef::Each), rest, false));
    }
    if let Some(rest) = after("at the beginning of your draw step, ") {
        return Some((TriggerCondition::BeginningOfDrawStep(PlayerRef::You), rest, false));
    }
    if let Some(rest) = after("at the beginning of your precombat main phase, ") {
        return Some((TriggerCondition::BeginningOfPrecombatMain(PlayerRef::You), rest, false));
    }
    // "Whenever another creature you control enters, ..." e variantes.
    if let Some(body) = lower.strip_prefix("whenever ") {
        if let Some((subject, rest)) = body.split_once(" cast ").or(body.split_once(" casts ")) {
            if subject == "you" {
                if let Some((what, rest)) = rest.split_once(", ") {
                    if let Some(spell) = what.strip_suffix(" spell") {
                        if let Ok(mut sel) = parse_selector_phrase(spell) {
                            sel.zone = ZoneScope::Stack;
                            sel.owner_scope = Some(PlayerRef::You);
                            return Some((TriggerCondition::SpellCast(sel), rest.trim(), false));
                        }
                    }
                }
            }
        }
        if let Some((subject, rest)) = body.split_once(" enters, ") {
            if let Ok(sel) = parse_selector_phrase(subject) {
                return Some((TriggerCondition::EntersBattlefield(sel), rest.trim(), false));
            }
        }
        if let Some((subject, rest)) = body.split_once(" dies, ") {
            if let Ok(sel) = parse_selector_phrase(subject) {
                return Some((TriggerCondition::Dies(sel), rest.trim(), false));
            }
        }
    }
    None
}

// ---- habilidade estática ----------------------------------------------------

fn parse_static(lower: &str, original: &str) -> Parsed<Option<Ability>> {
    // Texto que começa com gatilho não é estático. Sem esta guarda, um " get "
    // no meio de um gatilho não reconhecido faz o parser estático engasgar e
    // reportar um motivo que não tem nada a ver com o problema real.
    for prefix in ["when ", "whenever ", "at the beginning "] {
        if lower.starts_with(prefix) {
            return Err(Unsupported::new(format!("gatilho '{}'", trim_for_reason(lower))));
        }
    }
    // "Creatures you control get +1/+1." / "Other Elves you control get +1/+1."
    if let Some((subject, bonus)) = lower.split_once(" get ") {
        if let Some((p, t)) = parse_whole_pt_bonus(bonus) {
            let affects = parse_selector_phrase(subject)?;
            return Ok(Some(Ability::Static(StaticAbility {
                condition: Condition::Always,
                affects,
                modification: StaticMod::ModifyPT(Value::Const(p), Value::Const(t)),
                text: original.to_string(),
            })));
        }
    }
    if let Some((subject, bonus)) = lower.split_once(" gets ") {
        if let Some((p, t)) = parse_whole_pt_bonus(bonus) {
            let affects = parse_selector_phrase(subject)?;
            return Ok(Some(Ability::Static(StaticAbility {
                condition: Condition::Always,
                affects,
                modification: StaticMod::ModifyPT(Value::Const(p), Value::Const(t)),
                text: original.to_string(),
            })));
        }
    }
    for connector in [" have ", " has "] {
        if let Some((subject, granted)) = lower.split_once(connector) {
            if let Some(kws) = parse_keyword_line(granted.trim()) {
                let affects = parse_selector_phrase(subject)?;
                return Ok(Some(Ability::Static(StaticAbility {
                    condition: Condition::Always,
                    affects,
                    modification: StaticMod::GrantKeywords(kws),
                    text: original.to_string(),
                })));
            }
        }
    }
    if lower == "~ can't block" {
        return Ok(Some(Ability::Static(StaticAbility {
            condition: Condition::Always,
            affects: Selector::battlefield(Filter::IsSelf),
            modification: StaticMod::CantBlock,
            text: original.to_string(),
        })));
    }
    if lower == "~ can't attack" {
        return Ok(Some(Ability::Static(StaticAbility {
            condition: Condition::Always,
            affects: Selector::battlefield(Filter::IsSelf),
            modification: StaticMod::CantAttack,
            text: original.to_string(),
        })));
    }
    Ok(None)
}

/// "+1/+1 until end of turn" -> (1, 1); "-2/-0" -> (-2, 0).
fn parse_pt_bonus(text: &str) -> Option<(i32, i32)> {
    let token = text.split_whitespace().next()?;
    let (p, t) = token.split_once('/')?;
    Some((signed(p)?, signed(t)?))
}

/// Igual a `parse_pt_bonus`, mas exige que o bônus seja a frase inteira.
///
/// "Nonartifact creatures get +2/+2 **as long as they all share a color**" tem
/// uma condição que o parser não lê; aceitar o +2/+2 sozinho seria imprimir uma
/// carta mais forte que a de verdade.
fn parse_whole_pt_bonus(text: &str) -> Option<(i32, i32)> {
    let t = text.trim();
    if t.split_whitespace().count() != 1 {
        return None;
    }
    parse_pt_bonus(t)
}

fn signed(text: &str) -> Option<i32> {
    let t = text.trim();
    let (sign, digits) = match t.strip_prefix('+') {
        Some(d) => (1, d),
        None => match t.strip_prefix('-') {
            Some(d) => (-1, d),
            None => (1, t),
        },
    };
    digits.parse::<i32>().ok().map(|n| n * sign)
}

// ---- efeito de substituição -------------------------------------------------

fn parse_replacement(lower: &str, original: &str) -> Option<Ability> {
    if lower == "~ enters tapped" || lower == "~ enters the battlefield tapped" {
        return Some(Ability::Replacement(ReplacementAbility {
            event: ReplacementEvent::EntersTapped,
            replacement: Effect::Nothing,
            text: original.to_string(),
        }));
    }
    let with_counters = lower
        .strip_prefix("~ enters with ")
        .or_else(|| lower.strip_prefix("~ enters the battlefield with "))?;
    let (count_word, rest) = with_counters.split_once(' ')?;
    // Só o número fixo. "...para cada outra criatura que você controla" é um
    // valor dinâmico que este trecho não lê, e aceitá-lo daria carta menor.
    if !matches!(rest, "+1/+1 counter on it" | "+1/+1 counters on it") {
        return None;
    }
    let n = parse_word_number(count_word)?;
    Some(Ability::Replacement(ReplacementAbility {
        event: ReplacementEvent::EntersWithCounters(CounterKind::PlusOnePlusOne, Value::Const(n)),
        replacement: Effect::Nothing,
        text: original.to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Frases de efeito
// ---------------------------------------------------------------------------

/// Um bloco pode ter várias frases; viram `Sequence` na ordem escrita.
fn parse_effect_block(lower: &str, targets: &mut Targets) -> Parsed<Effect> {
    let mut effects = Vec::new();
    for sentence in split_sentences(lower) {
        effects.push(parse_sentence(&sentence, targets)?);
    }
    match effects.len() {
        0 => Err(Unsupported::new("efeito vazio")),
        1 => effects.pop().ok_or_else(|| Unsupported::new("efeito vazio")),
        _ => Ok(Effect::Sequence(effects)),
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    text.split(". ")
        .map(|s| s.trim().trim_end_matches('.').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_sentence(s: &str, t: &mut Targets) -> Parsed<Effect> {
    if let Some(rest) = s.strip_prefix("you may ") {
        let inner = parse_sentence(rest, t)?;
        return Ok(Effect::May { do_: Box::new(inner), prompt: rest.to_string() });
    }
    if let Some(e) = parse_damage(s, t)? {
        return Ok(e);
    }
    if let Some(e) = parse_player_sentence(s, t)? {
        return Ok(e);
    }
    if let Some(e) = parse_object_sentence(s, t)? {
        return Ok(e);
    }
    Err(Unsupported::text(format!("frase '{}'", trim_for_reason(s)), s))
}

/// "~ deals 3 damage to any target"
fn parse_damage(s: &str, t: &mut Targets) -> Parsed<Option<Effect>> {
    let Some(rest) = s.strip_prefix("~ deals ") else {
        return Ok(None);
    };
    let Some((amount_text, victim)) = rest.split_once(" damage to ") else {
        return Ok(None);
    };
    let amount = parse_amount(amount_text)?;
    let victim = victim.trim();

    if victim == "any target" {
        let target = t.any_target(victim)?;
        return Ok(Some(Effect::DealDamage { amount, target }));
    }
    if let Some(player) = parse_player_phrase(victim, t)? {
        return Ok(Some(Effect::DealDamageToPlayer { amount, player }));
    }
    let target = parse_object_phrase(victim, t)?;
    Ok(Some(Effect::DealDamage { amount, target }))
}

fn parse_amount(text: &str) -> Parsed<Value> {
    let t = text.trim();
    if t == "x" {
        return Ok(Value::X);
    }
    parse_word_number(t)
        .map(Value::Const)
        .ok_or_else(|| Unsupported::new(format!("quantidade '{t}'")))
}

/// Frases cujo sujeito é jogador: "you draw a card", "each opponent loses 2 life".
fn parse_player_sentence(s: &str, t: &mut Targets) -> Parsed<Option<Effect>> {
    const SUBJECTS: [&str; 6] =
        ["you ", "target player ", "target opponent ", "each opponent ", "each player ", "~'s controller "];
    let mut found: Option<(PlayerRef, &str)> = None;
    for subject in SUBJECTS {
        if let Some(rest) = s.strip_prefix(subject) {
            let who = match parse_player_phrase(subject.trim(), t)? {
                Some(p) => p,
                None => continue,
            };
            found = Some((who, rest.trim()));
            break;
        }
    }
    // Imperativo sem sujeito ("Draw a card.") fala com o controlador.
    let (player, rest) = match found {
        Some(x) => x,
        None => (PlayerRef::You, s),
    };

    for (verb, singular) in [("draws ", false), ("draw ", true)] {
        if let Some(count_text) = rest.strip_prefix(verb) {
            let _ = singular;
            let count = parse_card_count(count_text)?;
            return Ok(Some(Effect::DrawCards { count, player }));
        }
    }
    for verb in ["gains ", "gain "] {
        if let Some(rest) = rest.strip_prefix(verb) {
            if let Some(n) = rest.strip_suffix(" life") {
                return Ok(Some(Effect::GainLife { amount: parse_amount(n)?, player }));
            }
        }
    }
    for verb in ["loses ", "lose "] {
        if let Some(rest) = rest.strip_prefix(verb) {
            if let Some(n) = rest.strip_suffix(" life") {
                return Ok(Some(Effect::LoseLife { amount: parse_amount(n)?, player }));
            }
        }
    }
    for verb in ["discards ", "discard "] {
        if let Some(rest) = rest.strip_prefix(verb) {
            let count = parse_card_count(rest)?;
            return Ok(Some(Effect::Discard {
                count,
                player,
                filter: Filter::Any,
                random: rest.ends_with("at random"),
            }));
        }
    }
    for verb in ["mills ", "mill "] {
        if let Some(rest) = rest.strip_prefix(verb) {
            let count = parse_card_count(rest)?;
            return Ok(Some(Effect::Mill { count, player }));
        }
    }
    if let Some(n) = rest.strip_prefix("scry ") {
        return Ok(Some(Effect::Scry { count: parse_amount(n)?, player }));
    }
    if let Some(n) = rest.strip_prefix("surveil ") {
        return Ok(Some(Effect::Surveil { count: parse_amount(n)?, player }));
    }
    for verb in ["sacrifices ", "sacrifice "] {
        if let Some(rest) = rest.strip_prefix(verb) {
            let (count, phrase) = split_count(rest);
            let filter = parse_filter_phrase(phrase)?;
            return Ok(Some(Effect::Sacrifice {
                player,
                count: Value::Const(count as i32),
                filter: filter.filter,
            }));
        }
    }
    if rest == "wins the game" || rest == "win the game" {
        return Ok(Some(Effect::WinGame { player }));
    }
    if rest == "loses the game" || rest == "lose the game" {
        return Ok(Some(Effect::LoseGame { player }));
    }
    Ok(None)
}

/// "a card" / "two cards" / "x cards"
fn parse_card_count(text: &str) -> Parsed<Value> {
    let t = text.trim().trim_end_matches(" at random").trim();
    let head = t.strip_suffix(" cards").or_else(|| t.strip_suffix(" card")).unwrap_or(t);
    parse_amount(head)
}

/// Frases cujo objeto é permanente ou mágica.
fn parse_object_sentence(s: &str, t: &mut Targets) -> Parsed<Option<Effect>> {
    if let Some(rest) = s.strip_prefix("destroy ") {
        let (phrase, no_regen) = match rest.split_once(". it can't be regenerated") {
            Some((p, _)) => (p, true),
            None => (rest, false),
        };
        let target = parse_object_phrase(phrase, t)?;
        return Ok(Some(Effect::Destroy { target, no_regeneration: no_regen }));
    }
    if let Some(rest) = s.strip_prefix("exile ") {
        let target = parse_object_phrase(rest, t)?;
        return Ok(Some(Effect::Exile { target, until_source_leaves: false }));
    }
    if let Some(rest) = s.strip_prefix("tap ") {
        let target = parse_object_phrase(rest, t)?;
        return Ok(Some(Effect::Tap { target }));
    }
    if let Some(rest) = s.strip_prefix("untap ") {
        let target = parse_object_phrase(rest, t)?;
        return Ok(Some(Effect::Untap { target }));
    }
    if let Some(rest) = s.strip_prefix("return ") {
        if let Some(phrase) = rest.strip_suffix(" to its owner's hand") {
            let target = parse_object_phrase(phrase, t)?;
            return Ok(Some(Effect::ReturnToHand { target }));
        }
    }
    if let Some(rest) = s.strip_prefix("counter ") {
        let filter = match rest {
            "target spell" => Filter::Any,
            "target creature spell" => Filter::HasType(CardType::Creature),
            "target noncreature spell" => Filter::Not(Box::new(Filter::HasType(CardType::Creature))),
            _ => return Err(Unsupported::new(format!("contramágica '{}'", trim_for_reason(rest)))),
        };
        let target = t.spell(filter, rest)?;
        return Ok(Some(Effect::CounterSpell { target, unless_pays: None }));
    }
    if let Some(rest) = s.strip_prefix("~ fights ") {
        let b = parse_object_phrase(rest, t)?;
        return Ok(Some(Effect::Fight { a: ObjRef::SelfObject, b }));
    }
    if let Some(e) = parse_create_token(s)? {
        return Ok(Some(e));
    }
    if let Some(e) = parse_counter_placement(s, t)? {
        return Ok(Some(e));
    }
    if let Some(e) = parse_pump(s, t)? {
        return Ok(Some(e));
    }
    Ok(None)
}

/// "create a 1/1 white Soldier creature token with flying"
///
/// Só a forma completa (com P/T e o substantivo "creature") vira ficha. Ficha
/// predefinida — "create a Treasure token" — depende de uma planta que o
/// catálogo não carrega, então fica de fora em vez de virar 0/0 sem habilidade.
fn parse_create_token(s: &str) -> Parsed<Option<Effect>> {
    let Some(rest) = s.strip_prefix("create ") else {
        return Ok(None);
    };
    let Some((count_word, rest)) = rest.split_once(' ') else {
        return Ok(None);
    };
    let Some(count) = parse_word_number(count_word) else {
        return Ok(None);
    };
    let Some((head, tail)) = rest.split_once(" token") else {
        return Ok(None);
    };
    let tail = tail.trim_start_matches('s').trim();
    let keywords = if tail.is_empty() {
        Vec::new()
    } else if let Some(list) = tail.strip_prefix("with ") {
        match parse_keyword_line(list) {
            Some(k) => k,
            None => return Err(Unsupported::new(format!("ficha com '{}'", trim_for_reason(tail)))),
        }
    } else {
        return Err(Unsupported::new(format!("ficha '{}'", trim_for_reason(tail))));
    };

    let Some(body) = head.strip_suffix(" creature") else {
        return Err(Unsupported::new(format!("ficha '{}'", trim_for_reason(head))));
    };
    let mut words = body.split_whitespace();
    let Some(pt) = words.next() else {
        return Err(Unsupported::new("ficha sem P/T"));
    };
    let Some((p, q)) = pt.split_once('/').and_then(|(a, b)| Some((signed(a)?, signed(b)?))) else {
        return Err(Unsupported::new(format!("P/T de ficha '{pt}'")));
    };

    let mut colors = Vec::new();
    let mut subtypes = Vec::new();
    for word in words {
        if word == "and" {
            continue;
        }
        if let Some(c) = color_by_name(word) {
            colors.push(c);
            continue;
        }
        if word == "colorless" {
            continue;
        }
        subtypes.push(capitalize(word));
    }

    let name = if subtypes.is_empty() { "Token".to_string() } else { subtypes.join(" ") };
    let type_line =
        TypeLine { supertypes: Vec::new(), types: vec![CardType::Creature], subtypes };
    Ok(Some(Effect::CreateToken {
        spec: TokenSpec { name, type_line, colors, power: p, toughness: q, keywords, art_key: None },
        count: Value::Const(count),
        controller: PlayerRef::You,
    }))
}

/// "put a +1/+1 counter on target creature"
fn parse_counter_placement(s: &str, t: &mut Targets) -> Parsed<Option<Effect>> {
    let Some(rest) = s.strip_prefix("put ") else {
        return Ok(None);
    };
    let Some((count_part, where_part)) = rest.split_once(" counter on ") else {
        let Some((count_part, where_part)) = rest.split_once(" counters on ") else {
            return Ok(None);
        };
        return counter_effect(count_part, where_part, t).map(Some);
    };
    counter_effect(count_part, where_part, t).map(Some)
}

fn counter_effect(count_part: &str, where_part: &str, t: &mut Targets) -> Parsed<Effect> {
    let (count_word, kind_word) = count_part
        .rsplit_once(' ')
        .ok_or_else(|| Unsupported::new(format!("marcador '{count_part}'")))?;
    let kind = match kind_word {
        "+1/+1" => CounterKind::PlusOnePlusOne,
        "-1/-1" => CounterKind::MinusOneMinusOne,
        "charge" => CounterKind::Charge,
        other => CounterKind::Named(other.to_string()),
    };
    let count = parse_amount(count_word)?;
    let target = parse_object_phrase(where_part, t)?;
    Ok(Effect::AddCounters { target, kind, count })
}

/// "target creature gets +2/+2 until end of turn" e "... gains flying until end of turn".
fn parse_pump(s: &str, t: &mut Targets) -> Parsed<Option<Effect>> {
    if let Some((subject, rest)) = s.split_once(" gets ") {
        let Some((bonus, duration)) = split_duration(rest) else {
            return Ok(None);
        };
        let Some((p, q)) = parse_whole_pt_bonus(bonus) else {
            return Ok(None);
        };
        let target = parse_object_phrase(subject, t)?;
        return Ok(Some(Effect::ModifyPT {
            target,
            power: Value::Const(p),
            toughness: Value::Const(q),
            duration,
        }));
    }
    for verb in [" gains ", " gain "] {
        if let Some((subject, rest)) = s.split_once(verb) {
            let Some((granted, duration)) = split_duration(rest) else {
                continue;
            };
            let Some(keywords) = parse_keyword_line(granted.trim()) else {
                continue;
            };
            let target = parse_object_phrase(subject, t)?;
            return Ok(Some(Effect::GrantKeywords { target, keywords, duration }));
        }
    }
    Ok(None)
}

fn split_duration(text: &str) -> Option<(&str, Duration)> {
    if let Some(head) = text.strip_suffix(" until end of turn") {
        return Some((head, Duration::EndOfTurn));
    }
    None
}

// ---------------------------------------------------------------------------
// Sintagmas nominais
// ---------------------------------------------------------------------------

/// Um sintagma já resolvido: o filtro e, quando dito, de quem é o permanente.
#[derive(Debug, Clone)]
struct FilterPhrase {
    filter: Filter,
    owner: Option<PlayerRef>,
}

/// "target creature you control" e afins, na posição de objeto de um efeito.
fn parse_object_phrase(phrase: &str, t: &mut Targets) -> Parsed<ObjRef> {
    let phrase = phrase.trim();
    if phrase == "~" || phrase == "itself" {
        return Ok(ObjRef::SelfObject);
    }
    if let Some(rest) = phrase.strip_prefix("target ") {
        let parsed = parse_filter_phrase(rest)?;
        let mut selector = Selector::battlefield(parsed.filter);
        selector.owner_scope = parsed.owner;
        selector.max = Some(1);
        return t.object(selector, phrase);
    }
    for prefix in ["each ", "all "] {
        if let Some(rest) = phrase.strip_prefix(prefix) {
            let parsed = parse_filter_phrase(rest)?;
            let mut selector = Selector::battlefield(parsed.filter);
            selector.owner_scope = parsed.owner;
            return Ok(ObjRef::All(selector));
        }
    }
    // Sintagma no plural sem "target" é conjunto: "creatures you control gain
    // flying" fala de todas elas.
    if phrase.ends_with('s') || phrase.contains("s you control") {
        if let Ok(parsed) = parse_filter_phrase(phrase) {
            let mut selector = Selector::battlefield(parsed.filter);
            selector.owner_scope = parsed.owner;
            return Ok(ObjRef::All(selector));
        }
    }
    Err(Unsupported::new(format!("objeto '{}'", trim_for_reason(phrase))))
}

/// Sintagma na posição de sujeito de habilidade estática ou de gatilho:
/// "creatures you control", "another creature you control".
fn parse_selector_phrase(phrase: &str) -> Parsed<Selector> {
    let phrase = phrase.trim();
    if phrase == "~" {
        return Ok(Selector::battlefield(Filter::IsSelf));
    }
    let parsed = parse_filter_phrase(phrase)?;
    let mut selector = Selector::battlefield(parsed.filter);
    selector.owner_scope = parsed.owner;
    Ok(selector)
}

/// Frases de jogador. `None` quando o sintagma não é jogador.
fn parse_player_phrase(phrase: &str, t: &mut Targets) -> Parsed<Option<PlayerRef>> {
    let p = phrase.trim();
    let who = match p {
        "you" | "yourself" => PlayerRef::You,
        "each opponent" | "each of your opponents" | "your opponents" => PlayerRef::Opponents,
        "each player" => PlayerRef::Each,
        "~'s controller" => PlayerRef::ControllerOf(Box::new(ObjRef::SelfObject)),
        "target player" => return t.player(PlayerRef::Each, p).map(Some),
        "target opponent" => return t.player(PlayerRef::Opponents, p).map(Some),
        _ => return Ok(None),
    };
    Ok(Some(who))
}

/// O coração do reconhecimento de sintagma: adjetivos, substantivo, qualificador.
fn parse_filter_phrase(phrase: &str) -> Parsed<FilterPhrase> {
    let mut rest = phrase.trim();
    let mut owner = None;
    for (suffix, who) in [
        (" you control", PlayerRef::You),
        (" you don't control", PlayerRef::Opponents),
        (" an opponent controls", PlayerRef::Opponents),
        (" your opponents control", PlayerRef::Opponents),
    ] {
        if let Some(head) = rest.strip_suffix(suffix) {
            owner = Some(who);
            rest = head.trim();
            break;
        }
    }

    let mut extra: Vec<Filter> = Vec::new();
    if let Some((head, tail)) = rest.split_once(" with ") {
        extra.push(parse_with_qualifier(tail)?);
        rest = head.trim();
    }

    // "artifact or enchantment" — disjunção de substantivos simples.
    if rest.contains(" or ") {
        let mut alts = Vec::new();
        for part in rest.split(" or ") {
            alts.push(parse_noun_group(part.trim())?);
        }
        return Ok(FilterPhrase { filter: combine(Filter::Or(alts), extra), owner });
    }
    let base = parse_noun_group(rest)?;
    Ok(FilterPhrase { filter: combine(base, extra), owner })
}

fn combine(base: Filter, extra: Vec<Filter>) -> Filter {
    if extra.is_empty() {
        return base;
    }
    let mut all = vec![base];
    all.extend(extra);
    Filter::And(all)
}

fn parse_with_qualifier(tail: &str) -> Parsed<Filter> {
    let t = tail.trim();
    if let Some(k) = parse_keyword(t) {
        return Ok(Filter::HasKeyword(k));
    }
    for (prefix, build) in [
        ("power ", &Filter::PowerAtMost as &dyn Fn(i32) -> Filter),
        ("toughness ", &Filter::ToughnessAtMost),
    ] {
        if let Some(rest) = t.strip_prefix(prefix) {
            if let Some(n) = rest.strip_suffix(" or less").and_then(parse_word_number) {
                return Ok(build(n));
            }
            if let Some(n) = rest.strip_suffix(" or greater").and_then(parse_word_number) {
                return Ok(match prefix {
                    "power " => Filter::PowerAtLeast(n),
                    _ => Filter::ToughnessAtLeast(n),
                });
            }
        }
    }
    if let Some(n) = t.strip_prefix("mana value ").and_then(|r| r.strip_suffix(" or less")).and_then(parse_word_number)
    {
        let n = u32::try_from(n).map_err(|_| Unsupported::new(format!("qualificador '{t}'")))?;
        return Ok(Filter::ManaValueAtMost(n));
    }
    Err(Unsupported::new(format!("qualificador '{t}'")))
}

/// Adjetivos + substantivo. "another nontoken artifact creature".
fn parse_noun_group(phrase: &str) -> Parsed<Filter> {
    let mut parts: Vec<Filter> = Vec::new();
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if words.is_empty() {
        return Err(Unsupported::new("sintagma vazio"));
    }
    let mut nouns: Vec<Filter> = Vec::new();
    for word in &words {
        // Artigo não filtra nada; "a creature you control" é o mesmo conjunto
        // que "creature you control".
        if matches!(*word, "a" | "an" | "the") {
            continue;
        }
        if let Some(f) = adjective_filter(word) {
            parts.push(f);
            continue;
        }
        match noun_filter(word) {
            Some(f) => nouns.push(f),
            None => return Err(Unsupported::new(format!("substantivo '{word}'"))),
        }
    }
    if nouns.is_empty() {
        return Err(Unsupported::new(format!("sintagma '{}'", trim_for_reason(phrase))));
    }
    parts.extend(nouns);
    Ok(if parts.len() == 1 {
        parts.pop().unwrap_or(Filter::Any)
    } else {
        Filter::And(parts)
    })
}

fn adjective_filter(word: &str) -> Option<Filter> {
    let f = match word {
        "another" | "other" => Filter::IsOther,
        "attacking" => Filter::Attacking,
        "blocking" => Filter::Blocking,
        "blocked" => Filter::Blocked,
        "unblocked" => Filter::Unblocked,
        "tapped" => Filter::Tapped,
        "untapped" => Filter::Untapped,
        "token" => Filter::Token,
        "nontoken" => Filter::NonToken,
        "legendary" => Filter::HasSupertype(Supertype::Legendary),
        "basic" => Filter::HasSupertype(Supertype::Basic),
        "snow" => Filter::HasSupertype(Supertype::Snow),
        "colorless" => Filter::Colorless,
        "multicolored" => Filter::Multicolored,
        "nonland" => Filter::Not(Box::new(Filter::HasType(CardType::Land))),
        "noncreature" => Filter::Not(Box::new(Filter::HasType(CardType::Creature))),
        "nonartifact" => Filter::Not(Box::new(Filter::HasType(CardType::Artifact))),
        _ => {
            if let Some(color) = word.strip_prefix("non").and_then(color_by_name) {
                return Some(Filter::Not(Box::new(Filter::HasColor(color))));
            }
            return color_by_name(word).map(Filter::HasColor);
        }
    };
    Some(f)
}

fn noun_filter(word: &str) -> Option<Filter> {
    let singular = word.strip_suffix('s').unwrap_or(word);
    let f = match singular {
        "permanent" => Filter::Any,
        "creature" => Filter::HasType(CardType::Creature),
        "artifact" => Filter::HasType(CardType::Artifact),
        "enchantment" => Filter::HasType(CardType::Enchantment),
        "land" => Filter::HasType(CardType::Land),
        "planeswalker" => Filter::HasType(CardType::Planeswalker),
        "battle" => Filter::HasType(CardType::Battle),
        _ => return None,
    };
    Some(f)
}

/// Zona padrão dos seletores criados aqui. Existe só para deixar explícito que
/// nada neste compilador olha para cemitério ainda.
#[allow(dead_code)]
const DEFAULT_ZONE: ZoneScope = ZoneScope::Battlefield;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scryfall::CardFace;

    fn card(name: &str, type_line: &str, mana: &str, text: &str) -> ScryfallCard {
        ScryfallCard {
            oracle_id: Some("id".to_string()),
            name: Some(name.to_string()),
            mana_cost: Some(mana.to_string()),
            type_line: Some(type_line.to_string()),
            oracle_text: Some(text.to_string()),
            rarity: Some("common".to_string()),
            set: Some("tst".to_string()),
            collector_number: Some("1".to_string()),
            layout: Some("normal".to_string()),
            games: Some(vec!["paper".to_string()]),
            ..Default::default()
        }
    }

    fn creature(name: &str, p: &str, t: &str, text: &str) -> ScryfallCard {
        let mut c = card(name, "Creature \u{2014} Human Soldier", "{1}{W}", text);
        c.power = Some(p.to_string());
        c.toughness = Some(t.to_string());
        c
    }

    #[test]
    fn vanilla_creature_is_playable() {
        let out = compile_card(&creature("Grizzly Bears", "2", "2", ""), 0);
        assert!(out.playable, "criatura baunilha deve ser jogável: {:?}", out.reason);
        assert_eq!(out.def.power, Some(2));
        assert!(out.def.abilities.is_empty());
    }

    #[test]
    fn keyword_line_becomes_keyword_abilities() {
        let out = compile_card(&creature("Serra Angel", "4", "4", "Flying, vigilance"), 0);
        assert!(out.playable, "{:?}", out.reason);
        let kws: Vec<&Keyword> = out.def.keywords().collect();
        assert_eq!(kws, vec![&Keyword::Flying, &Keyword::Vigilance]);
    }

    #[test]
    fn bolt_compiles_to_damage_with_any_target() {
        let bolt = card(
            "Lightning Bolt",
            "Instant",
            "{R}",
            "Lightning Bolt deals 3 damage to any target.",
        );
        let out = compile_card(&bolt, 0);
        assert!(out.playable, "{:?}", out.reason);
        assert_eq!(
            out.def.spell_effect,
            Some(Effect::DealDamage { amount: Value::Const(3), target: ObjRef::Target(0) })
        );
        assert_eq!(out.def.spell_targets.len(), 1);
        assert!(matches!(out.def.spell_targets[0].kind, TargetKind::ObjectOrPlayer(_, _)));
    }

    #[test]
    fn giant_growth_pumps_target_creature() {
        let gg = card(
            "Giant Growth",
            "Instant",
            "{G}",
            "Target creature gets +3/+3 until end of turn.",
        );
        let out = compile_card(&gg, 0);
        assert!(out.playable, "{:?}", out.reason);
        assert_eq!(
            out.def.spell_effect,
            Some(Effect::ModifyPT {
                target: ObjRef::Target(0),
                power: Value::Const(3),
                toughness: Value::Const(3),
                duration: Duration::EndOfTurn,
            })
        );
    }

    #[test]
    fn multi_sentence_spell_becomes_a_sequence() {
        let c = card("Test Drain", "Sorcery", "{B}", "Each opponent loses 2 life. You gain 2 life.");
        let out = compile_card(&c, 0);
        assert!(out.playable, "{:?}", out.reason);
        match out.def.spell_effect {
            Some(Effect::Sequence(v)) => assert_eq!(v.len(), 2),
            other => panic!("esperava sequência, veio {other:?}"),
        }
    }

    #[test]
    fn etb_trigger_compiles() {
        let c = creature(
            "Gray Merchant",
            "2",
            "4",
            "When Gray Merchant enters, each opponent loses 2 life.",
        );
        let out = compile_card(&c, 0);
        assert!(out.playable, "{:?}", out.reason);
        let triggered: Vec<_> = out.def.triggered().collect();
        assert_eq!(triggered.len(), 1);
        assert!(matches!(triggered[0].1.trigger, TriggerCondition::EntersBattlefield(_)));
    }

    #[test]
    fn basic_land_gets_its_intrinsic_mana_ability() {
        let mut forest = card("Forest", "Basic Land \u{2014} Forest", "", "({T}: Add {G}.)");
        forest.colors = Some(vec![]);
        let out = compile_card(&forest, 0);
        assert!(out.playable, "{:?}", out.reason);
        let mana: Vec<_> = out.def.mana_abilities().collect();
        assert_eq!(mana.len(), 1, "terreno básico precisa produzir mana sem texto");
    }

    #[test]
    fn tap_for_mana_ability_compiles() {
        let c = card("Llanowar Elves", "Creature \u{2014} Elf Druid", "{G}", "{T}: Add {G}.");
        let out = compile_card(&c, 0);
        assert!(out.playable, "{:?}", out.reason);
        assert_eq!(out.def.mana_abilities().count(), 1);
    }

    #[test]
    fn anthem_becomes_static_ability() {
        let c = card(
            "Glorious Anthem",
            "Enchantment",
            "{1}{W}{W}",
            "Creatures you control get +1/+1.",
        );
        let out = compile_card(&c, 0);
        assert!(out.playable, "{:?}", out.reason);
        let statics: Vec<_> = out.def.statics().collect();
        assert_eq!(statics.len(), 1);
        assert_eq!(statics[0].1.affects.owner_scope, Some(PlayerRef::You));
    }

    #[test]
    fn unknown_text_marks_card_unplayable_but_keeps_metadata() {
        let c = card(
            "Weird Card",
            "Enchantment",
            "{2}{U}",
            "Whenever a player casts a spell with mana value 3 or greater, that player exiles the top card of their library face down.",
        );
        let out = compile_card(&c, 0);
        assert!(!out.playable);
        assert!(out.reason.is_some(), "não jogável tem que dizer por quê");
        assert_eq!(out.def.name, "Weird Card");
        assert_eq!(out.def.mana_cost.mana_value(), 3, "metadado continua correto");
    }

    #[test]
    fn multiface_card_without_face_data_is_never_playable() {
        let mut c = card("A // B", "Creature \u{2014} Human", "{U}", "");
        c.layout = Some("transform".to_string());
        c.card_faces = Some(vec![Default::default(), Default::default()]);
        let out = compile_card(&c, 0);
        assert!(!out.playable);
        assert_eq!(
            out.reason.as_deref(),
            Some("sem linha de tipo"),
            "face vazia não tem linha de tipo, e é isso que tem de ser dito"
        );
    }

    // -----------------------------------------------------------------------
    // Layout: qual face o compilador lê
    // -----------------------------------------------------------------------

    /// Carta de duas faces com dado de verdade em cada face, como o Scryfall
    /// entrega: a raiz só traz nome e linha de tipo juntados por `//`.
    fn two_faced(layout: &str, front: CardFace, back: CardFace) -> ScryfallCard {
        let joined = |a: &Option<String>, b: &Option<String>| {
            format!("{} // {}", a.clone().unwrap_or_default(), b.clone().unwrap_or_default())
        };
        ScryfallCard {
            oracle_id: Some("id".to_string()),
            name: Some(joined(&front.name, &back.name)),
            type_line: Some(joined(&front.type_line, &back.type_line)),
            rarity: Some("common".to_string()),
            set: Some("tst".to_string()),
            collector_number: Some("1".to_string()),
            layout: Some(layout.to_string()),
            games: Some(vec!["paper".to_string()]),
            card_faces: Some(vec![front, back]),
            ..Default::default()
        }
    }

    fn face(name: &str, cost: &str, type_line: &str, text: &str) -> CardFace {
        CardFace {
            name: Some(name.to_string()),
            mana_cost: Some(cost.to_string()),
            type_line: Some(type_line.to_string()),
            oracle_text: Some(text.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn transform_front_that_stands_alone_compiles_from_face_zero() {
        // Não existe carta assim no bulk de hoje — toda frente `transform` fala
        // da outra face. O teste prova o MECANISMO: quando a frente se basta,
        // custo, tipo, P/T e texto saem da face 0, nunca da raiz.
        let mut front = face("Lone Sentry", "{1}{U}", "Creature \u{2014} Bird", "Flying");
        front.power = Some("2".to_string());
        front.toughness = Some("3".to_string());
        let back = face("Awakened Sentry", "", "Creature \u{2014} Bird Horror", "Trample");
        let out = compile_card(&two_faced("transform", front, back), 0);

        assert!(out.playable, "frente autossuficiente tem de compilar: {:?}", out.reason);
        assert_eq!(out.def.mana_cost.mana_value(), 2, "custo vem da face 0, não da raiz");
        assert_eq!(out.def.type_line.subtypes, vec!["Bird".to_string()]);
        assert_eq!((out.def.power, out.def.toughness), (Some(2), Some(3)));
        assert_eq!(
            out.def.keywords().count(),
            1,
            "só o Flying da frente entra; o Trample de trás nunca"
        );
        assert_eq!(
            out.def.name, "Lone Sentry // Awakened Sentry",
            "o nome da CARTA continua o da raiz — é a chave do catálogo"
        );
    }

    #[test]
    fn transform_front_that_names_the_other_face_is_not_playable() {
        let mut front = face(
            "Delver of Secrets",
            "{U}",
            "Creature \u{2014} Human Wizard",
            "At the beginning of your upkeep, look at the top card of your library. \
             You may reveal that card. If an instant or sorcery card is revealed this \
             way, transform Delver of Secrets.",
        );
        front.power = Some("1".to_string());
        front.toughness = Some("1".to_string());
        let back = face("Insectile Aberration", "", "Creature \u{2014} Human Insect", "Flying");
        let out = compile_card(&two_faced("transform", front, back), 0);

        assert!(!out.playable, "a frente manda virar a carta; jogar só metade seria mentira");
        let why = out.reason.unwrap_or_default();
        assert!(why.contains("transform"), "o motivo tem de nomear a marca achada, veio: {why}");
        assert!(
            out.pattern.is_none(),
            "bloqueio de layout não é padrão de texto e não pode poluir o relatório"
        );
    }

    #[test]
    fn a_siege_is_blocked_even_without_saying_transform() {
        // "Invasion of Alara": CR 310.9 lança a face de trás quando sai o
        // último contador de defesa. O texto da frente não avisa nada disso.
        let front = face(
            "Invasion of Somewhere",
            "{2}{R}",
            "Battle \u{2014} Siege",
            "When this Siege enters, it deals 3 damage to any target.",
        );
        let back = face("Aftermath", "", "Creature \u{2014} Horror", "Haste");
        let out = compile_card(&two_faced("transform", front, back), 0);
        assert!(!out.playable, "Siege sem a face de trás nunca faz o que a carta faz");
        assert!(out.reason.unwrap_or_default().contains("battle"));
    }

    #[test]
    fn split_is_blocked_and_the_catalog_keeps_both_halves() {
        let out = compile_card(
            &two_faced(
                "split",
                face("Fire", "{1}{R}", "Instant", "Fire deals 2 damage divided as you choose."),
                face("Ice", "{1}{U}", "Instant", "Tap target permanent."),
            ),
            0,
        );
        assert!(!out.playable, "cada metade tem custo próprio; o IR não representa isso");
        assert!(out.reason.unwrap_or_default().contains("custo proprio"));
        let text = out.def.oracle_text.clone();
        assert!(text.contains("Fire {1}{R}"), "a metade esquerda entra com nome e custo: {text}");
        assert!(text.contains("Ice {1}{U}"), "a metade direita não pode sumir do catálogo: {text}");
        assert!(text.contains("Tap target permanent."), "o texto da direita também: {text}");
        assert_eq!(
            out.def.type_line.types,
            vec![CardType::Instant],
            "a linha de tipo do catálogo sai da face 0, e não fica vazia como antes"
        );
    }

    #[test]
    fn single_face_layout_reaches_the_text_compiler() {
        // Uma Saga não compila hoje, mas o motivo tem de ser o texto que falta,
        // não a palavra "saga". Essa diferença é o que o relatório de cobertura
        // mostra como trabalho a fazer.
        let mut c = card(
            "Saga de Teste",
            "Enchantment \u{2014} Saga",
            "{1}{W}",
            "I \u{2014} Exile target creature.",
        );
        c.layout = Some("saga".to_string());
        let out = compile_card(&c, 0);
        assert!(!out.playable);
        let why = out.reason.unwrap_or_default();
        assert!(
            !why.contains("layout"),
            "layout de face única não pode mais ser motivo em si, veio: {why}"
        );
        assert!(out.pattern.is_some(), "bloqueio textual tem de virar padrão no relatório");
    }

    #[test]
    fn an_unknown_layout_never_compiles() {
        let mut c = creature("Carta do Futuro", "2", "2", "Flying");
        c.layout = Some("hyperspace_dfc".to_string());
        let out = compile_card(&c, 0);
        assert!(!out.playable, "layout desconhecido é regra desconhecida");
        assert_eq!(out.reason.as_deref(), Some("layout desconhecido 'hyperspace_dfc'"));
    }

    #[test]
    fn variable_power_is_not_playable() {
        let out = compile_card(&creature("Tarmogoyf", "*", "1+*", ""), 0);
        assert!(!out.playable);
        assert_eq!(out.def.power, None);
    }

    #[test]
    fn reminder_text_is_ignored() {
        let out = compile_card(&creature("Flier", "1", "1", "Flying (It can't be blocked except by creatures with flying or reach.)"), 0);
        assert!(out.playable, "{:?}", out.reason);
        assert_eq!(out.def.keywords().count(), 1);
    }

    #[test]
    fn destroy_target_creature_collects_the_target() {
        let c = card("Murder", "Instant", "{1}{B}{B}", "Destroy target creature.");
        let out = compile_card(&c, 0);
        assert!(out.playable, "{:?}", out.reason);
        assert_eq!(out.def.spell_targets.len(), 1);
        assert_eq!(
            out.def.spell_effect,
            Some(Effect::Destroy { target: ObjRef::Target(0), no_regeneration: false })
        );
    }

    #[test]
    fn activated_ability_with_composite_cost() {
        let c = card(
            "Prodigal Pyromancer",
            "Creature \u{2014} Human Wizard",
            "{2}{R}",
            "{T}: Prodigal Pyromancer deals 1 damage to any target.",
        );
        let out = compile_card(&c, 0);
        assert!(out.playable, "{:?}", out.reason);
        let acts: Vec<_> = out.def.activated().collect();
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].1.cost, Cost::Tap);
        assert_eq!(acts[0].1.targets.len(), 1);
    }

    #[test]
    fn conditional_anthem_is_rejected_not_truncated() {
        // "Nonartifact creatures get +2/+2 as long as they all share a color":
        // aceitar só o +2/+2 imprimiria uma carta mais forte que a de verdade.
        let c = card(
            "Common Cause",
            "Enchantment",
            "{2}{W}",
            "Nonartifact creatures get +2/+2 as long as they all share a color.",
        );
        let out = compile_card(&c, 0);
        assert!(!out.playable, "condição ignorada não pode virar carta jogável");
    }

    /// O invariante e "nao truncar", nao "recusar". Enquanto so a primeira
    /// passada existia, a unica saida fiel para esta frase era recusa-la; a
    /// segunda passada compila a clausula inteira, e ai a carta jogavel PRECISA
    /// trazer o `GrantKeywords`. Aceitar sem ele seria a infidelidade que este
    /// teste sempre existiu para impedir.
    #[test]
    fn pump_with_extra_clause_is_never_truncated() {
        let c = card(
            "Might of the Ancestors",
            "Instant",
            "{W}",
            "Target creature you control gets +2/+0 and gains vigilance until end of turn.",
        );
        let out = compile_card(&c, 0);
        if !out.playable {
            return;
        }
        let Some(Effect::Sequence(steps)) = &out.def.spell_effect else {
            panic!("duas clausulas, um efeito so: {:?}", out.def.spell_effect);
        };
        assert!(
            steps.contains(&Effect::ModifyPT {
                target: ObjRef::Target(0),
                power: Value::Const(2),
                toughness: Value::Const(0),
                duration: Duration::EndOfTurn,
            }),
            "faltou o +2/+0: {steps:?}"
        );
        assert!(
            steps.contains(&Effect::GrantKeywords {
                target: ObjRef::Target(0),
                keywords: vec![Keyword::Vigilance],
                duration: Duration::EndOfTurn,
            }),
            "'and gains vigilance' foi descartado em silencio: {steps:?}"
        );
    }

    #[test]
    fn land_with_basic_subtype_taps_for_mana_without_the_supertype() {
        // CR 305.6: a habilidade vem do subtipo. Dryad Arbor não é Basic.
        let mut arbor = card("Dryad Arbor", "Land Creature \u{2014} Forest Dryad", "", "");
        arbor.power = Some("1".to_string());
        arbor.toughness = Some("1".to_string());
        let out = compile_card(&arbor, 0);
        assert!(out.playable, "{:?}", out.reason);
        assert_eq!(out.def.mana_abilities().count(), 1);
    }

    #[test]
    fn modal_with_per_mode_targets_is_rejected() {
        // O IR guarda alvos por habilidade, não por modo: aceitar isto pediria
        // os dois alvos de uma vez.
        let c = card(
            "Test Charm",
            "Instant",
            "{1}{W}",
            "Choose one \u{2014}\n\u{2022} Destroy target artifact.\n\u{2022} Destroy target enchantment.",
        );
        let out = compile_card(&c, 0);
        assert!(!out.playable);
        assert_eq!(out.reason.as_deref(), Some("modal com alvo por modo"));
    }

    #[test]
    fn enters_with_counters_only_accepts_a_fixed_number() {
        let fixed = creature(
            "Fixed",
            "0",
            "0",
            "Fixed enters with two +1/+1 counters on it.",
        );
        assert!(compile_card(&fixed, 0).playable);

        let dynamic = creature(
            "Dynamic",
            "0",
            "0",
            "Dynamic enters with a +1/+1 counter on it for each artifact you control.",
        );
        assert!(!compile_card(&dynamic, 0).playable, "'for each' não pode virar 1");
    }

    #[test]
    fn token_creation_compiles() {
        let c = card(
            "Raise the Alarm",
            "Instant",
            "{1}{W}",
            "Create two 1/1 white Soldier creature tokens.",
        );
        let out = compile_card(&c, 0);
        assert!(out.playable, "{:?}", out.reason);
        match out.def.spell_effect {
            Some(Effect::CreateToken { spec, count, .. }) => {
                assert_eq!(spec.power, 1);
                assert_eq!(spec.type_line.subtypes, vec!["Soldier".to_string()]);
                assert_eq!(count, Value::Const(2));
            }
            other => panic!("esperava ficha, veio {other:?}"),
        }
    }

    #[test]
    fn ability_word_prefix_is_stripped() {
        let c = card(
            "Test Landfall",
            "Enchantment",
            "{G}",
            "Landfall \u{2014} Whenever a land you control enters, you gain 1 life.",
        );
        let out = compile_card(&c, 0);
        assert!(out.playable, "{:?}", out.reason);
    }

    #[test]
    fn compilation_is_deterministic() {
        let c = card("Murder", "Instant", "{1}{B}{B}", "Destroy target creature.");
        let a = compile_card(&c, 7);
        let b = compile_card(&c, 7);
        assert_eq!(a.def, b.def);
        assert_eq!(a.playable, b.playable);
    }

    // -----------------------------------------------------------------------
    // Segunda passada (`mtg_oracle::compile`)
    // -----------------------------------------------------------------------

    /// "Each player sacrifices a land of their choice" e vocabulario que so o
    /// compilador de `mtg-oracle` tem. O teste afirma o IR INTEIRO, nao so o
    /// `playable`: e o texto que manda cada jogador sacrificar UM terreno, e e
    /// exatamente isso que tem de estar no `Effect`.
    #[test]
    fn second_pass_compiles_each_player_sacrifices_with_faithful_ir() {
        let c = card(
            "Tremble",
            "Sorcery",
            "{1}{R}",
            "Each player sacrifices a land of their choice.",
        );
        let out = compile_card(&c, 0);
        assert!(out.playable, "esperava jogavel, veio {:?}", out.reason);
        assert!(out.second_pass, "esta familia so existe na segunda passada");
        assert_eq!(
            out.def.spell_effect,
            Some(Effect::Sacrifice {
                player: PlayerRef::Each,
                count: Value::Const(1),
                filter: Filter::HasType(CardType::Land),
            })
        );
        assert!(out.def.spell_targets.is_empty(), "a carta nao tem alvo");
    }

    /// Custo composto com sacrificio de si mesma, alvo restrito e dano: a IR
    /// tem de trazer o custo, o alvo e o dano exatamente como escritos.
    #[test]
    fn second_pass_compiles_activated_ability_with_faithful_cost_and_target() {
        let mut c = creature(
            "Expendable Troops",
            "2",
            "1",
            "{T}, Sacrifice this creature: It deals 2 damage to target attacking or blocking creature.",
        );
        c.mana_cost = Some("{1}{W}".to_string());
        let out = compile_card(&c, 0);
        assert!(out.playable, "esperava jogavel, veio {:?}", out.reason);
        assert!(out.second_pass);
        let [Ability::Activated(a)] = &out.def.abilities[..] else {
            panic!("esperava uma habilidade ativada, veio {:?}", out.def.abilities);
        };
        assert_eq!(a.cost, Cost::Composite(vec![Cost::Tap, Cost::Sacrifice(1, Filter::IsSelf)]));
        assert_eq!(a.effect, Effect::DealDamage { amount: Value::Const(2), target: ObjRef::Target(0) });
        assert_eq!(a.targets.len(), 1, "um alvo, o da frase");
    }

    /// O portao de seguranca: a segunda passada nao pode ressuscitar carta
    /// barrada por LAYOUT. O texto da face frontal aqui compila sozinho, e
    /// mesmo assim a carta tem de continuar fora — a outra metade nao existe
    /// no IR e jogar so a frente seria jogar outra carta.
    #[test]
    fn second_pass_does_not_rescue_a_card_blocked_by_layout() {
        let mut c = card("Fire // Ice", "Instant // Instant", "", "");
        c.layout = Some("split".to_string());
        c.card_faces = Some(vec![
            CardFace {
                name: Some("Fire".to_string()),
                mana_cost: Some("{1}{R}".to_string()),
                type_line: Some("Instant".to_string()),
                oracle_text: Some("Each player sacrifices a land of their choice.".to_string()),
                ..Default::default()
            },
            CardFace {
                name: Some("Ice".to_string()),
                mana_cost: Some("{1}{U}".to_string()),
                type_line: Some("Instant".to_string()),
                oracle_text: Some("Draw a card.".to_string()),
                ..Default::default()
            },
        ]);
        let out = compile_card(&c, 0);
        assert!(!out.playable, "split nao pode entrar por meia carta");
        assert!(!out.second_pass);
        let reason = out.reason.unwrap_or_default();
        assert!(reason.contains("split"), "o motivo tem de ser o layout, veio {reason}");
    }

    /// Carta que o compilador deste crate ja aceitava nao pode mudar de IR por
    /// causa da segunda passada — ela so entra depois de um `Err`.
    #[test]
    fn second_pass_never_touches_a_card_the_first_pass_accepted() {
        let bolt = card(
            "Lightning Bolt",
            "Instant",
            "{R}",
            "Lightning Bolt deals 3 damage to any target.",
        );
        let out = compile_card(&bolt, 0);
        assert!(out.playable);
        assert!(!out.second_pass, "primeira passada aceitou, a segunda nao roda");
    }
}
