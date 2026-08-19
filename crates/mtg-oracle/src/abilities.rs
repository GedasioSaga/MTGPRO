//! Habilidades de permanente: disparada, ativada, de mana e estática.
//!
//! `effects.rs` traduz a frase que um feitiço executa ao resolver. Aqui está a
//! outra metade: a **moldura** em volta da frase — quando ela acontece
//! (gatilho), o que se paga para ela acontecer (custo de ativação) ou o fato de
//! ela nunca "acontecer" e valer o tempo todo (estática).
//!
//! # Regras que governam este arquivo
//!
//! **Fidelidade acima de cobertura.** Todo reconhecedor casa a frase INTEIRA.
//! Prefixo que casa e sobra texto devolve `None`, e a carta inteira vira
//! `Unsupported`. Não existe habilidade "quase certa": uma criatura marcada
//! jogável cuja habilidade só dispara metade das vezes mente para o jogador e
//! para o log da partida.
//!
//! **Uma linha travada derruba a carta inteira.** O laço de `compile` já é
//! assim, e é de propósito: cada linha de oracle é uma habilidade independente,
//! mas jogar Goblin Chieftain com o `+1/+1` e sem o `haste` é jogar uma carta
//! que não existe. Compilar o que dá e ignorar o resto seria a pior das
//! opções — silenciosa e plausível. Por isso este módulo só devolve `Some` para
//! a linha inteira, e quem não devolve `Some` manda a carta para `Unsupported`
//! com o padrão normalizado no relatório de cobertura.
//!
//! **Só emitimos o que o motor executa.** Todo `Effect`, `TriggerCondition` e
//! `StaticMod` usado aqui foi conferido contra `engine/triggers.rs`,
//! `engine/resolve.rs` e `engine/layers.rs`. O caso mais visível de recusa por
//! esse motivo é `"~ can't be blocked"`: `StaticMod` não tem a variante, e
//! `StaticMod::CantBeBlockedExceptBy` é ignorada em `layers.rs`. Emitir
//! qualquer aproximação ali daria uma criatura bloqueável que o texto diz ser
//! imbloqueável.
//!
//! # Texto de lembrete
//!
//! Não chega aqui: `text::normalize_lines` remove tudo entre parênteses antes
//! de qualquer casamento. `split_cost_effect` mesmo assim ignora `:` dentro de
//! parênteses, porque a separação custo/efeito precisa valer também se um dia
//! alguém casar padrão sobre texto não normalizado.

use mtg_core::card::{
    Ability, ActivatedAbility, ManaAbility, ManaProduction, StaticAbility, StaticMod,
    TriggerCondition, TriggeredAbility,
};
use mtg_core::ir::{
    Condition, Cost, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind, TargetSpec,
    TimingRestriction, Value, ZoneScope,
};
use mtg_core::mana::ManaSymbol;
use mtg_core::types::{CardType, CounterKind};

use crate::effects::Parsed;
use crate::keywords::parse_keyword;
use crate::parse::parse_mana_cost;
use crate::text::{parse_count, parse_signed};

/// Uma linha de oracle vira uma ou mais habilidades, ou nada.
///
/// Devolve `Vec` porque uma linha do oracle pode ser mais de uma habilidade no
/// IR: `"Other Goblin creatures you control get +1/+1 and have haste."` é uma
/// frase e duas `StaticAbility`, já que `StaticMod` modela uma modificação por
/// vez.
///
/// `is_permanent` é a fronteira com `effects.rs`: instantâneo e feitiço não têm
/// habilidade de permanente, e deixar o reconhecedor de estática ver a linha de
/// um feitiço criaria efeito contínuo eterno onde o texto diz "until end of
/// turn".
pub fn parse_ability_line(norm: &str, raw: &str, is_permanent: bool) -> Option<Vec<Ability>> {
    if !is_permanent {
        return None;
    }
    let body = trim_sentence(norm);
    if body.is_empty() {
        return None;
    }
    if let Some(triggered) = parse_triggered(body, raw) {
        return Some(vec![Ability::Triggered(triggered)]);
    }
    if let Some(activated) = parse_activated(body, raw) {
        return Some(activated);
    }
    parse_static(body, raw)
}

fn trim_sentence(text: &str) -> &str {
    let t = text.trim();
    t.strip_suffix('.').unwrap_or(t).trim()
}

/// Corta na primeira `", "`. Serve para separar a cláusula de gatilho do corpo.
fn split_first_comma(text: &str) -> Option<(&str, &str)> {
    let (head, tail) = text.split_once(", ")?;
    let head = head.trim();
    let tail = tail.trim();
    if head.is_empty() || tail.is_empty() {
        return None;
    }
    Some((head, tail))
}

// ---------------------------------------------------------------------------
// Sujeito: o grupo de objetos de que a frase fala
// ---------------------------------------------------------------------------

/// `"another creature you control"`, `"other goblin creatures you control"`,
/// `"a creature"`, `"~"`.
///
/// É o mesmo vocabulário para gatilho ("whenever **another creature you
/// control** dies") e para estática ("**other creatures you control** get
/// +1/+1"), então mora num lugar só.
fn subject(text: &str) -> Option<Selector> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }
    if rest == "~" {
        return Some(Selector::battlefield(Filter::IsSelf));
    }

    let mut other = false;
    for prefix in ["another ", "other ", "each other "] {
        if let Some(r) = rest.strip_prefix(prefix) {
            other = true;
            rest = r;
            break;
        }
    }
    if !other {
        for prefix in ["a ", "an ", "each ", "all "] {
            if let Some(r) = rest.strip_prefix(prefix) {
                rest = r;
                break;
            }
        }
    }

    let mut owner_scope = None;
    for (suffix, who) in [
        (" you control", PlayerRef::You),
        (" an opponent controls", PlayerRef::Opponents),
        (" your opponents control", PlayerRef::Opponents),
    ] {
        if let Some(head) = rest.strip_suffix(suffix) {
            owner_scope = Some(who);
            rest = head.trim();
            break;
        }
    }

    let mut filter = noun_filter(rest)?;
    if other {
        filter = and_also(filter, Filter::IsOther);
    }
    Some(Selector { zone: ZoneScope::Battlefield, filter, owner_scope, max: None })
}

fn and_also(filter: Filter, extra: Filter) -> Filter {
    match filter {
        Filter::And(mut parts) => {
            parts.push(extra);
            Filter::And(parts)
        }
        other => Filter::And(vec![other, extra]),
    }
}

/// O substantivo do sujeito, singular ou plural.
fn noun_filter(text: &str) -> Option<Filter> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(f) = plain_noun(t) {
        return Some(f);
    }
    // "goblin creature" / "goblin creatures"
    if let Some(head) = t
        .strip_suffix(" creatures")
        .or_else(|| t.strip_suffix(" creature"))
    {
        let sub = creature_subtype(head)?;
        return Some(Filter::And(vec![
            Filter::HasType(CardType::Creature),
            Filter::HasSubtype(sub),
        ]));
    }
    // "goblins" / "elves" — o subtipo sozinho, como o oracle escreve nos lordes.
    creature_subtype(t).map(Filter::HasSubtype)
}

fn plain_noun(t: &str) -> Option<Filter> {
    let f = match t {
        "creature" | "creatures" => Filter::HasType(CardType::Creature),
        "permanent" | "permanents" => Filter::Any,
        "artifact" | "artifacts" => Filter::HasType(CardType::Artifact),
        "enchantment" | "enchantments" => Filter::HasType(CardType::Enchantment),
        "land" | "lands" => Filter::HasType(CardType::Land),
        "artifact creature" | "artifact creatures" => Filter::And(vec![
            Filter::HasType(CardType::Artifact),
            Filter::HasType(CardType::Creature),
        ]),
        "nonland permanent" | "nonland permanents" => {
            Filter::Not(Box::new(Filter::HasType(CardType::Land)))
        }
        _ => return None,
    };
    Some(f)
}

