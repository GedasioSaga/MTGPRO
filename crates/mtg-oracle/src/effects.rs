//! Frase de efeito -> IR.
//!
//! Regra que vale para todo reconhecedor daqui: casamento é da frase INTEIRA.
//! Prefixo que casa e sobra texto devolve `None`. Uma carta ausente do catálogo
//! custa uma carta; uma carta que joga diferente do que está escrito quebra a
//! partida em silêncio.
//!
//! # Como o texto é quebrado
//!
//! Um parágrafo vira uma lista de frases separadas por ponto, e cada frase vira
//! um efeito; duas ou mais viram `Sequence`. É por isso que "Gain control of
//! target creature until end of turn. Untap that creature. It gains haste until
//! end of turn." não precisa de um padrão próprio: são três frases que já
//! existem, mais a regra de que "it"/"that creature" apontam para o alvo que a
//! frase anterior escolheu.
//!
//! # Alvos
//!
//! Os alvos são acumulados em [`Ctx`] na ordem em que aparecem no texto, e o
//! índice devolvido é o mesmo que o IR usa em `ObjRef::Target`. Reconhecedor
//! que falha no meio desfaz o que registrou (`checkpoint`/`rollback`), senão a
//! próxima tentativa herdaria um alvo fantasma e os índices sairiam trocados.

use mtg_core::ir::{
    Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TargetKind, TargetSpec, Value,
};
use mtg_core::types::{CardType, CounterKind};

use crate::keywords::parse_keyword;
use crate::parse::parse_mana_cost;
use crate::text::{parse_count, parse_signed};

mod phrases;
mod tokens;

#[cfg(test)]
mod tests;

use phrases::{
    any_target, creature_or_player, filter_phrase, graveyard_phrase, object_phrase, player_phrase,
    split_count,
};
use tokens::parse_create_token;

/// Efeito reconhecido junto com os alvos que ele exige.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub effect: Effect,
    pub targets: Vec<TargetSpec>,
}

/// Teto de alvos numa habilidade. Texto que pede mais que isto sempre vem com
/// construção que este compilador não entende, e o teto evita que um erro de
/// reconhecimento gere uma carta com dez alvos.
const MAX_TARGETS: usize = 4;

/// Acumulador de alvos de uma habilidade.
#[derive(Debug, Default)]
struct Ctx {
    specs: Vec<TargetSpec>,
    /// Último alvo de objeto registrado, para resolver "it"/"that creature".
    last_object: Option<u8>,
}

impl Ctx {
    fn push(&mut self, kind: TargetKind, description: &str) -> Option<u8> {
        if self.specs.len() >= MAX_TARGETS {
            return None;
        }
        let index = u8::try_from(self.specs.len()).ok()?;
        self.specs.push(TargetSpec { kind, description: description.to_string() });
        Some(index)
    }

    fn object(&mut self, selector: Selector, description: &str) -> Option<ObjRef> {
        let index = self.push(TargetKind::Object(selector), description)?;
        self.last_object = Some(index);
        Some(ObjRef::Target(index))
    }

    fn object_or_player(
        &mut self,
        selector: Selector,
        players: PlayerRef,
        description: &str,
    ) -> Option<ObjRef> {
        let index = self.push(TargetKind::ObjectOrPlayer(selector, players), description)?;
        self.last_object = Some(index);
        Some(ObjRef::Target(index))
    }

    fn player(&mut self, pool: PlayerRef, description: &str) -> Option<PlayerRef> {
        let index = self.push(TargetKind::Player(pool), description)?;
        Some(PlayerRef::Target(index))
    }

    fn spell(&mut self, filter: Filter, description: &str) -> Option<ObjRef> {
        let index = self.push(TargetKind::SpellOnStack(filter), description)?;
        Some(ObjRef::Target(index))
    }

    fn last_object(&self) -> Option<ObjRef> {
        self.last_object.map(ObjRef::Target)
    }

    fn checkpoint(&self) -> (usize, Option<u8>) {
        (self.specs.len(), self.last_object)
    }

