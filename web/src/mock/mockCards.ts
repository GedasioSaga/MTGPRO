/**
 * Catalogo de demonstracao: um subconjunto real do catalogo Lua (ver
 * `cards/*.lua`), o suficiente para montar `demoMatch.ts` sem servidor.
 *
 * Cada carta vive uma vez como `MockCardTemplate` — fatos que nao mudam em
 * partida — e duas projecoes derivam dele: `toCardDef` (forma do catalogo,
 * `GET /api/cards`) e `toCardView` (instancia numa partida, com id e zona
 * proprios). Uma fonte, duas formas: o nome nunca diverge do custo.
 */
import type {
  CardDef,
  CardType,
  CardView,
  ColorSet,
  ObjectId,
  PlayerId,
  Rarity,
  Supertype,
  ZoneKind,
} from '../types/protocol'
import type { DeckInfo } from '../net/api'

// ---------------------------------------------------------------------------
// Modelo
// ---------------------------------------------------------------------------

export interface MockCardTemplate {
  key: string
  name: string
  /** `null` para terrenos, que nao tem custo de mana. */
  manaCostText: string | null
  manaValue: number
  typeLineText: string
  supertypes: Supertype[]
  types: CardType[]
  subtypes: string[]
  colors: ColorSet
  power: number | null
  toughness: number | null
  oracleText: string
  keywords: string[]
  rarity: Rarity
  setCode: string
}

/** Bits de `ColorSet` no motor (`mana.rs`): W=1, U=2, B=4, R=8, G=16. */
const WHITE: ColorSet = 1
const GREEN: ColorSet = 16
const RED: ColorSet = 8
const COLORLESS: ColorSet = 0

function land(name: string, subtype: string): MockCardTemplate {
  return {
    key: name.toLowerCase(),
    name,
    manaCostText: null,
    manaValue: 0,
    typeLineText: 'Land',
    supertypes: ['Basic'],
    types: ['Land'],
    subtypes: [subtype],
    colors: COLORLESS,
    power: null,
    toughness: null,
    oracleText: `{T}: Add ${subtype === 'Mountain' ? '{R}' : subtype === 'Forest' ? '{G}' : '{W}'}.`,
    keywords: [],
    rarity: 'Common',
    setCode: 'basic',
  }
}

function creature(opts: {
  key: string
  name: string
  cost: string
  subtypes: string[]
  colors: ColorSet
  pt: [number, number]
  text: string
  keywords?: string[]
  rarity: Rarity
  set: string
}): MockCardTemplate {
  return {
    key: opts.key,
    name: opts.name,
    manaCostText: opts.cost,
    manaValue: manaValueOf(opts.cost),
    typeLineText: `Creature — ${opts.subtypes.join(' ')}`,
    supertypes: [],
    types: ['Creature'],
    subtypes: opts.subtypes,
    colors: opts.colors,
    power: opts.pt[0],
    toughness: opts.pt[1],
    oracleText: opts.text,
    keywords: opts.keywords ?? [],
    rarity: opts.rarity,
    setCode: opts.set,
  }
}

function instant(opts: {
  key: string
  name: string
  cost: string
  colors: ColorSet
  text: string
  rarity: Rarity
  set: string
}): MockCardTemplate {
  return {
    key: opts.key,
    name: opts.name,
    manaCostText: opts.cost,
    manaValue: manaValueOf(opts.cost),
    typeLineText: 'Instant',
    supertypes: [],
    types: ['Instant'],
    subtypes: [],
    colors: opts.colors,
    power: null,
    toughness: null,
    oracleText: opts.text,
    keywords: [],
    rarity: opts.rarity,
    setCode: opts.set,
  }
}

/** Soma o generico e os simbolos coloridos de um custo `"{1}{R}{R}"`. */
function manaValueOf(cost: string): number {
  const symbols = cost.match(/\{([^}]*)\}/g) ?? []
  return symbols.reduce((total, raw) => {
    const body = raw.slice(1, -1)
    return total + (/^\d+$/.test(body) ? Number(body) : 1)
  }, 0)
}