/// Vocabulário fechado de subtipo de criatura.
///
/// Lista, e não "qualquer palavra": `"other artifact creatures you control"`
/// tem `artifact` na mesma posição de `goblin`, e aceitar palavra livre daria
/// um `HasSubtype("Artifact")` que nunca casa — um lorde que não levanta
/// ninguém, jogável e errado. Subtipo fora da lista vira `Unsupported`, que é o
/// erro barato.
const CREATURE_SUBTYPES: [&str; 96] = [
    "advisor", "angel", "ape", "archer", "artificer", "assassin", "avatar", "barbarian", "bat",
    "bear", "beast", "berserker", "bird", "boar", "cat", "centaur", "cleric", "cockatrice",
    "construct", "crab", "crocodile", "cyclops", "demon", "devil", "dinosaur", "djinn", "dragon",
    "drake", "druid", "dryad", "dwarf", "efreet", "elemental", "elephant", "elf", "elk", "eye",
    "faerie", "ferret", "fish", "fox", "frog", "fungus", "gargoyle", "giant", "gnome", "goat",
    "goblin", "god", "golem", "gorgon", "griffin", "harpy", "hippo", "horror", "horse", "hound",
    "human", "hydra", "illusion", "imp", "insect", "jellyfish", "kavu", "kirin", "kithkin",
    "knight", "kobold", "kor", "kraken", "leviathan", "lizard", "merfolk", "minotaur", "monk",
    "moonfolk", "mutant", "myr", "naga", "nightmare", "ninja", "nomad", "nymph", "octopus", "ogre",
    "ooze", "orc", "ox", "oyster", "pegasus", "phoenix", "pirate", "plant", "rat", "rebel",
    "rhino",
];

/// Segunda metade do vocabulário. Duas listas porque um único array de 190
/// entradas é ilegível na revisão; a busca varre as duas.
const CREATURE_SUBTYPES_2: [&str; 44] = [
    "rogue", "salamander", "samurai", "satyr", "scarecrow", "scout", "serpent", "shade", "shaman",
    "shapeshifter", "sheep", "skeleton", "slith", "sliver", "snake", "soldier", "spider", "spike",
    "spirit", "sponge", "squid", "squirrel", "starfish", "surrakar", "thopter", "thrull", "treefolk",
    "troll", "turtle", "unicorn", "vampire", "vedalken", "viashino", "wall", "warrior", "weird",
    "werewolf", "whale", "wizard", "wolf", "wolverine", "wombat", "worm", "zombie",
];

/// Plural irregular. O resto sai da regra do `s`/`es`.
const IRREGULAR_PLURALS: [(&str, &str); 12] = [
    ("elves", "elf"),
    ("dwarves", "dwarf"),
    ("wolves", "wolf"),
    ("efreeti", "efreet"),
    ("fungi", "fungus"),
    ("merfolk", "merfolk"),
    ("kithkin", "kithkin"),
    ("moonfolk", "moonfolk"),
    ("samurai", "samurai"),
    ("myr", "myr"),
    ("kor", "kor"),
    ("nymphs", "nymph"),
];

fn is_creature_subtype(word: &str) -> bool {
    CREATURE_SUBTYPES.contains(&word) || CREATURE_SUBTYPES_2.contains(&word)
}