    fn rollback(&mut self, mark: (usize, Option<u8>)) {
        self.specs.truncate(mark.0);
        self.last_object = mark.1;
    }
}

/// Ponto de entrada: uma frase normalizada vira efeito, ou nada.
pub fn parse_effect(text: &str) -> Option<Parsed> {
    let body = text.trim();
    let body = body.strip_suffix('.').unwrap_or(body).trim();
    if body.is_empty() {
        return None;
    }

    let mut ctx = Ctx::default();
    let mut effects: Vec<Effect> = Vec::new();
    for sentence in split_sentences(body) {
        // "Destroy target creature. It can't be regenerated." é uma frase só
        // partida em duas por convenção de template: a segunda metade não é um
        // efeito, é uma marca na primeira.
        if is_no_regeneration(&sentence) {
            match effects.last_mut() {
                Some(Effect::Destroy { no_regeneration, .. }) => *no_regeneration = true,
                _ => return None,
            }
            continue;
        }
        effects.push(parse_sentence(&sentence, &mut ctx)?);
    }

    // Mana no meio de uma sequência não é habilidade de mana (CR 605.1a): ela
    // passaria pela pilha e chegaria tarde demais para pagar a mágica que a
    // pediu. "{T}: Add {C}{C}. ~ deals 1 damage to you." precisa continuar
    // fora até o IR saber dizer "produz mana E tem custo colateral".
    if effects.len() > 1 && effects.iter().any(produces_mana) {
        return None;
    }

    let effect = match effects.len() {
        0 => return None,
        1 => effects.remove(0),
        _ => Effect::Sequence(effects),
    };
    Some(Parsed { effect, targets: ctx.specs })
}

