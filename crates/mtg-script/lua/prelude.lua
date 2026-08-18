-- Prelúdio de autoria de cartas.
--
-- Objetivo: quem lê uma entrada entende a carta sem conhecer o IR. Cada helper
-- aqui só monta a tabela na forma que o serde de `mtg-core` desserializa —
-- a forma crua (variante externa: { DealDamage = { ... } }) fica escondida.

__cards = {}

-- ---------------------------------------------------------------------------
-- Valores e referências
-- ---------------------------------------------------------------------------

-- Número literal vira Value::Const; tabela passa direto (já é um Value).
function v(n)
  if type(n) == "table" then return n end
  return { Const = n }
end

X = "X"                                   -- Value::X
function count_of(selector) return { Count = selector } end
function power_of(ref) return { PowerOf = ref } end
function life_of(who) return { LifeOf = who } end
function cards_in_hand(who) return { CardsInHandOf = who } end
function add(a, b) return { Add = { v(a), v(b) } } end
function sub(a, b) return { Sub = { v(a), v(b) } } end

-- Alvos são 1-based em Lua e 0-based no motor: converte aqui, uma vez só.
function target(i) return { Target = (i or 1) - 1 } end
SELF = "SelfObject"
SELECTED = "Selected"
ATTACHED = "Attached"
TRIGGER_OBJECT = "TriggerObject"
TRIGGER_SOURCE = "TriggerSource"
function all(selector) return { All = selector } end

YOU = "You"
OPPONENTS = "Opponents"
EACH_PLAYER = "Each"
ACTIVE_PLAYER = "ActivePlayer"
function target_player(i) return { Target = (i or 1) - 1 } end

-- ---------------------------------------------------------------------------
-- Filtros e seletores
-- ---------------------------------------------------------------------------

ANY = "Any"
TAPPED = "Tapped"
UNTAPPED = "Untapped"
ATTACKING = "Attacking"
BLOCKING = "Blocking"
TOKEN = "Token"
NONTOKEN = "NonToken"
IS_SELF = "IsSelf"
IS_OTHER = "IsOther"

function has_type(t) return { HasType = t } end
function has_subtype(s) return { HasSubtype = s } end
function has_color(c) return { HasColor = c } end
function named(n) return { HasName = n } end
function has_keyword(k) return { HasKeyword = k } end
function power_at_least(n) return { PowerAtLeast = n } end
function power_at_most(n) return { PowerAtMost = n } end
function toughness_at_most(n) return { ToughnessAtMost = n } end
function mana_value_at_most(n) return { ManaValueAtMost = n } end
function controlled_by(who) return { ControlledBy = who } end
function f_and(...) return { And = { ... } } end
function f_or(...) return { Or = { ... } } end
function f_not(x) return { Not = x } end

CREATURE = { HasType = "Creature" }
ARTIFACT = { HasType = "Artifact" }
ENCHANTMENT = { HasType = "Enchantment" }
LAND = { HasType = "Land" }
PLANESWALKER = { HasType = "Planeswalker" }

-- sel{ zone = "Battlefield", filter = CREATURE, owner = OPPONENTS, max = 1 }
function sel(spec)
  return {
    zone = spec.zone or "Battlefield",
    filter = spec.filter or ANY,
    owner_scope = spec.owner,
    max = spec.max,
  }
end

function creatures(spec)
  spec = spec or {}
  return sel { zone = "Battlefield", filter = spec.filter or CREATURE, owner = spec.owner, max = spec.max }
end

function your_creatures() return creatures { owner = YOU } end
function enemy_creatures() return creatures { owner = OPPONENTS } end
function in_graveyard(filter) return sel { zone = "Graveyard", filter = filter or ANY } end

-- ---------------------------------------------------------------------------
-- Especificação de alvo
-- ---------------------------------------------------------------------------

function t_creature(desc, spec)
  return { kind = { Object = creatures(spec or {}) }, description = desc or "target creature" }
end

function t_permanent(desc, filter)
  return { kind = { Object = sel { filter = filter or ANY } }, description = desc or "target permanent" }
end

-- "any target": criatura, planeswalker ou jogador.
function t_any(desc)
  local objects = sel { filter = f_or(CREATURE, PLANESWALKER) }
  return { kind = { ObjectOrPlayer = { objects, "Each" } }, description = desc or "any target" }
end