/// Palavra do texto -> subtipo com a grafia do `type_line`, se for subtipo.
fn creature_subtype(word: &str) -> Option<String> {
    let w = word.trim();
    if w.is_empty() || w.contains(' ') {
        return None;
    }
    if is_creature_subtype(w) {
        return Some(capitalize(w));
    }
    for (plural, singular) in IRREGULAR_PLURALS {
        if w == plural && is_creature_subtype(singular) {
            return Some(capitalize(singular));
        }
    }
    for suffix in ["es", "s"] {
        if let Some(head) = w.strip_suffix(suffix) {
            if is_creature_subtype(head) {
                return Some(capitalize(head));
            }
        }
    }
    None
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Habilidades disparadas
// ---------------------------------------------------------------------------

/// `"<gatilho>, [if <condição>,] [you may] <efeito>"`.
fn parse_triggered(body: &str, raw: &str) -> Option<TriggeredAbility> {
    let (head, tail) = split_first_comma(body)?;
    let trigger = trigger_condition(head)?;

    let mut rest = tail;
    // CR 603.4 — condição de intervenção.
    let mut intervening_if = Condition::Always;
    if let Some(after_if) = rest.strip_prefix("if ") {
        let (cond_text, remainder) = split_first_comma(after_if)?;
        intervening_if = intervening_condition(cond_text)?;
        rest = remainder;
    }

    // CR 603.5 — gatilho opcional é confirmado na resolução, não vira
    // `Effect::May`: quem pergunta é a pilha.
    let mut optional = false;
    if let Some(r) = rest.strip_prefix("you may ") {
        optional = true;
        rest = r;
    }

    let parsed = body_effect(rest)?;
    Some(TriggeredAbility {
        trigger,
        intervening_if,
        targets: parsed.targets,
        effect: parsed.effect,
        optional,
        once_per_turn: false,
        // Gatilho de morte da própria fonte já é tratado por
        // `triggers::leaves_battlefield_self`; ligar esta bandeira faria a
        // habilidade disparar também da mão e do exílio, que o texto não diz.
        triggers_from_graveyard: false,
        text: raw.to_string(),
    })
}

fn trigger_condition(head: &str) -> Option<TriggerCondition> {
    if let Some(rest) = head.strip_prefix("at the beginning of ") {
        return step_trigger(rest);
    }
    let rest = head
        .strip_prefix("whenever ")
        .or_else(|| head.strip_prefix("when "))?;
    self_trigger(rest)
        .or_else(|| cast_trigger(rest))
        .or_else(|| event_trigger(rest))
}

/// Gatilho que fala da própria fonte.
fn self_trigger(rest: &str) -> Option<TriggerCondition> {
    let me = || Selector::battlefield(Filter::IsSelf);
    let cond = match rest {
        "~ enters" | "~ enters the battlefield" => TriggerCondition::EntersBattlefield(me()),
        "~ dies" | "~ is put into a graveyard from the battlefield" => {
            TriggerCondition::Dies(me())
        }
        "~ leaves the battlefield" => TriggerCondition::LeavesBattlefield(me()),
        "~ is sacrificed" => TriggerCondition::Sacrificed(me()),
        "~ attacks" => TriggerCondition::Attacks(me()),
        "~ attacks alone" => TriggerCondition::AttacksAlone(me()),
        "~ blocks" => TriggerCondition::Blocks(me()),
        "~ becomes blocked" => TriggerCondition::BecomesBlocked(me()),
        "~ becomes tapped" => TriggerCondition::Taps(me()),
        "~ becomes untapped" => TriggerCondition::Untaps(me()),
        "~ deals combat damage to a player" => TriggerCondition::DealsCombatDamageToPlayer(me()),
        "~ attacks or blocks" => TriggerCondition::Any(vec![
            TriggerCondition::Attacks(me()),
            TriggerCondition::Blocks(me()),
        ]),
        "~ enters or attacks" => TriggerCondition::Any(vec![
            TriggerCondition::EntersBattlefield(me()),
            TriggerCondition::Attacks(me()),
        ]),
        // "~ ou outra criatura" é, em conjunto, "uma criatura qualquer".
        "~ or another creature dies" => TriggerCondition::Dies(Selector::creatures()),
        "~ or another creature you control dies" => {
            TriggerCondition::Dies(Selector::creatures().yours())
        }
        _ => return None,
    };
    Some(cond)
}

/// `"you cast a creature spell"`, `"an opponent casts a spell"`.
fn cast_trigger(rest: &str) -> Option<TriggerCondition> {
    let (who, what) = [
        ("you cast ", Some(PlayerRef::You)),
        ("an opponent casts ", Some(PlayerRef::Opponents)),
        ("a player casts ", None),
    ]
    .into_iter()
    .find_map(|(prefix, who)| rest.strip_prefix(prefix).map(|what| (who, what)))?;
    let mut noun = what.trim();
    for prefix in ["a ", "an ", "another "] {
        if let Some(r) = noun.strip_prefix(prefix) {
            noun = r;
            break;
        }
    }
    let noun = noun.strip_suffix(" spell").or_else(|| noun.strip_suffix("spell"))?;
    let filter = spell_filter(noun.trim())?;
    Some(TriggerCondition::SpellCast(Selector {
        zone: ZoneScope::Stack,
        filter,
        owner_scope: who,
        max: None,
    }))
}

fn spell_filter(noun: &str) -> Option<Filter> {
    let f = match noun {
        "" => Filter::Any,
        "creature" => Filter::HasType(CardType::Creature),
        "noncreature" => Filter::Not(Box::new(Filter::HasType(CardType::Creature))),
        "artifact" => Filter::HasType(CardType::Artifact),
        "enchantment" => Filter::HasType(CardType::Enchantment),
        "instant" => Filter::HasType(CardType::Instant),
        "sorcery" => Filter::HasType(CardType::Sorcery),
        "instant or sorcery" => Filter::Or(vec![
            Filter::HasType(CardType::Instant),
            Filter::HasType(CardType::Sorcery),
        ]),
        other => Filter::HasSubtype(creature_subtype(other)?),
    };
    Some(f)
}

/// Gatilho que fala de um grupo: entra, morre, ataca.
fn event_trigger(rest: &str) -> Option<TriggerCondition> {
    for suffix in [
        " enters the battlefield under your control",
        " enters under your control",
    ] {
        if let Some(head) = rest.strip_suffix(suffix) {
            let sel = scoped(subject(head)?, PlayerRef::You)?;
            return Some(TriggerCondition::EntersBattlefield(sel));
        }
    }
    if let Some(head) = rest
        .strip_suffix(" enters the battlefield")
        .or_else(|| rest.strip_suffix(" enters"))
    {
        return Some(TriggerCondition::EntersBattlefield(subject(head)?));
    }
    if let Some(head) = rest.strip_suffix(" dies") {
        return Some(TriggerCondition::Dies(subject(head)?));
    }
    if let Some(head) = rest.strip_suffix(" attacks") {
        return Some(TriggerCondition::Attacks(subject(head)?));
    }
    if let Some(head) = rest.strip_suffix(" deals combat damage to a player") {
        return Some(TriggerCondition::DealsCombatDamageToPlayer(subject(head)?));
    }
    None
}

/// Escopo escrito depois do verbo ("...enters **under your control**"). Se o
/// sujeito já trouxe escopo, o texto está dizendo duas coisas e não sabemos
/// qual vale.
fn scoped(mut sel: Selector, who: PlayerRef) -> Option<Selector> {
    if sel.owner_scope.is_some() {
        return None;
    }
    sel.owner_scope = Some(who);
    Some(sel)
}

fn step_trigger(rest: &str) -> Option<TriggerCondition> {
    let cond = match rest {
        "your upkeep" => TriggerCondition::BeginningOfUpkeep(PlayerRef::You),
        "each upkeep" | "the upkeep" => TriggerCondition::BeginningOfUpkeep(PlayerRef::Each),
        "each opponent's upkeep" => TriggerCondition::BeginningOfUpkeep(PlayerRef::Opponents),
        "your draw step" => TriggerCondition::BeginningOfDrawStep(PlayerRef::You),
        "your precombat main phase" => {
            TriggerCondition::BeginningOfPrecombatMain(PlayerRef::You)
        }
        "combat on your turn" => TriggerCondition::BeginningOfCombat(PlayerRef::You),
        "your end step" => TriggerCondition::BeginningOfEndStep(PlayerRef::You),
        // "the end step" sem dono é o fim de cada turno. "the NEXT end step" é
        // gatilho atrasado, coisa diferente, e não casa aqui.
        "each end step" | "the end step" => {
            TriggerCondition::BeginningOfEndStep(PlayerRef::Each)
        }
        "each opponent's end step" => TriggerCondition::BeginningOfEndStep(PlayerRef::Opponents),
        _ => return None,
    };
    Some(cond)
}

fn intervening_condition(text: &str) -> Option<Condition> {
    let rest = text.strip_prefix("you control ")?;
    let sel = subject(rest)?;
    // "you control" já é o escopo; sujeito com escopo próprio seria redundante
    // ou contraditório.
    if sel.owner_scope.is_some() {
        return None;
    }
    Some(Condition::YouControlAtLeast(1, sel.filter))
}

// ---------------------------------------------------------------------------
// Habilidades ativadas e de mana
// ---------------------------------------------------------------------------

/// `"<custo>: <efeito>"`.
fn parse_activated(body: &str, raw: &str) -> Option<Vec<Ability>> {
    let (cost_text, effect_text) = split_cost_effect(body)?;
    let cost = parse_cost(cost_text)?;
    let (effect_text, timing) = strip_timing(effect_text);

    // CR 605.3a — habilidade de mana não usa a pilha. Tratá-la como ativada
    // deixaria o oponente responder a um `{T}: Add {G}`, e todo cálculo de
    // mana disponível do bot pararia de fechar.
    if let Some(production) = mana_production(effect_text) {
        if timing != TimingRestriction::Instant {
            return None;
        }
        return Some(vec![Ability::Mana(ManaAbility {
            cost,
            production,
            restriction: Condition::Always,
            text: raw.to_string(),
        })]);
    }

    let parsed = body_effect(effect_text)?;
    Some(vec![Ability::Activated(ActivatedAbility {
        cost,
        targets: parsed.targets,
        effect: parsed.effect,
        timing,
        restriction: Condition::Always,
        uses_per_turn: None,
        loyalty_change: None,
        text: raw.to_string(),
    })])
}

/// Primeiro `:` fora de parênteses.
fn split_cost_effect(body: &str) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    for (i, c) in body.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => {
                let cost = body[..i].trim();
                let effect = trim_sentence(&body[i + 1..]);
                if cost.is_empty() || effect.is_empty() {
                    return None;
                }
                return Some((cost, effect));
            }
            _ => {}
        }
    }
    None
}

fn parse_cost(text: &str) -> Option<Cost> {
    let mut parts = Vec::new();
    for chunk in text.split(',') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            return None;
        }
        parts.push(cost_part(chunk)?);
    }
    match parts.len() {
        0 => None,
        1 => Some(parts.remove(0)),
        _ => Some(Cost::Composite(parts)),
    }
}

fn cost_part(part: &str) -> Option<Cost> {
    match part {
        "{t}" => return Some(Cost::Tap),
        "{q}" => return Some(Cost::Untap),
        "sacrifice ~" => return Some(Cost::Sacrifice(1, Filter::IsSelf)),
        "discard a card" => return Some(Cost::Discard(1, Filter::Any)),
        _ => {}
    }
    if part.starts_with('{') {
        let cost = parse_mana_cost(part)?;
        if cost.symbols.is_empty() || cost.symbols.contains(&ManaSymbol::X) {
            return None;
        }
        return Some(Cost::Mana(cost.symbols));
    }
    if let Some(rest) = part.strip_prefix("pay ") {
        let n = parse_count(rest.strip_suffix(" life")?)?;
        if n <= 0 {
            return None;
        }
        return Some(Cost::PayLife(Value::Const(n)));
    }
    if let Some(rest) = part.strip_prefix("sacrifice ") {
        let sel = subject(rest)?;
        // "sacrifice a creature an opponent controls" não é custo pagável:
        // sacrifício só alcança o que você controla.
        if !matches!(sel.owner_scope, None | Some(PlayerRef::You)) {
            return None;
        }
        return Some(Cost::Sacrifice(1, sel.filter));
    }
    if let Some(rest) = part.strip_prefix("remove ") {
        let inner = rest.strip_suffix(" from ~")?;
        let (count, kind) = counter_phrase(inner)?;
        return Some(Cost::RemoveCounters(count, kind));
    }
    None
}