fn split_sentences(body: &str) -> Vec<String> {
    body.split(". ")
        .map(|s| s.trim().trim_end_matches('.').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn produces_mana(effect: &Effect) -> bool {
    matches!(
        effect,
        Effect::AddMana { .. } | Effect::AddManaAnyColor { .. }
    )
}

fn is_no_regeneration(sentence: &str) -> bool {
    matches!(
        sentence,
        "it can't be regenerated" | "that creature can't be regenerated"
    )
}

// ---------------------------------------------------------------------------
// Uma frase
// ---------------------------------------------------------------------------

fn parse_sentence(sentence: &str, ctx: &mut Ctx) -> Option<Effect> {
    let s = sentence.trim();

    if let Some(rest) = s.strip_prefix("you may ") {
        let inner = attempt(ctx, rest, parse_sentence)?;
        return Some(Effect::May { do_: Box::new(inner), prompt: rest.to_string() });
    }
    // "Until end of turn, target creature gets +2/+2" é a mesma frase com a
    // duração escrita na frente.
    if let Some(rest) = s.strip_prefix("until end of turn, ") {
        let moved = format!("{rest} until end of turn");
        return attempt(ctx, &moved, parse_sentence);
    }

    attempt(ctx, s, parse_damage)
        .or_else(|| attempt(ctx, s, parse_player_sentence))
        .or_else(|| attempt(ctx, s, parse_object_sentence))
}

/// Roda um reconhecedor sem deixar alvo pela metade se ele falhar.
fn attempt(
    ctx: &mut Ctx,
    sentence: &str,
    recognizer: fn(&str, &mut Ctx) -> Option<Effect>,
) -> Option<Effect> {
    let mark = ctx.checkpoint();
    match recognizer(sentence, ctx) {
        Some(effect) => Some(effect),
        None => {
            ctx.rollback(mark);
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Dano
// ---------------------------------------------------------------------------

/// "~ deals 3 damage to any target".
///
/// O sujeito só pode ser a própria fonte: quem causa o dano no motor é sempre
/// a fonte do efeito, então aceitar "aquela criatura causa dano" daria a fonte
/// errada em vínculo com a vida e em gatilho de dano.
fn parse_damage(sentence: &str, ctx: &mut Ctx) -> Option<Effect> {
    let rest = damage_subject(sentence, ctx)?;
    let (amount, victim) = rest.split_once(" damage to ")?;
    let value = Value::Const(parse_count(amount)?);
    let victim = victim.trim();

    if victim == "any target" {
        return Some(Effect::DealDamage { amount: value, target: any_target(ctx)? });
    }
    if victim == "target creature or player" {
        return Some(Effect::DealDamage { amount: value, target: creature_or_player(ctx)? });
    }
    if let Some(player) = player_phrase(ctx, victim) {
        return Some(Effect::DealDamageToPlayer { amount: value, player });
    }
    let target = object_phrase(ctx, victim)?;
    Some(Effect::DealDamage { amount: value, target })
}

fn damage_subject<'a>(sentence: &'a str, ctx: &Ctx) -> Option<&'a str> {
    if let Some(rest) = sentence.strip_prefix("~ deals ") {
        return Some(rest);
    }
    // "When ~ enters, it deals 2 damage to any target": "it" é a fonte
    // enquanto nenhum alvo foi escolhido antes dela.
    if ctx.last_object.is_none() {
        return sentence.strip_prefix("it deals ");
    }
    None
}

// ---------------------------------------------------------------------------
// Frases de jogador
// ---------------------------------------------------------------------------

/// Sujeitos possíveis de uma frase de jogador, do mais longo para o mais curto
/// para que "each opponent" não seja lido como imperativo.
const PLAYER_SUBJECTS: [&str; 7] = [
    "each of your opponents ",
    "target player ",
    "target opponent ",
    "each opponent ",
    "each player ",
    "~'s controller ",
    "you ",
];

/// "target player discards two cards", "each opponent loses 3 life", "scry 2".
fn parse_player_sentence(sentence: &str, ctx: &mut Ctx) -> Option<Effect> {
    let mut found: Option<(PlayerRef, &str)> = None;
    for subject in PLAYER_SUBJECTS {
        if let Some(rest) = sentence.strip_prefix(subject) {
            let who = player_phrase(ctx, subject.trim())?;
            found = Some((who, rest.trim()));
            break;
        }
    }
    // Imperativo sem sujeito ("Draw a card.") fala com o controlador.
    let (player, rest) = found.unwrap_or((PlayerRef::You, sentence));

    for verb in ["draws ", "draw "] {
        if let Some(count) = rest.strip_prefix(verb) {
            return Some(Effect::DrawCards { count: card_count(count)?, player });
        }
    }
    for verb in ["gains ", "gain "] {
        if let Some(amount) = rest.strip_prefix(verb).and_then(|r| r.strip_suffix(" life")) {
            return Some(Effect::GainLife { amount: Value::Const(parse_count(amount)?), player });
        }
    }
    for verb in ["loses ", "lose "] {
        if let Some(amount) = rest.strip_prefix(verb).and_then(|r| r.strip_suffix(" life")) {
            return Some(Effect::LoseLife { amount: Value::Const(parse_count(amount)?), player });
        }
    }
    for verb in ["discards ", "discard "] {
        if let Some(what) = rest.strip_prefix(verb) {
            let (what, random) = match what.strip_suffix(" at random") {
                Some(head) => (head.trim(), true),
                None => (what, false),
            };
            return Some(Effect::Discard {
                count: card_count(what)?,
                player,
                filter: Filter::Any,
                random,
            });
        }
    }
    for verb in ["mills ", "mill "] {
        if let Some(count) = rest.strip_prefix(verb) {
            return Some(Effect::Mill { count: card_count(count)?, player });
        }
    }
    if let Some(count) = rest.strip_prefix("scry ") {
        return Some(Effect::Scry { count: Value::Const(parse_count(count)?), player });
    }
    if let Some(count) = rest.strip_prefix("surveil ") {
        return Some(Effect::Surveil { count: Value::Const(parse_count(count)?), player });
    }
    for verb in ["sacrifices ", "sacrifice "] {
        if let Some(what) = rest.strip_prefix(verb) {
            // "of their choice" é redundante no IR: quem sacrifica sempre
            // escolhe (CR 701.17a), e é o que o motor faz.
            let what = what.strip_suffix(" of their choice").unwrap_or(what);
            let (count, phrase) = split_count(what);
            let (filter, owner) = filter_phrase(phrase)?;
            // "sacrifica uma criatura que você controla" dito por outro
            // jogador é conjunto que o efeito não sabe representar.
            if owner.is_some() {
                return None;
            }
            return Some(Effect::Sacrifice {
                player,
                count: Value::Const(count),
                filter,
            });
        }
    }
    if rest == "shuffles their library" || rest == "shuffle your library" {
        return Some(Effect::ShuffleLibrary { player });
    }
    if rest == "wins the game" || rest == "win the game" {
        return Some(Effect::WinGame { player });
    }
    if rest == "loses the game" || rest == "lose the game" {
        return Some(Effect::LoseGame { player });
    }
    None
}

/// "a card" / "two cards" / "five cards".
fn card_count(text: &str) -> Option<Value> {
    let t = text.trim();
    let head = t.strip_suffix(" cards").or_else(|| t.strip_suffix(" card"))?;
    Some(Value::Const(parse_count(head)?))
}

// ---------------------------------------------------------------------------
// Frases de objeto
// ---------------------------------------------------------------------------

fn parse_object_sentence(sentence: &str, ctx: &mut Ctx) -> Option<Effect> {
    let s = sentence;

    if let Some(what) = s.strip_prefix("destroy ") {
        let target = object_phrase(ctx, what)?;
        return Some(Effect::Destroy { target, no_regeneration: false });
    }
    if let Some(what) = s.strip_prefix("exile ") {
        // "exile ... until ~ leaves the battlefield" devolve a carta depois, e
        // o motor não rastreia essa volta: compilar como exílio simples daria
        // uma carta permanentemente melhor que a impressa.
        let target = object_phrase(ctx, what)?;
        return Some(Effect::Exile { target, until_source_leaves: false });
    }
    if let Some(what) = s.strip_prefix("tap ") {
        return Some(Effect::Tap { target: object_phrase(ctx, what)? });
    }
    if let Some(what) = s.strip_prefix("untap ") {
        return Some(Effect::Untap { target: object_phrase(ctx, what)? });
    }
    if let Some(effect) = parse_return(s, ctx) {
        return Some(effect);
    }
    if let Some(effect) = parse_counterspell(s, ctx) {
        return Some(effect);
    }
    if let Some((a, b)) = s.split_once(" fights ") {
        let first = object_phrase(ctx, a)?;
        let second = object_phrase(ctx, b)?;
        return Some(Effect::Fight { a: first, b: second });
    }
    if let Some(what) = s.strip_suffix(" until end of turn") {
        if let Some(target) = what.strip_prefix("gain control of ") {
            let target = object_phrase(ctx, target)?;
            return Some(Effect::GainControl {
                target,
                player: PlayerRef::You,
                duration: Duration::EndOfTurn,
            });
        }
    }
    if let Some(effect) = parse_create_token(s) {
        return Some(effect);
    }
    if let Some(effect) = parse_counters(s, ctx) {
        return Some(effect);
    }
    if let Some(effect) = parse_continuous(s, ctx) {
        return Some(effect);
    }
    if let Some(subject) =
        s.strip_suffix(" doesn't untap during its controller's next untap step")
    {
        return Some(Effect::Freeze { target: object_phrase(ctx, subject)? });
    }
    if let Some(effect) = parse_search_library(s) {
        return Some(effect);
    }
    if let Some(effect) = parse_add_mana(s) {
        return Some(effect);
    }
    None
}

/// "return target creature to its owner's hand" e as duas formas de cemitério.
fn parse_return(sentence: &str, ctx: &mut Ctx) -> Option<Effect> {
    let rest = sentence.strip_prefix("return ")?;
    if let Some(what) = rest.strip_suffix(" to its owner's hand") {
        return Some(Effect::ReturnToHand { target: object_phrase(ctx, what)? });
    }
    if let Some(what) = rest.strip_suffix(" to the battlefield") {
        return Some(Effect::ReturnFromGraveyardToBattlefield {
            target: graveyard_phrase(ctx, what)?,
        });
    }
    if let Some(what) = rest.strip_suffix(" to your hand") {
        return Some(Effect::ReturnToHand { target: graveyard_phrase(ctx, what)? });
    }
    None
}

/// "counter target spell", "counter target creature spell unless its
/// controller pays {2}".
fn parse_counterspell(sentence: &str, ctx: &mut Ctx) -> Option<Effect> {
    let rest = sentence.strip_prefix("counter ")?;
    let (head, unless_pays) = match rest.split_once(" unless its controller pays ") {
        Some((head, cost)) => {
            let mana = parse_mana_cost(cost.trim())?;
            if mana.symbols.is_empty() {
                return None;
            }
            (head, Some(mtg_core::ir::Cost::Mana(mana.symbols)))
        }
        None => (rest, None),
    };
    let what = head.strip_prefix("target ")?.strip_suffix("spell")?.trim();
    let filter = match what {
        "" => Filter::Any,
        "creature" => Filter::HasType(CardType::Creature),
        "noncreature" => Filter::Not(Box::new(Filter::HasType(CardType::Creature))),
        "artifact" => Filter::HasType(CardType::Artifact),
        "instant or sorcery" => Filter::Or(vec![
            Filter::HasType(CardType::Instant),
            Filter::HasType(CardType::Sorcery),
        ]),
        _ => return None,
    };
    let description = if what.is_empty() {
        "target spell".to_string()
    } else {
        format!("target {what} spell")
    };
    let target = ctx.spell(filter, &description)?;
    Some(Effect::CounterSpell { target, unless_pays })
}

/// "put a +1/+1 counter on target creature", "remove a -1/-1 counter from ~".
fn parse_counters(sentence: &str, ctx: &mut Ctx) -> Option<Effect> {
    if let Some(rest) = sentence.strip_prefix("put ") {
        let (count_part, where_part) = split_counter_clause(rest, " on ")?;
        let (kind, count) = counter_amount(count_part)?;
        let target = object_phrase(ctx, where_part)?;
        return Some(Effect::AddCounters { target, kind, count });
    }
    if let Some(rest) = sentence.strip_prefix("remove ") {
        let (count_part, where_part) = split_counter_clause(rest, " from ")?;
        let (kind, count) = counter_amount(count_part)?;
        let target = object_phrase(ctx, where_part)?;
        return Some(Effect::RemoveCounters { target, kind, count });
    }
    None
}

fn split_counter_clause<'a>(rest: &'a str, joint: &str) -> Option<(&'a str, &'a str)> {
    for noun in [" counter", " counters"] {
        let needle = format!("{noun}{joint}");
        if let Some((head, tail)) = rest.split_once(&needle) {
            return Some((head, tail));
        }
    }
    None
}

/// "a +1/+1" -> (marcador, 1). Só os marcadores cujo efeito o motor conhece:
/// marcador nomeado ("oil", "stun") só faz sentido junto do texto que fala
/// dele, e esse texto não compila.
fn counter_amount(count_part: &str) -> Option<(CounterKind, Value)> {
    let (count_word, kind_word) = count_part.trim().rsplit_once(' ')?;
    let kind = match kind_word {
        "+1/+1" => CounterKind::PlusOnePlusOne,
        "-1/-1" => CounterKind::MinusOneMinusOne,
        "charge" => CounterKind::Charge,
        _ => return None,
    };
    Some((kind, Value::Const(parse_count(count_word)?)))
}

/// Modificação contínua até o fim do turno.
fn parse_continuous(sentence: &str, ctx: &mut Ctx) -> Option<Effect> {
    if let Some(subject) = sentence.strip_suffix(" can't be blocked this turn") {
        let target = object_phrase(ctx, subject)?;
        return Some(Effect::CantBeBlocked { target, duration: Duration::EndOfTurn });
    }
    if let Some(subject) = sentence.strip_suffix(" can't attack or block this turn") {
        let target = object_phrase(ctx, subject)?;
        return Some(Effect::CantAttackOrBlock { target, duration: Duration::EndOfTurn });
    }

    let rest = sentence.strip_suffix(" until end of turn")?;
    if let Some((subject, bonus)) = rest.split_once(" gets ") {
        let (bonus, granted) = match bonus.split_once(" and gains ") {
            Some((bonus, keywords)) => (bonus, Some(keywords)),
            None => (bonus, None),
        };
        let (p, t) = bonus.trim().split_once('/')?;
        let power = Value::Const(parse_signed(p)?);
        let toughness = Value::Const(parse_signed(t)?);
        let keywords = match granted {
            Some(list) => Some(granted_keywords(list)?),
            None => None,
        };
        let target = object_phrase(ctx, subject)?;
        let modify = Effect::ModifyPT {
            target: target.clone(),
            power,
            toughness,
            duration: Duration::EndOfTurn,
        };
        return Some(match keywords {
            Some(keywords) => Effect::Sequence(vec![
                modify,
                Effect::GrantKeywords { target, keywords, duration: Duration::EndOfTurn },
            ]),
            None => modify,
        });
    }
    if let Some((subject, granted)) = rest.split_once(" gains ") {
        let keywords = granted_keywords(granted)?;
        let target = object_phrase(ctx, subject)?;
        return Some(Effect::GrantKeywords { target, keywords, duration: Duration::EndOfTurn });
    }
    None
}

fn granted_keywords(list: &str) -> Option<Vec<Keyword>> {
    let flattened = list.replace(", and ", " and ").replace(", ", " and ");
    let mut out = Vec::new();
    for token in flattened.split(" and ") {
        out.push(parse_keyword(token.trim())?);
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// "search your library for a basic land card, reveal it, put it into your
/// hand, then shuffle".
///
/// O "reveal it" some no caminho: revelar não muda estado de jogo, e o IR não
/// tem como dizer "mostre esta carta". É a única liberdade tomada aqui, e ela
/// é de informação, não de efeito.
fn parse_search_library(sentence: &str) -> Option<Effect> {
    let rest = sentence.strip_prefix("search your library for ")?;
    let mut parts: Vec<&str> = rest.split(", ").map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    if parts.pop()? != "then shuffle" {
        return None;
    }
    let head = parts.remove(0);
    let to_hand = match parts.pop()? {
        "put it into your hand" | "put that card into your hand" => true,
        "put it onto the battlefield" | "put that card onto the battlefield" => false,
        _ => return None,
    };
    for middle in parts {
        if !matches!(middle, "reveal it" | "reveal that card") {
            return None;
        }
    }

    // Só a forma singular: "up to two ... cards" precisa de destino no plural
    // e de uma quantidade que o texto do destino tem que confirmar.
    let spec = head.strip_suffix(" card")?;
    let (count, phrase) = split_count(spec);
    if count != 1 {
        return None;
    }
    let (filter, owner) = filter_phrase(phrase)?;
    if owner.is_some() {
        return None;
    }
    Some(Effect::Sequence(vec![
        Effect::SearchLibrary {
            count: Value::Const(1),
            filter,
            player: PlayerRef::You,
            to_hand,
        },
        Effect::ShuffleLibrary { player: PlayerRef::You },
    ]))
}

/// "add {b}{b}{b}" e "add one mana of any color".
fn parse_add_mana(sentence: &str) -> Option<Effect> {
    let rest = sentence.strip_prefix("add ")?.trim();
    if rest == "one mana of any color" {
        return Some(Effect::AddManaAnyColor { count: Value::Const(1), player: PlayerRef::You });
    }
    let cost = parse_mana_cost(rest)?;
    if cost.symbols.is_empty() {
        return None;
    }
    Some(Effect::AddMana { symbols: cost.symbols, player: PlayerRef::You })
}