function t_player(desc, who)
  return { kind = { Player = who or "Each" }, description = desc or "target player" }
end

function t_spell(desc, filter)
  return { kind = { SpellOnStack = filter or ANY }, description = desc or "target spell" }
end

-- ---------------------------------------------------------------------------
-- Efeitos
-- ---------------------------------------------------------------------------

function seq(list) return { Sequence = list } end
NOTHING = "Nothing"

function deal_damage(n, tgt) return { DealDamage = { amount = v(n), target = tgt or target(1) } } end
function damage_player(n, who) return { DealDamageToPlayer = { amount = v(n), player = who or OPPONENTS } } end
function gain_life(n, who) return { GainLife = { amount = v(n), player = who or YOU } } end
function lose_life(n, who) return { LoseLife = { amount = v(n), player = who or OPPONENTS } } end

function draw(n, who) return { DrawCards = { count = v(n or 1), player = who or YOU } } end
function mill(n, who) return { Mill = { count = v(n), player = who or OPPONENTS } } end
function scry(n, who) return { Scry = { count = v(n), player = who or YOU } } end
function surveil(n, who) return { Surveil = { count = v(n), player = who or YOU } } end

function discard(n, who, spec)
  spec = spec or {}
  return { Discard = { count = v(n), player = who or OPPONENTS, filter = spec.filter or ANY, random = spec.random or false } }
end

function search(spec)
  return { SearchLibrary = {
    count = v(spec.count or 1),
    filter = spec.filter or ANY,
    player = spec.player or YOU,
    to_hand = spec.to_hand ~= false,
  } }
end

function shuffle(who) return { ShuffleLibrary = { player = who or YOU } } end
function bounce(tgt) return { ReturnToHand = { target = tgt or target(1) } } end
function reanimate(tgt) return { ReturnFromGraveyardToBattlefield = { target = tgt or target(1) } } end

function destroy(tgt, no_regen)
  return { Destroy = { target = tgt or target(1), no_regeneration = no_regen or false } }
end

function exile(tgt, until_leaves)
  return { Exile = { target = tgt or target(1), until_source_leaves = until_leaves or false } }
end

function sacrifice(n, who, filter)
  return { Sacrifice = { player = who or YOU, count = v(n or 1), filter = filter or ANY } }
end

function tap(tgt) return { Tap = { target = tgt or target(1) } } end
function untap(tgt) return { Untap = { target = tgt or target(1) } } end
function freeze(tgt) return { Freeze = { target = tgt or target(1) } } end
function fight(a, b) return { Fight = { a = a or SELF, b = b or target(1) } } end

function gain_control(tgt, who, dur)
  return { GainControl = { target = tgt or target(1), player = who or YOU, duration = dur or "Permanent" } }
end

function attach(tgt) return { AttachTo = { equipment = SELF, target = tgt or target(1) } } end

