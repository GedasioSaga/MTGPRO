//! Leitura de carta para fins de heurística: que papel ela cumpre e quanto
//! vale. Aqui — e só aqui — a IA lê `CardDef`, porque o texto do efeito (a
//! árvore de IR) não existe em `Characteristics`. Nenhuma decisão de regra sai
//! deste módulo: P/T, tipos e palavras-chave continuam vindo das camadas.
use mtg_core::card::CardDef;
use mtg_core::engine::Game;
use mtg_core::ids::{ObjectId, PlayerId};
use mtg_core::ir::{Effect, Keyword, TargetKind, Value};
use mtg_core::types::CardType;

use crate::eval::{CreatureInfo, Snapshot, Traits};

/// Papel de uma mágica ou habilidade na estratégia do bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellRole {
    Land,
    Creature,
    /// Mata ou neutraliza um permanente adversário.
    Removal,
    /// Dano que pode ir à cara.
    Burn,
    Counter,
    /// Truque de combate: muda P/T ou concede palavra-chave.
    Pump,
    Draw,
    Ramp,
    Other,
}

impl SpellRole {
    /// Papel que só faz sentido em resposta a algo do oponente. Guardar mana
    /// para estes é a diferença entre um bot que blefa e um que passa turno.
    pub fn is_reactive(self) -> bool {
        matches!(self, SpellRole::Counter | SpellRole::Pump)
    }
}

pub fn card_def(game: &Game, object: ObjectId) -> Option<&CardDef> {
    let obj = game.state.object(object)?;
    game.db.get(obj.card)
}

/// Percorre a árvore de efeito procurando um padrão.
fn effect_has(effect: &Effect, pred: impl Fn(&Effect) -> bool) -> bool {
    let mut found = false;
    effect.walk(&mut |e| {
        if pred(e) {
            found = true;
        }
    });
    found
}

pub fn classify(def: &CardDef) -> SpellRole {
    if def.type_line.is_land() {
        return SpellRole::Land;
    }
    if def.type_line.is_creature() {
        return SpellRole::Creature;
    }
    let Some(effect) = &def.spell_effect else {
        return SpellRole::Other;
    };
    classify_effect(effect)
}

/// Classifica pelo efeito. Serve para mágicas e para habilidades ativadas.
pub fn classify_effect(effect: &Effect) -> SpellRole {
    if effect_has(effect, |e| matches!(e, Effect::CounterSpell { .. })) {
        return SpellRole::Counter;
    }
    if effect_has(effect, |e| {
        matches!(
            e,
            Effect::Destroy { .. } | Effect::Exile { .. } | Effect::Fight { .. }
        )
    }) {
        return SpellRole::Removal;
    }
    if effect_has(effect, |e| {
        matches!(
            e,
            Effect::DealDamage { .. }
                | Effect::DealDamageToPlayer { .. }
                | Effect::DivideDamage { .. }
        )
    }) {
        return SpellRole::Burn;
    }
    if effect_has(effect, |e| {
        matches!(
            e,
            Effect::ModifyPT { .. } | Effect::SetPT { .. } | Effect::GrantKeywords { .. }
        )
    }) {
        return SpellRole::Pump;
    }
    if effect_has(effect, |e| matches!(e, Effect::DrawCards { .. })) {
        return SpellRole::Draw;
    }
    if effect_has(effect, |e| {
        matches!(
            e,
            Effect::AddMana { .. } | Effect::AddManaAnyColor { .. }
        )
    }) {
        return SpellRole::Ramp;
    }
    SpellRole::Other
}

/// Pode ser lançada com a pilha do oponente aberta (CR 601.3a, 702.8b).
pub fn is_instant_speed(def: &CardDef) -> bool {
    def.type_line.has_type(CardType::Instant) || def.has_keyword(&Keyword::Flash)
}

/// Dano fixo que o efeito causa. Só constantes: valor que depende do estado
/// não dá para prever aqui sem o motor, e chutar seria pior que zero.
pub fn fixed_damage(effect: &Effect, x: u32) -> i32 {
    let mut total = 0;
    effect.walk(&mut |e| {
        let amount = match e {
            Effect::DealDamage { amount, .. }
            | Effect::DealDamageToPlayer { amount, .. }
            | Effect::DivideDamage { total: amount, .. }
            | Effect::LoseLife { amount, .. } => Some(amount),
            _ => None,
        };
        if let Some(v) = amount {
            total += match v {
                Value::Const(n) => *n,
                Value::X => x as i32,
                _ => 0,
            };
        }
    });
    total
}