/// `"a +1/+1 counter"`, `"two +1/+1 counters"`.
fn counter_phrase(text: &str) -> Option<(u8, CounterKind)> {
    let t = text.trim();
    let (count_word, rest) = t.split_once(' ')?;
    let n = parse_count(count_word)?;
    if n <= 0 || n > u8::MAX as i32 {
        return None;
    }
    let kind = match rest.trim() {
        "+1/+1 counter" | "+1/+1 counters" => CounterKind::PlusOnePlusOne,
        "-1/-1 counter" | "-1/-1 counters" => CounterKind::MinusOneMinusOne,
        "charge counter" | "charge counters" => CounterKind::Charge,
        _ => return None,
    };
    Some((n as u8, kind))
}

/// Restrição de tempo escrita como frase depois do efeito.
///
/// Só a redação de feitiço entra. `"Activate only during your turn"` NÃO vira
/// `TimingRestriction::Sorcery`: feitiço exige fase principal e pilha vazia, e
/// a carta que só exige ser seu turno funcionaria no combate. Marcar Sorcery
/// ali seria uma carta mais restrita que a impressa — silenciosamente errada.
fn strip_timing(text: &str) -> (&str, TimingRestriction) {
    for suffix in [
        ". activate only as a sorcery",
        ". activate this ability only as a sorcery",
        ". activate only any time you could cast a sorcery",
        ". activate this ability only any time you could cast a sorcery",
    ] {
        if let Some(head) = text.strip_suffix(suffix) {
            let head = trim_sentence(head);
            if head.is_empty() {
                return (text, TimingRestriction::Instant);
            }
            return (head, TimingRestriction::Sorcery);
        }
    }
    (text, TimingRestriction::Instant)
}

/// `"add {g}"`, `"add {w} or {u}"`, `"add one mana of any color"`.
fn mana_production(text: &str) -> Option<ManaProduction> {
    let rest = text.strip_prefix("add ")?.trim();
    if rest == "one mana of any color" {
        return Some(ManaProduction::AnyColor(1));
    }
    if rest.contains(" or ") {
        let flattened = rest.replace(", or ", " or ").replace(", ", " or ");
        let mut symbols = Vec::new();
        for part in flattened.split(" or ") {
            symbols.push(crate::parse::parse_braced_symbol(part.trim())?);
        }
        return Some(ManaProduction::OneOf(symbols));
    }
    let cost = parse_mana_cost(rest)?;
    if cost.symbols.is_empty() || cost.symbols.contains(&ManaSymbol::X) {
        return None;
    }
    Some(ManaProduction::Fixed(cost.symbols))
}

// ---------------------------------------------------------------------------
// Habilidades estáticas
// ---------------------------------------------------------------------------

/// Verbos que separam sujeito de predicado numa estática.
const STATIC_VERBS: [&str; 5] = [" get ", " gets ", " have ", " has ", " can't "];

fn parse_static(body: &str, raw: &str) -> Option<Vec<Ability>> {
    let index = STATIC_VERBS.iter().filter_map(|v| body.find(v)).min()?;
    let subject_text = body[..index].trim();
    let predicate = body[index + 1..].trim();
    let affects = subject(subject_text)?;
    let mods = static_mods(predicate)?;
    Some(
        mods.into_iter()
            .map(|modification| {
                Ability::Static(StaticAbility {
                    condition: Condition::Always,
                    affects: affects.clone(),
                    modification,
                    text: raw.to_string(),
                })
            })
            .collect(),
    )
}

fn static_mods(predicate: &str) -> Option<Vec<StaticMod>> {
    if let Some(rest) = predicate
        .strip_prefix("get ")
        .or_else(|| predicate.strip_prefix("gets "))
    {
        let (pt, keywords) = match rest.split_once(" and have ") {
            Some((pt, kws)) => (pt, Some(kws)),
            None => match rest.split_once(" and has ") {
                Some((pt, kws)) => (pt, Some(kws)),
                None => (rest, None),
            },
        };
        let mut out = vec![pt_mod(pt)?];
        if let Some(kws) = keywords {
            out.push(StaticMod::GrantKeywords(keyword_list(kws)?));
        }
        return Some(out);
    }
    if let Some(rest) = predicate
        .strip_prefix("have ")
        .or_else(|| predicate.strip_prefix("has "))
    {
        return Some(vec![StaticMod::GrantKeywords(keyword_list(rest)?)]);
    }
    if let Some(rest) = predicate.strip_prefix("can't ") {
        // "can't be blocked" fica de fora de propósito: `StaticMod` não tem a
        // variante e `layers.rs` ignora `CantBeBlockedExceptBy`.
        let mods = match rest.trim() {
            "attack" => vec![StaticMod::CantAttack],
            "block" => vec![StaticMod::CantBlock],
            "attack or block" => vec![StaticMod::CantAttack, StaticMod::CantBlock],
            _ => return None,
        };
        return Some(mods);
    }
    None
}

fn pt_mod(text: &str) -> Option<StaticMod> {
    let (p, t) = text.trim().split_once('/')?;
    Some(StaticMod::ModifyPT(
        Value::Const(parse_signed(p)?),
        Value::Const(parse_signed(t)?),
    ))
}