-- pump(2, 2)                    -> +2/+2 até o fim do turno na fonte
-- pump(1, 1, { target = target(1), keywords = { "Trample" } })
function pump(p, t, opts)
  opts = opts or {}
  local dur = opts.duration or "EndOfTurn"
  local tgt = opts.target or target(1)
  local effects = { { ModifyPT = { target = tgt, power = v(p), toughness = v(t), duration = dur } } }
  if opts.keywords then
    effects[#effects + 1] = { GrantKeywords = { target = tgt, keywords = opts.keywords, duration = dur } }
  end
  if #effects == 1 then return effects[1] end
  return seq(effects)
end

function set_pt(p, t, opts)
  opts = opts or {}
  return { SetPT = { target = opts.target or target(1), power = v(p), toughness = v(t), duration = opts.duration or "EndOfTurn" } }
end

function grant(keywords, opts)
  opts = opts or {}
  return { GrantKeywords = { target = opts.target or target(1), keywords = keywords, duration = opts.duration or "EndOfTurn" } }
end

function add_counters(n, kind, tgt)
  return { AddCounters = { target = tgt or SELF, kind = kind or "PlusOnePlusOne", count = v(n) } }
end

function remove_counters(n, kind, tgt)
  return { RemoveCounters = { target = tgt or SELF, kind = kind or "PlusOnePlusOne", count = v(n) } }
end

function counter_spell(tgt, unless)
  return { CounterSpell = { target = tgt or target(1), unless_pays = unless } }
end

function cant_be_blocked(tgt, dur)
  return { CantBeBlocked = { target = tgt or target(1), duration = dur or "EndOfTurn" } }
end

-- token{ name = "Soldier", type = "Creature — Soldier", pt = {1,1}, colors = {"White"}, count = 2 }
function token(spec)
  local pt = spec.pt or { 1, 1 }
  return { CreateToken = {
    spec = {
      name = spec.name,
      type_line = typeline(spec.type),
      colors = spec.colors or {},
      power = pt[1],
      toughness = pt[2],
      keywords = spec.keywords or {},
      art_key = spec.art or spec.name,
    },
    count = v(spec.count or 1),
    controller = spec.controller or YOU,
  } }
end

function add_mana(spec, who) return { AddMana = { symbols = mana(spec), player = who or YOU } } end
function any_color_mana(n, who) return { AddManaAnyColor = { count = v(n or 1), player = who or YOU } } end

function may(effect, prompt) return { May = { do_ = effect, prompt = prompt or "" } } end
function when(condition, then_do, else_do)
  return { Conditional = { cond = condition, then_do = then_do, else_do = else_do } }
end
function for_each(selector, effect) return { ForEach = { over = selector, do_ = effect } } end
function repeat_times(n, effect) return { Repeat = { times = v(n), do_ = effect } } end

-- modal{ "Draw a card", draw(1), "Deal 2 damage", deal_damage(2) }
function modal(spec)
  local options = {}
  for i = 1, #spec, 2 do
    options[#options + 1] = { spec[i], spec[i + 1] }
  end
  return { Modal = { choose = spec.choose or 1, options = options } }
end

-- ---------------------------------------------------------------------------
-- Condições
-- ---------------------------------------------------------------------------

ALWAYS = "Always"
YOUR_TURN = "IsYourTurn"
function exists(selector) return { Exists = selector } end
function you_control_at_least(n, filter) return { YouControlAtLeast = { n, filter } } end
function c_and(...) return { And = { ... } } end
function c_or(...) return { Or = { ... } } end
function c_not(x) return { Not = x } end
function compare(a, op, b) return { Compare = { v(a), op, v(b) } } end

-- ---------------------------------------------------------------------------
-- Habilidades
-- ---------------------------------------------------------------------------

function kw(k) return { Keyword = k } end

local function trigger_ability(condition, effect, opts)
  opts = opts or {}
  return { Triggered = {
    trigger = condition,
    intervening_if = opts.if_ or ALWAYS,
    targets = opts.targets or {},
    effect = effect,
    optional = opts.optional or false,
    once_per_turn = opts.once_per_turn or false,
    triggers_from_graveyard = opts.from_graveyard or false,
    text = opts.text or "",
  } }
end

-- "Quando isto entra no campo de batalha, ..."
function etb(effect, opts)
  return trigger_ability({ EntersBattlefield = sel { filter = IS_SELF } }, effect, opts)
end

-- "Quando isto morre, ..."
function dies(effect, opts)
  opts = opts or {}
  opts.from_graveyard = true
  return trigger_ability({ Dies = sel { filter = IS_SELF } }, effect, opts)
end

function when_attacks(effect, opts)
  return trigger_ability({ Attacks = sel { filter = IS_SELF } }, effect, opts)
end

function when_other_dies(filter, effect, opts)
  return trigger_ability({ Dies = sel { filter = f_and(filter or CREATURE, IS_OTHER) } }, effect, opts)
end

function at_upkeep(effect, opts)
  return trigger_ability({ BeginningOfUpkeep = YOU }, effect, opts)
end

function at_end_step(effect, opts)
  return trigger_ability({ BeginningOfEndStep = YOU }, effect, opts)
end

function on_cast(filter, effect, opts)
  return trigger_ability({ SpellCast = sel { zone = "Stack", filter = filter or ANY } }, effect, opts)
end

-- activated{ cost = "{2}{R}", tap = true, effect = deal_damage(1), targets = { t_any() } }
function activated(spec)
  local costs = {}
  if spec.cost then costs[#costs + 1] = { Mana = mana(spec.cost) } end
  if spec.tap then costs[#costs + 1] = "Tap" end
  if spec.pay_life then costs[#costs + 1] = { PayLife = v(spec.pay_life) } end
  if spec.sacrifice then costs[#costs + 1] = { Sacrifice = { 1, spec.sacrifice } } end
  if spec.discard then costs[#costs + 1] = { Discard = { 1, ANY } } end
  local cost
  if #costs == 0 then cost = "Free"
  elseif #costs == 1 then cost = costs[1]
  else cost = { Composite = costs } end

  return { Activated = {
    cost = cost,
    targets = spec.targets or {},
    effect = spec.effect,
    timing = spec.sorcery_speed and "Sorcery" or "Instant",
    restriction = spec.restriction or ALWAYS,
    uses_per_turn = spec.uses_per_turn,
    loyalty_change = spec.loyalty,
    text = spec.text or "",
  } }
end

-- mana_ability{ produces = "{G}" }  ou  mana_ability{ produces = "any", count = 1 }
function mana_ability(spec)
  local costs = {}
  if spec.cost then costs[#costs + 1] = { Mana = mana(spec.cost) } end
  costs[#costs + 1] = spec.no_tap and nil or "Tap"
  local cost
  if #costs == 1 then cost = costs[1] else cost = { Composite = costs } end

  local production
  if spec.produces == "any" then
    production = { AnyColor = spec.count or 1 }
  elseif type(spec.produces) == "table" then
    production = { OneOf = spec.produces }
  else
    production = { Fixed = mana(spec.produces) }
  end

  return { Mana = {
    cost = cost,
    production = production,
    restriction = spec.restriction or ALWAYS,
    text = spec.text or "",
  } }
end

-- static{ affects = your_creatures(), mod = { ModifyPT = { 1, 1 } } }
-- Atalho: static_pt(1, 1, your_creatures())
function static_ability(spec)
  return { Static = {
    condition = spec.condition or ALWAYS,
    affects = spec.affects,
    modification = spec.mod,
    text = spec.text or "",
  } }
end

function static_pt(p, t, affects, text)
  return static_ability {
    affects = affects or your_creatures(),
    mod = { ModifyPT = { v(p), v(t) } },
    text = text,
  }
end

function static_grant(keywords, affects, text)
  return static_ability {
    affects = affects or your_creatures(),
    mod = { GrantKeywords = keywords },
    text = text,
  }
end

function enters_tapped()
  return { Replacement = { event = "EntersTapped", replacement = NOTHING, text = "enters tapped" } }
end

function enters_with_counters(n, kind)
  return { Replacement = {
    event = { EntersWithCounters = { kind or "PlusOnePlusOne", v(n) } },
    replacement = NOTHING,
    text = "",
  } }
end

-- ---------------------------------------------------------------------------
-- Declaração da carta
-- ---------------------------------------------------------------------------

-- card {
--   name = "Lightning Bolt", cost = "{R}", type = "Instant",
--   rarity = "Common", set = "LEA",
--   text = "Lightning Bolt deals 3 damage to any target.",
--   targets = { t_any() },
--   effect = deal_damage(3),
-- }
function card(spec)
  assert(spec.name, "carta sem nome")
  assert(spec.type, "carta " .. tostring(spec.name) .. " sem linha de tipo")

  local abilities = {}
  for _, k in ipairs(spec.keywords or {}) do
    abilities[#abilities + 1] = kw(k)
  end
  for _, a in ipairs(spec.abilities or {}) do
    abilities[#abilities + 1] = a
  end

  local pt = spec.pt
  local def = {
    id = 0,                                  -- reindexado no lado Rust
    name = spec.name,
    mana_cost = mana(spec.cost or ""),
    type_line = typeline(spec.type),
    color_override = spec.colors,
    power = pt and pt[1] or spec.power,
    toughness = pt and pt[2] or spec.toughness,
    loyalty = spec.loyalty,
    abilities = abilities,
    spell_effect = spec.effect,
    spell_targets = spec.targets or {},
    oracle_text = spec.text or "",
    flavor_text = spec.flavor,
    rarity = spec.rarity or "Common",
    set_code = spec.set or "CORE",
    collector_number = spec.number or "",
    artist = spec.artist,
    art_key = spec.art or spec.name,         -- a UI busca a arte no Scryfall por este nome
  }

  __cards[#__cards + 1] = def
  return def
end

-- Terreno básico: mesmo formato cinco vezes, então vira helper.
function basic_land(name, subtype, symbol)
  return card {
    name = name,
    type = "Basic Land — " .. subtype,
    rarity = "Common",
    set = "CORE",
    text = "({T}: Add " .. symbol .. ".)",
    abilities = { mana_ability { produces = symbol } },
  }
end