// ---------------------------------------------------------------------------
// Catalogo — dois arquetipos reais de `crates/mtg-cards/src/decks.rs`
// ---------------------------------------------------------------------------

export const MOCK_CARDS: Readonly<Record<string, MockCardTemplate>> = {
  mountain: land('Mountain', 'Mountain'),
  forest: land('Forest', 'Forest'),
  plains: land('Plains', 'Plains'),

  ragingGoblin: creature({
    key: 'ragingGoblin',
    name: 'Raging Goblin',
    cost: '{R}',
    subtypes: ['Goblin', 'Berserker'],
    colors: RED,
    pt: [1, 1],
    text: 'Haste',
    keywords: ['Haste'],
    rarity: 'Common',
    set: 'USG',
  }),
  moggFanatic: creature({
    key: 'moggFanatic',
    name: 'Mogg Fanatic',
    cost: '{R}',
    subtypes: ['Goblin'],
    colors: RED,
    pt: [1, 1],
    text: 'Sacrifice Mogg Fanatic: Mogg Fanatic deals 1 damage to any target.',
    rarity: 'Uncommon',
    set: 'TMP',
  }),
  goblinPiker: creature({
    key: 'goblinPiker',
    name: 'Goblin Piker',
    cost: '{1}{R}',
    subtypes: ['Goblin', 'Warrior'],
    colors: RED,
    pt: [2, 1],
    text: '',
    rarity: 'Common',
    set: 'M10',
  }),
  emberHauler: creature({
    key: 'emberHauler',
    name: 'Ember Hauler',
    cost: '{1}{R}',
    subtypes: ['Goblin'],
    colors: RED,
    pt: [2, 2],
    text: '{1}, Sacrifice Ember Hauler: Ember Hauler deals 2 damage to any target.',
    rarity: 'Rare',
    set: 'M11',
  }),
  goblinChieftain: creature({
    key: 'goblinChieftain',
    name: 'Goblin Chieftain',
    cost: '{1}{R}{R}',
    subtypes: ['Goblin'],
    colors: RED,
    pt: [2, 2],
    text: 'Haste\nOther Goblin creatures you control get +1/+1 and have haste.',
    keywords: ['Haste'],
    rarity: 'Rare',
    set: 'M10',
  }),
  lightningBolt: instant({
    key: 'lightningBolt',
    name: 'Lightning Bolt',
    cost: '{R}',
    colors: RED,
    text: 'Lightning Bolt deals 3 damage to any target.',
    rarity: 'Common',
    set: 'LEA',
  }),

  elvishVisionary: creature({
    key: 'elvishVisionary',
    name: 'Elvish Visionary',
    cost: '{1}{G}',
    subtypes: ['Elf', 'Shaman'],
    colors: GREEN,
    pt: [1, 1],
    text: 'When Elvish Visionary enters the battlefield, draw a card.',
    rarity: 'Common',
    set: 'M13',
  }),
  wallOfBlossoms: creature({
    key: 'wallOfBlossoms',
    name: 'Wall of Blossoms',
    cost: '{1}{G}',
    subtypes: ['Plant', 'Wall'],
    colors: GREEN,
    pt: [0, 4],
    text: 'Defender\nWhen Wall of Blossoms enters the battlefield, draw a card.',
    keywords: ['Defender'],
    rarity: 'Uncommon',
    set: 'STH',
  }),
  centaurCourser: creature({
    key: 'centaurCourser',
    name: 'Centaur Courser',
    cost: '{2}{G}',
    subtypes: ['Centaur', 'Warrior'],
    colors: GREEN,
    pt: [3, 3],
    text: '',
    rarity: 'Common',
    set: 'M10',
  }),
  veteranArmorer: creature({
    key: 'veteranArmorer',
    name: 'Veteran Armorer',
    cost: '{1}{W}',
    subtypes: ['Human', 'Soldier'],
    colors: WHITE,
    pt: [2, 2],
    text: 'Other creatures you control get +0/+1.',
    rarity: 'Common',
    set: 'M10',
  }),
  divineVerdict: instant({
    key: 'divineVerdict',
    name: 'Divine Verdict',
    cost: '{3}{W}',
    colors: WHITE,
    text: 'Destroy target attacking or blocking creature.',
    rarity: 'Common',
    set: 'M13',
  }),
}