/// A mágica consegue mirar um jogador? Determina se serve de dano na cara.
pub fn can_target_player(def: &CardDef) -> bool {
    def.spell_targets.iter().any(|t| {
        matches!(
            t.kind,
            TargetKind::Player(_) | TargetKind::ObjectOrPlayer(_, _)
        )
    })
}

/// Valor do corpo que entraria em campo se esta criatura fosse lançada.
pub fn body_value(def: &CardDef, controller: PlayerId) -> i64 {
    let keywords: Vec<Keyword> = def.keywords().cloned().collect();
    let mut c = CreatureInfo::vanilla(
        ObjectId::NONE,
        controller,
        def.power.unwrap_or(0),
        def.toughness.unwrap_or(0),
    );
    c.mana_value = def.mana_value();
    c.traits = Traits::from_keywords(&keywords);
    c.summoning_sick = true;
    c.value()
}

/// Quanto vale este objeto para mim agora — em campo, na mão ou no cemitério.
pub fn object_value(game: &Game, s: &Snapshot, id: ObjectId) -> i64 {
    if let Some(c) = s.find(id) {
        return c.value();
    }
    let Some(def) = card_def(game, id) else {
        return 100;
    };
    match classify(def) {
        SpellRole::Land => 150,
        SpellRole::Creature => body_value(def, s.me),
        SpellRole::Removal | SpellRole::Counter => 220 + def.mana_value() as i64 * 30,
        SpellRole::Burn => 200 + def.mana_value() as i64 * 30,
        SpellRole::Draw => 210,
        SpellRole::Pump => 120,
        SpellRole::Ramp => 140,
        SpellRole::Other => 130 + def.mana_value() as i64 * 20,
    }
}

/// Quão bem-vinda esta carta é no topo da biblioteca. Negativa quando a carta
/// é justamente o que não falta: terreno com sete no campo, bomba de 7 manas
/// no turno 2. É isso que faz vidência valer alguma coisa.
pub fn draw_desirability(game: &Game, s: &Snapshot, id: ObjectId) -> i64 {
    let Some(def) = card_def(game, id) else {
        return 0;
    };
    let lands = s.my_lands as i64;
    if def.type_line.is_land() {
        return match lands {
            0..=3 => 150,
            4..=5 => 30,
            _ => -100,
        };
    }
    let mv = def.mana_value() as i64;
    let castable_soon = mv <= lands + 1;
    let base = object_value(game, s, id) / 3;
    if castable_soon {
        base + 60
    } else if mv > lands + 3 {
        base - 140
    } else {
        base - 20
    }
}

/// Existe alvo adversário que justifique gastar uma remoção deste custo?
/// Usado para não queimar Doom Blade em Elfo e para decidir dano na cara.
pub fn has_worthy_creature_target(s: &Snapshot, threshold: i64) -> bool {
    s.opp_creatures
        .iter()
        .any(|c| !c.traits.untargetable_by_opponent() && c.value() >= threshold)
}

/// Ameaça representada por uma criatura adversária: o valor dela mais o quanto
/// ela está machucando agora. Criatura atacando vale mais morta.
pub fn threat_value(s: &Snapshot, c: &CreatureInfo) -> i64 {
    let mut v = c.value();
    if c.attacking {
        v += c.power.max(0) as i64 * 20;
    }
    if c.power.max(0) >= s.my_life {
        // Sozinha ela já é metade do relógio: prioridade máxima.
        v += 400;
    }
    v
}

/// Palavras que denunciam um custo (o bot escolhe o pior) em vez de um
/// benefício (escolhe o melhor). Prompts do motor são em português.
const COST_MARKERS: [&str; 10] = [
    "sacrifi", "descart", "discard", "pague", "paga ", "perde", "exile um", "moa", "mill", "remova",
];

pub fn prompt_is_cost(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    COST_MARKERS.iter().any(|m| lower.contains(m))
}

/// Prompt de gatilho opcional com desvantagem embutida ("você pode sacrificar").
pub fn prompt_has_downside(prompt: &str) -> bool {
    prompt_is_cost(prompt)
}

/// Peso grosseiro de um modo pelo texto, quando o motor só oferece rótulos.
pub fn text_value(text: &str) -> i64 {
    let t = text.to_lowercase();
    let mut v = 100;
    for (marker, weight) in [
        ("destr", 300),
        ("exil", 280),
        ("anul", 260),
        ("counter", 260),
        ("dano", 240),
        ("damage", 240),
        ("compre", 220),
        ("draw", 220),
        ("ficha", 200),
        ("token", 200),
        ("devolv", 170),
        ("return", 170),
        ("descart", 150),
        ("vida", 70),
        ("life", 70),
    ] {
        if t.contains(marker) {
            v = v.max(weight);
        }
    }
    v
}