/// `"flying"`, `"flying and haste"`, `"first strike, vigilance, and trample"`.
fn keyword_list(text: &str) -> Option<Vec<Keyword>> {
    let flattened = text.trim().replace(", and ", " and ").replace(", ", " and ");
    let mut out = Vec::new();
    for token in flattened.split(" and ") {
        out.push(parse_keyword(token.trim())?);
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Corpo de habilidade: efeito
// ---------------------------------------------------------------------------

/// O que uma habilidade executa quando resolve.
///
/// Primeiro tenta o vocabulário de `effects.rs`, que é o mesmo de feitiço.
/// Depois as frases que só aparecem em habilidade. Por último a conjunção
/// `"A and B"`, e só quando as duas metades casam inteiras.
fn body_effect(text: &str) -> Option<Parsed> {
    let body = trim_sentence(text);
    if body.is_empty() {
        return None;
    }
    single_effect(body).or_else(|| conjunction(body))
}

fn single_effect(body: &str) -> Option<Parsed> {
    crate::effects::parse_effect(body).or_else(|| extra_effect(body))
}

/// `"target player loses 1 life and you gain 1 life"`.
///
/// Duas metades com alvo dariam `Target(0)` para as duas, e o motor mandaria os
/// dois efeitos para o mesmo objeto — Blood Artist drenando o mesmo jogador
/// duas vezes. Por isso no máximo uma metade pode pedir alvo.
fn conjunction(body: &str) -> Option<Parsed> {
    let parts: Vec<&str> = body.split(" and ").collect();
    if parts.len() < 2 {
        return None;
    }
    let mut effects = Vec::new();
    let mut targets: Vec<TargetSpec> = Vec::new();
    for part in parts {
        let parsed = single_effect(part.trim())?;
        if !parsed.targets.is_empty() {
            if !targets.is_empty() {
                return None;
            }
            targets = parsed.targets;
        }
        effects.push(parsed.effect);
    }
    Some(Parsed { effect: Effect::Sequence(effects), targets })
}

const FIRST_TARGET: ObjRef = ObjRef::Target(0);

fn spec(kind: TargetKind, description: &str) -> TargetSpec {
    TargetSpec { kind, description: description.to_string() }
}

fn t_creature() -> TargetSpec {
    spec(TargetKind::Object(Selector::creatures()), "target creature")
}

fn t_player() -> TargetSpec {
    spec(TargetKind::Player(PlayerRef::Each), "target player")
}

fn plain(effect: Effect) -> Option<Parsed> {
    Some(Parsed { effect, targets: Vec::new() })
}

fn targeted(effect: Effect, target: TargetSpec) -> Option<Parsed> {
    Some(Parsed { effect, targets: vec![target] })
}

/// Frases que aparecem em corpo de habilidade e não em `effects.rs`.
fn extra_effect(body: &str) -> Option<Parsed> {
    match body {
        "tap target creature" => {
            return targeted(Effect::Tap { target: FIRST_TARGET }, t_creature())
        }
        "untap target creature" => {
            return targeted(Effect::Untap { target: FIRST_TARGET }, t_creature())
        }
        "exile target creature" => {
            return targeted(
                Effect::Exile { target: FIRST_TARGET, until_source_leaves: false },
                t_creature(),
            )
        }
        "return target creature to its owner's hand" => {
            return targeted(Effect::ReturnToHand { target: FIRST_TARGET }, t_creature())
        }
        "return ~ to its owner's hand" => {
            return plain(Effect::ReturnToHand { target: ObjRef::SelfObject })
        }
        "sacrifice ~" => {
            return plain(Effect::Sacrifice {
                player: PlayerRef::You,
                count: Value::Const(1),
                filter: Filter::IsSelf,
            })
        }
        "untap ~" => return plain(Effect::Untap { target: ObjRef::SelfObject }),
        "tap ~" => return plain(Effect::Tap { target: ObjRef::SelfObject }),
        _ => {}
    }

    if let Some(rest) = body.strip_prefix("target player loses ") {
        let n = parse_count(rest.strip_suffix(" life")?)?;
        return targeted(
            Effect::LoseLife { amount: Value::Const(n), player: PlayerRef::Target(0) },
            t_player(),
        );
    }
    if let Some(rest) = body.strip_prefix("each opponent loses ") {
        let n = parse_count(rest.strip_suffix(" life")?)?;
        return plain(Effect::LoseLife {
            amount: Value::Const(n),
            player: PlayerRef::Opponents,
        });
    }
    if let Some(rest) = body.strip_prefix("you lose ") {
        let n = parse_count(rest.strip_suffix(" life")?)?;
        return plain(Effect::LoseLife { amount: Value::Const(n), player: PlayerRef::You });
    }
    if let Some(rest) = body.strip_prefix("you gain ") {
        let n = parse_count(rest.strip_suffix(" life")?)?;
        return plain(Effect::GainLife { amount: Value::Const(n), player: PlayerRef::You });
    }
    if let Some(rest) = body.strip_prefix("you draw ") {
        let n = parse_count(
            rest.strip_suffix(" cards")
                .or_else(|| rest.strip_suffix(" card"))?,
        )?;
        return plain(Effect::DrawCards { count: Value::Const(n), player: PlayerRef::You });
    }
    if let Some(rest) = body.strip_prefix("target player draws ") {
        let n = parse_count(
            rest.strip_suffix(" cards")
                .or_else(|| rest.strip_suffix(" card"))?,
        )?;
        return targeted(
            Effect::DrawCards { count: Value::Const(n), player: PlayerRef::Target(0) },
            t_player(),
        );
    }
    if let Some(rest) = body.strip_prefix("scry ") {
        let n = parse_count(rest)?;
        return plain(Effect::Scry { count: Value::Const(n), player: PlayerRef::You });
    }
    if let Some(rest) = body.strip_prefix("put ") {
        if let Some(inner) = rest.strip_suffix(" on ~") {
            let (count, kind) = counter_phrase(inner)?;
            return plain(Effect::AddCounters {
                target: ObjRef::SelfObject,
                kind,
                count: Value::Const(count as i32),
            });
        }
        if let Some(inner) = rest.strip_suffix(" on target creature") {
            let (count, kind) = counter_phrase(inner)?;
            return targeted(
                Effect::AddCounters {
                    target: FIRST_TARGET,
                    kind,
                    count: Value::Const(count as i32),
                },
                t_creature(),
            );
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile, CompileResult, OracleCard};
    use mtg_core::card::CardDef;
    use mtg_core::mana::Color;

    fn playable(card: &OracleCard) -> CardDef {
        match compile(card) {
            CompileResult::Playable(def) => def,
            CompileResult::Unsupported { reason, pattern } => {
                panic!("esperava jogável, veio Unsupported: {reason} / {pattern}")
            }
        }
    }

    fn rejected(card: &OracleCard) -> String {
        match compile(card) {
            CompileResult::Playable(def) => {
                panic!("esperava Unsupported, compilou: {:?}", def.abilities)
            }
            CompileResult::Unsupported { reason, .. } => reason,
        }
    }

    fn creature(name: &str, cost: &str, types: &str, text: &str, p: &str, t: &str) -> OracleCard {
        OracleCard::new(name, cost, types, text).with_pt(p, t)
    }

    fn triggered_of(def: &CardDef, index: usize) -> &TriggeredAbility {
        match &def.abilities[index] {
            Ability::Triggered(t) => t,
            other => panic!("habilidade {index} deveria ser disparada: {other:?}"),
        }
    }

    fn activated_of(def: &CardDef, index: usize) -> &ActivatedAbility {
        match &def.abilities[index] {
            Ability::Activated(a) => a,
            other => panic!("habilidade {index} deveria ser ativada: {other:?}"),
        }
    }

    fn static_of(def: &CardDef, index: usize) -> &StaticAbility {
        match &def.abilities[index] {
            Ability::Static(s) => s,
            other => panic!("habilidade {index} deveria ser estática: {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // As oito cartas nomeadas
    // -----------------------------------------------------------------------

    #[test]
    fn wall_of_omens_dispara_etb_e_compra() {
        let def = playable(&creature(
            "Wall of Omens",
            "{1}{W}",
            "Creature — Wall",
            "Defender\nWhen Wall of Omens enters, draw a card.",
            "0",
            "4",
        ));
        assert_eq!(def.abilities.len(), 2);
        assert_eq!(def.abilities[0], Ability::Keyword(Keyword::Defender));
        let trig = triggered_of(&def, 1);
        assert_eq!(
            trig.trigger,
            TriggerCondition::EntersBattlefield(Selector::battlefield(Filter::IsSelf))
        );
        assert_eq!(
            trig.effect,
            Effect::DrawCards { count: Value::Const(1), player: PlayerRef::You }
        );
        assert!(!trig.optional);
    }

    #[test]
    fn serra_angel_e_so_palavra_chave() {
        let def = playable(&creature(
            "Serra Angel",
            "{3}{W}{W}",
            "Creature — Angel",
            "Flying, vigilance",
            "4",
            "4",
        ));
        assert_eq!(
            def.abilities,
            vec![
                Ability::Keyword(Keyword::Flying),
                Ability::Keyword(Keyword::Vigilance),
            ]
        );
    }

    #[test]
    fn llanowar_elves_vira_habilidade_de_mana_e_nao_ativada() {
        let def = playable(&creature(
            "Llanowar Elves",
            "{G}",
            "Creature — Elf Druid",
            "{T}: Add {G}.",
            "1",
            "1",
        ));
        // CR 605.3a: mana não passa pela pilha. Se isto virar `Activated`, o
        // oponente ganha janela de resposta que não existe.
        assert_eq!(def.abilities.len(), 1);
        let Ability::Mana(mana) = &def.abilities[0] else {
            panic!("esperava habilidade de mana: {:?}", def.abilities[0]);
        };
        assert_eq!(mana.cost, Cost::Tap);
        assert_eq!(
            mana.production,
            ManaProduction::Fixed(vec![ManaSymbol::Colored(Color::Green)])
        );
    }

    #[test]
    fn glorious_anthem_levanta_so_as_suas_criaturas() {
        let def = playable(&OracleCard::new(
            "Glorious Anthem",
            "{1}{W}{W}",
            "Enchantment",
            "Creatures you control get +1/+1.",
        ));
        assert_eq!(def.abilities.len(), 1);
        let st = static_of(&def, 0);
        assert_eq!(st.affects.filter, Filter::HasType(CardType::Creature));
        assert_eq!(st.affects.owner_scope, Some(PlayerRef::You));
        assert_eq!(st.affects.zone, ZoneScope::Battlefield);
        assert_eq!(
            st.modification,
            StaticMod::ModifyPT(Value::Const(1), Value::Const(1))
        );
        assert_eq!(st.condition, Condition::Always);
    }

    #[test]
    fn prodigal_sorcerer_vira_ativada_com_custo_de_virar() {
        let def = playable(&creature(
            "Prodigal Sorcerer",
            "{2}{U}",
            "Creature — Human Wizard",
            "{T}: Prodigal Sorcerer deals 1 damage to any target.",
            "1",
            "1",
        ));
        assert_eq!(def.abilities.len(), 1);
        let ab = activated_of(&def, 0);
        assert_eq!(ab.cost, Cost::Tap);
        assert_eq!(
            ab.effect,
            Effect::DealDamage { amount: Value::Const(1), target: ObjRef::Target(0) }
        );
        assert_eq!(ab.targets.len(), 1);
        assert_eq!(ab.targets[0].description, "any target");
        assert_eq!(ab.timing, TimingRestriction::Instant);
    }

    #[test]
    fn blood_artist_dispara_com_qualquer_criatura_e_dreno_na_ordem_do_texto() {
        let def = playable(&creature(
            "Blood Artist",
            "{1}{B}",
            "Creature — Vampire",
            "Whenever Blood Artist or another creature dies, target player loses 1 life and you gain 1 life.",
            "0",
            "1",
        ));
        let trig = triggered_of(&def, 0);
        // "~ ou outra criatura" cobre a morte da própria fonte; o gatilho de
        // saída de campo funciona do cemitério sem `triggers_from_graveyard`.
        assert_eq!(trig.trigger, TriggerCondition::Dies(Selector::creatures()));
        assert!(!trig.triggers_from_graveyard);
        assert_eq!(
            trig.effect,
            Effect::Sequence(vec![
                Effect::LoseLife { amount: Value::Const(1), player: PlayerRef::Target(0) },
                Effect::GainLife { amount: Value::Const(1), player: PlayerRef::You },
            ])
        );
        assert_eq!(trig.targets.len(), 1);
        assert!(matches!(trig.targets[0].kind, TargetKind::Player(_)));
    }

    #[test]
    fn elvish_visionary_compra_ao_entrar() {
        let def = playable(&creature(
            "Elvish Visionary",
            "{1}{G}",
            "Creature — Elf Shaman",
            "When Elvish Visionary enters, draw a card.",
            "1",
            "1",
        ));
        let trig = triggered_of(&def, 0);
        assert_eq!(
            trig.effect,
            Effect::DrawCards { count: Value::Const(1), player: PlayerRef::You }
        );
    }

    #[test]
    fn goblin_chieftain_vira_duas_estaticas_que_nao_alcancam_ele_mesmo() {
        let def = playable(&creature(
            "Goblin Chieftain",
            "{1}{R}{R}",
            "Creature — Goblin",
            "Haste\nOther Goblin creatures you control get +1/+1 and have haste.",
            "2",
            "2",
        ));
        assert_eq!(def.abilities.len(), 3);
        assert_eq!(def.abilities[0], Ability::Keyword(Keyword::Haste));

        let esperado = Filter::And(vec![
            Filter::HasType(CardType::Creature),
            Filter::HasSubtype("Goblin".to_string()),
            Filter::IsOther,
        ]);
        let pt = static_of(&def, 1);
        assert_eq!(pt.affects.filter, esperado);
        assert_eq!(pt.affects.owner_scope, Some(PlayerRef::You));
        assert_eq!(
            pt.modification,
            StaticMod::ModifyPT(Value::Const(1), Value::Const(1))
        );

        let kw = static_of(&def, 2);
        assert_eq!(kw.affects.filter, esperado);
        assert_eq!(kw.modification, StaticMod::GrantKeywords(vec![Keyword::Haste]));
    }

    // -----------------------------------------------------------------------
    // Disparadas: as formas do oracle
    // -----------------------------------------------------------------------

    #[test]
    fn formas_de_gatilho_viram_a_condicao_certa() {
        let me = || Selector::battlefield(Filter::IsSelf);
        let casos: Vec<(&str, TriggerCondition)> = vec![
            ("Whenever ~ attacks", TriggerCondition::Attacks(me())),
            (
                "Whenever ~ deals combat damage to a player",
                TriggerCondition::DealsCombatDamageToPlayer(me()),
            ),
            ("Whenever ~ dies", TriggerCondition::Dies(me())),
            ("When ~ dies", TriggerCondition::Dies(me())),
            ("When ~ leaves the battlefield", TriggerCondition::LeavesBattlefield(me())),
            ("Whenever ~ becomes tapped", TriggerCondition::Taps(me())),
            (
                "Whenever ~ attacks or blocks",
                TriggerCondition::Any(vec![
                    TriggerCondition::Attacks(me()),
                    TriggerCondition::Blocks(me()),
                ]),
            ),
            (
                "Whenever another creature enters the battlefield under your control",
                TriggerCondition::EntersBattlefield(Selector {
                    zone: ZoneScope::Battlefield,
                    filter: Filter::And(vec![
                        Filter::HasType(CardType::Creature),
                        Filter::IsOther,
                    ]),
                    owner_scope: Some(PlayerRef::You),
                    max: None,
                }),
            ),
            (
                "Whenever a creature you control dies",
                TriggerCondition::Dies(Selector::creatures().yours()),
            ),
            (
                "Whenever a creature an opponent controls dies",
                TriggerCondition::Dies(Selector::creatures().opponents()),
            ),
            (
                "Whenever you cast a creature spell",
                TriggerCondition::SpellCast(Selector {
                    zone: ZoneScope::Stack,
                    filter: Filter::HasType(CardType::Creature),
                    owner_scope: Some(PlayerRef::You),
                    max: None,
                }),
            ),
            (
                "Whenever you cast an instant or sorcery spell",
                TriggerCondition::SpellCast(Selector {
                    zone: ZoneScope::Stack,
                    filter: Filter::Or(vec![
                        Filter::HasType(CardType::Instant),
                        Filter::HasType(CardType::Sorcery),
                    ]),
                    owner_scope: Some(PlayerRef::You),
                    max: None,
                }),
            ),
            (
                "At the beginning of your upkeep",
                TriggerCondition::BeginningOfUpkeep(PlayerRef::You),
            ),
            (
                "At the beginning of your end step",
                TriggerCondition::BeginningOfEndStep(PlayerRef::You),
            ),
            (
                "At the beginning of each opponent's upkeep",
                TriggerCondition::BeginningOfUpkeep(PlayerRef::Opponents),
            ),
        ];

        for (head, esperado) in casos {
            let text = format!("{head}, draw a card.");
            let def = playable(&creature(
                "Tester",
                "{1}{U}",
                "Creature — Human",
                &text,
                "1",
                "1",
            ));
            let trig = triggered_of(&def, 0);
            assert_eq!(trig.trigger, esperado, "falhou em {head:?}");
            assert_eq!(
                trig.effect,
                Effect::DrawCards { count: Value::Const(1), player: PlayerRef::You },
                "falhou no corpo de {head:?}"
            );
        }
    }

    #[test]
    fn you_may_vira_gatilho_opcional_e_nao_efeito_extra() {
        let def = playable(&creature(
            "Hesitant Scout",
            "{1}{G}",
            "Creature — Human Scout",
            "Whenever Hesitant Scout attacks, you may draw a card.",
            "2",
            "2",
        ));
        let trig = triggered_of(&def, 0);
        assert!(trig.optional);
        assert_eq!(
            trig.effect,
            Effect::DrawCards { count: Value::Const(1), player: PlayerRef::You }
        );
    }

    #[test]
    fn condicao_de_intervencao_vira_intervening_if() {
        let def = playable(&creature(
            "Goblin Lookout",
            "{1}{R}",
            "Creature — Goblin Scout",
            "When Goblin Lookout enters, if you control a Goblin, draw a card.",
            "1",
            "1",
        ));
        let trig = triggered_of(&def, 0);
        assert_eq!(
            trig.intervening_if,
            Condition::YouControlAtLeast(1, Filter::HasSubtype("Goblin".to_string()))
        );
        assert_eq!(
            trig.effect,
            Effect::DrawCards { count: Value::Const(1), player: PlayerRef::You }
        );
    }

    #[test]
    fn phyrexian_arena_compila_a_conjuncao_na_ordem_do_texto() {
        let def = playable(&OracleCard::new(
            "Phyrexian Arena",
            "{1}{B}{B}",
            "Enchantment",
            "At the beginning of your upkeep, you draw a card and you lose 1 life.",
        ));
        let trig = triggered_of(&def, 0);
        assert_eq!(
            trig.trigger,
            TriggerCondition::BeginningOfUpkeep(PlayerRef::You)
        );
        assert_eq!(
            trig.effect,
            Effect::Sequence(vec![
                Effect::DrawCards { count: Value::Const(1), player: PlayerRef::You },
                Effect::LoseLife { amount: Value::Const(1), player: PlayerRef::You },
            ])
        );
    }

    // -----------------------------------------------------------------------
    // Ativadas
    // -----------------------------------------------------------------------

    #[test]
    fn custo_composto_separa_mana_de_virar() {
        let def = playable(&creature(
            "Pinger Adept",
            "{3}{U}",
            "Creature — Merfolk",
            "{2}{R}, {T}: Pinger Adept deals 2 damage to any target.",
            "1",
            "4",
        ));
        let ab = activated_of(&def, 0);
        assert_eq!(
            ab.cost,
            Cost::Composite(vec![
                Cost::Mana(vec![
                    ManaSymbol::Generic(2),
                    ManaSymbol::Colored(Color::Red)
                ]),
                Cost::Tap,
            ])
        );
        assert_eq!(
            ab.effect,
            Effect::DealDamage { amount: Value::Const(2), target: ObjRef::Target(0) }
        );
    }

    #[test]
    fn sacrificio_da_propria_fonte_e_custo_e_nao_efeito() {
        let def = playable(&creature(
            "Mogg Fanatic",
            "{R}",
            "Creature — Goblin",
            "Sacrifice Mogg Fanatic: Mogg Fanatic deals 1 damage to any target.",
            "1",
            "1",
        ));
        let ab = activated_of(&def, 0);
        assert_eq!(ab.cost, Cost::Sacrifice(1, Filter::IsSelf));
        assert_eq!(
            ab.effect,
            Effect::DealDamage { amount: Value::Const(1), target: ObjRef::Target(0) }
        );
    }

    #[test]
    fn descarte_no_custo_e_alvo_no_efeito() {
        let def = playable(&creature(
            "Discard Hound",
            "{1}{G}",
            "Creature — Hound",
            "{1}, Discard a card: Target creature gets +1/+1 until end of turn.",
            "2",
            "2",
        ));
        let ab = activated_of(&def, 0);
        assert_eq!(
            ab.cost,
            Cost::Composite(vec![
                Cost::Mana(vec![ManaSymbol::Generic(1)]),
                Cost::Discard(1, Filter::Any),
            ])
        );
        assert_eq!(ab.targets.len(), 1);
    }

    #[test]
    fn activate_only_as_a_sorcery_vira_timing_de_feitico() {
        let def = playable(&creature(
            "Slow Sage",
            "{2}{U}",
            "Creature — Human Wizard",
            "{2}{U}: Draw a card. Activate only as a sorcery.",
            "1",
            "3",
        ));
        let ab = activated_of(&def, 0);
        assert_eq!(ab.timing, TimingRestriction::Sorcery);
        assert_eq!(
            ab.effect,
            Effect::DrawCards { count: Value::Const(1), player: PlayerRef::You }
        );
    }

    #[test]
    fn habilidade_de_mana_com_custo_alem_de_virar_continua_sendo_mana() {
        let def = playable(&OracleCard::new(
            "Mana Drum",
            "",
            "Artifact",
            "{1}, {T}: Add {G}.",
        ));
        let Ability::Mana(mana) = &def.abilities[0] else {
            panic!("esperava habilidade de mana: {:?}", def.abilities[0]);
        };
        assert_eq!(
            mana.cost,
            Cost::Composite(vec![
                Cost::Mana(vec![ManaSymbol::Generic(1)]),
                Cost::Tap
            ])
        );
        assert_eq!(
            mana.production,
            ManaProduction::Fixed(vec![ManaSymbol::Colored(Color::Green)])
        );
    }

    #[test]
    fn efeitos_de_corpo_de_habilidade_viram_o_ir_correspondente() {
        let casos: Vec<(&str, Effect)> = vec![
            ("tap target creature", Effect::Tap { target: ObjRef::Target(0) }),
            ("untap target creature", Effect::Untap { target: ObjRef::Target(0) }),
            (
                "exile target creature",
                Effect::Exile { target: ObjRef::Target(0), until_source_leaves: false },
            ),
            (
                "return target creature to its owner's hand",
                Effect::ReturnToHand { target: ObjRef::Target(0) },
            ),
            (
                "target player loses 2 life",
                Effect::LoseLife { amount: Value::Const(2), player: PlayerRef::Target(0) },
            ),
            (
                "each opponent loses 1 life",
                Effect::LoseLife { amount: Value::Const(1), player: PlayerRef::Opponents },
            ),
            ("scry 2", Effect::Scry { count: Value::Const(2), player: PlayerRef::You }),
            (
                "put a +1/+1 counter on ~",
                Effect::AddCounters {
                    target: ObjRef::SelfObject,
                    kind: CounterKind::PlusOnePlusOne,
                    count: Value::Const(1),
                },
            ),
            (
                "put two +1/+1 counters on target creature",
                Effect::AddCounters {
                    target: ObjRef::Target(0),
                    kind: CounterKind::PlusOnePlusOne,
                    count: Value::Const(2),
                },
            ),
        ];
        for (frase, esperado) in casos {
            let text = format!("{{T}}: {frase}.");
            let def = playable(&creature(
                "Tester",
                "{1}",
                "Creature — Human",
                &text,
                "1",
                "1",
            ));
            assert_eq!(activated_of(&def, 0).effect, esperado, "falhou em {frase:?}");
        }
    }

    // -----------------------------------------------------------------------
    // Estáticas
    // -----------------------------------------------------------------------

    #[test]
    fn lordes_com_subtipo_no_plural_tambem_compilam() {
        let def = playable(&creature(
            "Elf Captain",
            "{1}{G}{G}",
            "Creature — Elf Druid",
            "Other Elves you control get +1/+1.",
            "2",
            "2",
        ));
        let st = static_of(&def, 0);
        assert_eq!(
            st.affects.filter,
            Filter::And(vec![
                Filter::HasSubtype("Elf".to_string()),
                Filter::IsOther
            ])
        );
        assert_eq!(st.affects.owner_scope, Some(PlayerRef::You));
    }

    #[test]
    fn concessao_de_palavra_chave_sem_pt() {
        let def = playable(&creature(
            "Sky Marshal",
            "{2}{W}",
            "Creature — Human Soldier",
            "Soldier creatures you control have vigilance.",
            "2",
            "2",
        ));
        let st = static_of(&def, 0);
        assert_eq!(
            st.modification,
            StaticMod::GrantKeywords(vec![Keyword::Vigilance])
        );
        assert_eq!(
            st.affects.filter,
            Filter::And(vec![
                Filter::HasType(CardType::Creature),
                Filter::HasSubtype("Soldier".to_string()),
            ])
        );
    }

    #[test]
    fn cant_attack_or_block_vira_duas_restricoes() {
        let def = playable(&creature(
            "Sluggish Golem",
            "{4}",
            "Artifact Creature — Golem",
            "Sluggish Golem can't attack or block.",
            "4",
            "4",
        ));
        assert_eq!(def.abilities.len(), 2);
        assert_eq!(static_of(&def, 0).modification, StaticMod::CantAttack);
        assert_eq!(static_of(&def, 1).modification, StaticMod::CantBlock);
        assert_eq!(
            static_of(&def, 0).affects,
            Selector::battlefield(Filter::IsSelf)
        );
    }

    #[test]
    fn estatica_de_oponente_usa_o_escopo_do_oponente() {
        let def = playable(&OracleCard::new(
            "Withering Aura",
            "{2}{B}",
            "Enchantment",
            "Creatures your opponents control get -1/-1.",
        ));
        let st = static_of(&def, 0);
        assert_eq!(st.affects.owner_scope, Some(PlayerRef::Opponents));
        assert_eq!(
            st.modification,
            StaticMod::ModifyPT(Value::Const(-1), Value::Const(-1))
        );
    }

    // -----------------------------------------------------------------------
    // Texto de lembrete
    // -----------------------------------------------------------------------

    #[test]
    fn lembrete_entre_parenteses_nao_atrapalha_o_casamento() {
        let def = playable(&creature(
            "Reminder Elf",
            "{G}",
            "Creature — Elf Druid",
            "{T}: Add {G}. (This is a mana ability: it doesn't use the stack.)",
            "1",
            "1",
        ));
        assert!(matches!(def.abilities[0], Ability::Mana(_)));

        // O lembrete de vigilance carrega vírgula e ponto, e mesmo assim a
        // linha continua sendo uma estática inteira.
        let anthem = playable(&OracleCard::new(
            "Reminder Anthem",
            "{2}{W}",
            "Enchantment",
            "Creatures you control have vigilance. (Attacking doesn't cause them to tap.)",
        ));
        assert_eq!(
            static_of(&anthem, 0).modification,
            StaticMod::GrantKeywords(vec![Keyword::Vigilance])
        );
    }

    // -----------------------------------------------------------------------
    // O que NÃO pode compilar
    // -----------------------------------------------------------------------

    #[test]
    fn linha_travada_derruba_a_carta_inteira() {
        // A primeira linha compila sozinha; a segunda não. Jogar com metade das
        // habilidades é jogar outra carta.
        let reason = rejected(&creature(
            "Half Understood",
            "{1}{G}",
            "Creature — Elf Warrior",
            "Other Elves you control get +1/+1.\nWhenever Half Understood becomes the target of a spell, its controller may pay {2}.",
            "2",
            "2",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    #[test]
    fn cant_be_blocked_fica_de_fora_porque_o_ir_nao_tem_a_variante() {
        // `StaticMod` não tem `CantBeBlocked` e `layers.rs` ignora
        // `CantBeBlockedExceptBy`. Emitir qualquer aproximação daria uma
        // criatura bloqueável com texto dizendo o contrário.
        let reason = rejected(&creature(
            "Sneaky Beast",
            "{U}",
            "Creature — Beast",
            "Sneaky Beast can't be blocked.",
            "1",
            "1",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    #[test]
    fn subtipo_fora_do_vocabulario_nao_vira_lorde_vazio() {
        let reason = rejected(&creature(
            "Assembly Lord",
            "{3}",
            "Artifact Creature — Assembly-Worker",
            "Other Assembly-Workers you control get +1/+1.",
            "2",
            "2",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    #[test]
    fn palavra_de_tipo_no_lugar_de_subtipo_nao_vira_has_subtype() {
        // "artifact creatures" tem `artifact` na posição de `goblin`. Aceitar
        // palavra livre daria `HasSubtype("Artifact")`, que nunca casa.
        let def = playable(&OracleCard::new(
            "Forge Anthem",
            "{2}{W}",
            "Enchantment",
            "Artifact creatures you control get +1/+1.",
        ));
        assert_eq!(
            static_of(&def, 0).affects.filter,
            Filter::And(vec![
                Filter::HasType(CardType::Artifact),
                Filter::HasType(CardType::Creature),
            ])
        );
    }

    #[test]
    fn activate_only_during_your_turn_nao_vira_timing_de_feitico() {
        // Feitiço exige fase principal e pilha vazia; "só no seu turno" não.
        // Marcar Sorcery deixaria a carta mais restrita que a impressa.
        let reason = rejected(&creature(
            "Turn Bound",
            "{1}{R}",
            "Creature — Goblin",
            "{T}: Turn Bound deals 1 damage to any target. Activate only during your turn.",
            "1",
            "1",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    #[test]
    fn dois_alvos_na_mesma_conjuncao_sao_recusados() {
        // Os dois `Target(0)` apontariam para o mesmo objeto.
        let reason = rejected(&creature(
            "Double Trouble",
            "{2}{U}",
            "Creature — Human Wizard",
            "{T}: Tap target creature and untap target creature.",
            "1",
            "1",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    #[test]
    fn estatica_com_condicao_nao_modelada_e_recusada() {
        let reason = rejected(&creature(
            "Fickle Lord",
            "{2}{G}",
            "Creature — Elf",
            "Other Elves you control get +1/+1 as long as you control a Forest.",
            "2",
            "2",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    #[test]
    fn aura_com_enchanted_creature_nao_compila() {
        // `StaticAbility::affects` é um `Selector`; não existe seletor para "o
        // que esta aura encanta".
        let reason = rejected(&OracleCard::new(
            "Holy Strength",
            "{W}",
            "Enchantment — Aura",
            "Enchant creature\nEnchanted creature gets +1/+2.",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    #[test]
    fn feitico_nao_ganha_habilidade_de_permanente() {
        // A mesma frase num feitiço seria efeito contínuo eterno.
        let reason = rejected(&OracleCard::new(
            "Odd Sorcery",
            "{1}{W}",
            "Sorcery",
            "Creatures you control get +1/+1.",
        ));
        assert!(!reason.is_empty());
    }

    #[test]
    fn gatilho_atrasado_nao_e_gatilho_de_permanente() {
        // "the next end step" é gatilho atrasado criado por uma resolução, não
        // uma habilidade da carta.
        let reason = rejected(&creature(
            "Delayed Golem",
            "{3}",
            "Artifact Creature — Golem",
            "At the beginning of the next end step, draw a card.",
            "3",
            "3",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    #[test]
    fn corpo_de_gatilho_desconhecido_nao_vira_gatilho_vazio() {
        let reason = rejected(&creature(
            "Mystery Bird",
            "{1}{U}",
            "Creature — Bird",
            "Whenever Mystery Bird attacks, roll a six-sided die.",
            "2",
            "1",
        ));
        assert_eq!(reason, "linha de habilidade não reconhecida");
    }

    // -----------------------------------------------------------------------
    // Determinismo
    // -----------------------------------------------------------------------

    #[test]
    fn mesma_entrada_da_mesma_saida() {
        let card = creature(
            "Goblin Chieftain",
            "{1}{R}{R}",
            "Creature — Goblin",
            "Haste\nOther Goblin creatures you control get +1/+1 and have haste.",
            "2",
            "2",
        );
        let a = compile(&card);
        let b = compile(&card);
        assert_eq!(a, b);
    }
}