// ---------------------------------------------------------------------------
// Projecoes
// ---------------------------------------------------------------------------

let mockCardDefId = 900000

/** Forma do catalogo (`CardDef`), para MatchSetup pre-visualizar sem servidor. */
export function toCardDef(template: MockCardTemplate): CardDef {
  mockCardDefId += 1
  return {
    id: mockCardDefId,
    name: template.name,
    manaCost: [],
    typeLine: {
      supertypes: template.supertypes,
      types: template.types,
      subtypes: template.subtypes,
    },
    colorOverride: null,
    power: template.power,
    toughness: template.toughness,
    loyalty: null,
    oracleText: template.oracleText,
    flavorText: null,
    rarity: template.rarity,
    setCode: template.setCode,
    collectorNumber: '',
    artist: null,
    artKey: template.name,
  }
}

export interface CardViewOverrides {
  zone: ZoneKind
  controller: PlayerId
  owner: PlayerId
  tapped?: boolean
  summoningSick?: boolean
  power?: number
  toughness?: number
  keywords?: string[]
}

/** Instancia uma carta numa partida: mesmos fatos, id e zona proprios. */
export function toCardView(
  template: MockCardTemplate,
  id: ObjectId,
  overrides: CardViewOverrides,
): CardView {
  return {
    id,
    name: template.name,
    manaCost: template.manaCostText,
    manaValue: template.manaValue,
    typeLine: template.typeLineText,
    oracleText: template.oracleText,
    flavorText: null,
    colors: template.colors,
    power: overrides.power ?? template.power,
    toughness: overrides.toughness ?? template.toughness,
    basePower: template.power,
    baseToughness: template.toughness,
    loyalty: null,
    damage: 0,
    tapped: overrides.tapped ?? false,
    faceDown: false,
    summoningSick: overrides.summoningSick ?? false,
    attacking: null,
    blocking: [],
    blockedBy: [],
    counters: [],
    keywords: overrides.keywords ?? template.keywords,
    attachedTo: null,
    attachments: [],
    isToken: false,
    controller: overrides.controller,
    owner: overrides.owner,
    zone: overrides.zone,
    artKey: template.name,
    rarity: template.rarity,
    setCode: template.setCode,
    isLegalTarget: false,
    isActionable: false,
  }
}

// ---------------------------------------------------------------------------
// Decks — fallback de `GET /api/decks` (ver `crates/mtg-cards/src/decks.rs`)
// ---------------------------------------------------------------------------

export interface MockDeckInfo extends DeckInfo {
  description: string
  /** Letras WUBRG na ordem de exibicao do deck, para o chip de cor. */
  colors: string
}

export const MOCK_DECKS: readonly MockDeckInfo[] = [
  {
    id: 'Goblin Onslaught',
    name: 'Goblin Onslaught',
    description: 'Agressivo mono-vermelho: pressa e dano direto para fechar antes do turno seis.',
    colors: 'R',
  },
  {
    id: 'Azorius Control',
    name: 'Azorius Control',
    description: 'Controle azul-branco: contramagia, remocao incondicional e compra.',
    colors: 'WU',
  },
  {
    id: 'Selesnya Valor',
    name: 'Selesnya Valor',
    description: 'Meio-de-curva verde-branco: cada criatura traz valor ao entrar.',
    colors: 'WG',
  },
  {
    id: 'Gruul Stampede',
    name: 'Gruul Stampede',
    description: 'Rampa verde-vermelha: elfos no turno um pagam gigantes no turno quatro.',
    colors: 'RG',
  },
]
