/**
 * Camada pura da carta: decide moldura, cores, arte procedural e o modelo de
 * exibição de P/T a partir de um `CardView`. Sem JSX e sem DOM — assim o visual
 * é testável e os componentes ficam só com a marcação.
 */
import type { CSSProperties } from 'react'
import type { CardView, Defender } from '../../types/protocol'

// ---------------------------------------------------------------------------
// Cores
// ---------------------------------------------------------------------------

export type ManaColor = 'W' | 'U' | 'B' | 'R' | 'G'

/** Bits de `ColorSet` no motor: White=0, Blue=1, Black=2, Red=3, Green=4. */
const COLOR_BITS: readonly (readonly [ManaColor, number])[] = [
  ['W', 1],
  ['U', 2],
  ['B', 4],
  ['R', 8],
  ['G', 16],
]

export function colorsFromMask(mask: number): ManaColor[] {
  return COLOR_BITS.filter(([, bit]) => (mask & bit) !== 0).map(([color]) => color)
}

// ---------------------------------------------------------------------------
// Utilidades de cor
// ---------------------------------------------------------------------------

function parseHex(hex: string): [number, number, number] {
  const body = hex.replace('#', '')
  const full =
    body.length === 3 ? body[0] + body[0] + body[1] + body[1] + body[2] + body[2] : body
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ]
}

function toHex(channels: [number, number, number]): string {
  return (
    '#' +
    channels
      .map((value) =>
        Math.max(0, Math.min(255, Math.round(value))).toString(16).padStart(2, '0'),
      )
      .join('')
  )
}

/** Mistura linear: `t = 0` devolve `a`, `t = 1` devolve `b`. */
export function mixHex(a: string, b: string, t: number): string {
  const [ar, ag, ab] = parseHex(a)
  const [br, bg, bb] = parseHex(b)
  return toHex([ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t])
}

export function darken(hex: string, amount: number): string {
  return mixHex(hex, '#000000', amount)
}

export function lighten(hex: string, amount: number): string {
  return mixHex(hex, '#ffffff', amount)
}

// ---------------------------------------------------------------------------
// Molduras
// ---------------------------------------------------------------------------

export type FrameKind =
  | 'W'
  | 'U'
  | 'B'
  | 'R'
  | 'G'
  | 'gold'
  | 'artifact'
  | 'colorless'
  | 'land'

export interface FrameTone {
  /** Stop claro da moldura. */
  light: string
  /** Stop escuro da moldura. */
  dark: string
  /** Placa das barras de nome e de tipo. */
  bar: string
  /** Tinta sobre as barras. */
  barInk: string
  /** Fundo da caixa de texto. */
  plate: string
  /** Tinta da caixa de texto. */
  plateInk: string
}

const TONES: Record<FrameKind, FrameTone> = {
  W: {
    light: '#fffdf3',
    dark: '#c7b78c',
    bar: '#6d654f',
    barInk: '#fdf9ec',
    plate: '#f7f2e2',
    plateInk: '#1b170e',
  },
  U: {
    light: '#7cb6e3',
    dark: '#1e4770',
    bar: '#122e49',
    barInk: '#eaf3fb',
    plate: '#e4edf5',
    plateInk: '#0f1a25',
  },
  B: {
    light: '#7a7181',
    dark: '#171120',
    bar: '#0e0a15',
    barInk: '#ece7f2',
    plate: '#d7d2dc',
    plateInk: '#15111c',
  },
  R: {
    light: '#ef8f6b',
    dark: '#8b2a15',
    bar: '#4f1708',
    barInk: '#fdeee7',
    plate: '#f8e7dc',
    plateInk: '#1f0d06',
  },
  G: {
    light: '#80bd8b',
    dark: '#1b562c',
    bar: '#0e2f19',
    barInk: '#ecf6ec',
    plate: '#e5f0e4',
    plateInk: '#0e1a11',
  },
  gold: {
    light: '#f4e2a6',
    dark: '#9c7724',
    bar: '#553f10',
    barInk: '#fcf3dc',
    plate: '#f8f0da',
    plateInk: '#201806',
  },
  artifact: {
    light: '#bdc8d2',
    dark: '#434f59',
    bar: '#252c33',
    barInk: '#eef2f6',
    plate: '#e9edf1',
    plateInk: '#0f1419',
  },
  colorless: {
    light: '#cbcbd0',
    dark: '#4d4d54',
    bar: '#2a2a31',
    barInk: '#f0f0f3',
    plate: '#eeeef1',
    plateInk: '#131317',
  },
  land: {
    light: '#c9a983',
    dark: '#533c23',
    bar: '#302211',
    barInk: '#f6ece0',
    plate: '#f0e5d3',
    plateInk: '#1a1208',
  },
}

export interface FramePaint {
  kind: FrameKind
  /** Preenchido só quando a carta é exatamente bicolor. */
  secondKind: FrameKind | null
  tone: FrameTone
  secondTone: FrameTone | null
  colors: ManaColor[]
}

function typeLineHas(card: CardView, word: string): boolean {
  return (card.typeLine ?? '').toLowerCase().includes(word)
}

export function framePaintFor(card: CardView): FramePaint {
  const colors = colorsFromMask(card.colors)

  if (typeLineHas(card, 'land')) {
    return { kind: 'land', secondKind: null, tone: TONES.land, secondTone: null, colors }
  }
  if (colors.length === 0) {
    const kind: FrameKind = typeLineHas(card, 'artifact') ? 'artifact' : 'colorless'
    return { kind, secondKind: null, tone: TONES[kind], secondTone: null, colors }
  }
  if (colors.length === 1) {
    const kind = colors[0] as FrameKind
    return { kind, secondKind: null, tone: TONES[kind], secondTone: null, colors }
  }
  if (colors.length === 2) {
    const first = colors[0] as FrameKind
    const second = colors[1] as FrameKind
    // Bicolor guarda as duas cores na moldura mas usa placa dourada nas barras e
    // na caixa de texto, como faz a carta impressa.
    return {
      kind: first,
      secondKind: second,
      tone: {
        ...TONES[first],
        bar: TONES.gold.bar,
        barInk: TONES.gold.barInk,
        plate: TONES.gold.plate,
        plateInk: TONES.gold.plateInk,
      },
      secondTone: TONES[second],
      colors,
    }
  }
  return { kind: 'gold', secondKind: null, tone: TONES.gold, secondTone: null, colors }
}

// ---------------------------------------------------------------------------
// Tamanhos
// ---------------------------------------------------------------------------

export type CardSize = 'micro' | 'small' | 'medium' | 'large'

export interface SizeSpec {
  /** Largura em px; a altura sai de `aspect-ratio: 5 / 7`. */
  width: number
  /** `compact` esconde tipo e texto, deixando a arte dominar. */
  layout: 'compact' | 'full'
  /** Flavor text só cabe no zoom. */
  showFlavor: boolean
}

export const CARD_SIZES: Record<CardSize, SizeSpec> = {
  micro: { width: 44, layout: 'compact', showFlavor: false },
  small: { width: 102, layout: 'compact', showFlavor: false },
  medium: { width: 170, layout: 'full', showFlavor: false },
  large: { width: 316, layout: 'full', showFlavor: true },
}

// ---------------------------------------------------------------------------
// Custo de mana
// ---------------------------------------------------------------------------

export type ManaToken =
  | { kind: 'generic'; text: string }
  | { kind: 'color'; color: ManaColor }
  | { kind: 'hybrid'; a: ManaColor; b: ManaColor }
  | { kind: 'monoHybrid'; generic: string; color: ManaColor }
  | { kind: 'phyrexian'; color: ManaColor }
  | { kind: 'colorless' }
  | { kind: 'snow' }
  | { kind: 'variable'; letter: string }
  | { kind: 'tap' }
  | { kind: 'untap' }
  | { kind: 'text'; text: string }

const MANA_COLOR_LETTERS: ReadonlySet<string> = new Set(['W', 'U', 'B', 'R', 'G'])

function asManaColor(letter: string): ManaColor | null {
  return MANA_COLOR_LETTERS.has(letter) ? (letter as ManaColor) : null
}

/** Interpreta o miolo de um símbolo, ou seja, o que está entre as chaves. */
export function parseManaSymbol(body: string): ManaToken {
  const raw = body.trim().toUpperCase()
  if (/^\d+$/.test(raw)) return { kind: 'generic', text: raw }
  if (raw === 'C') return { kind: 'colorless' }
  if (raw === 'S') return { kind: 'snow' }
  if (raw === 'T') return { kind: 'tap' }
  if (raw === 'Q') return { kind: 'untap' }
  if (raw === 'X' || raw === 'Y' || raw === 'Z') return { kind: 'variable', letter: raw }

  const single = asManaColor(raw)
  if (single) return { kind: 'color', color: single }

  const parts = raw.split('/')
  if (parts.length === 2) {
    const [left, right] = parts
    const leftColor = asManaColor(left)
    const rightColor = asManaColor(right)
    if (leftColor && right === 'P') return { kind: 'phyrexian', color: leftColor }
    if (leftColor && rightColor) return { kind: 'hybrid', a: leftColor, b: rightColor }
    if (/^\d+$/.test(left) && rightColor) {
      return { kind: 'monoHybrid', generic: left, color: rightColor }
    }
  }
  return { kind: 'text', text: raw }
}

/** `"{2}{W}{W}"` vira a lista de símbolos; texto solto vira token `text`. */
export function parseManaCost(cost: string | null): ManaToken[] {
  if (!cost) return []
  const tokens: ManaToken[] = []
  const pattern = /\{([^}]*)\}/g
  let cursor = 0
  let match = pattern.exec(cost)
  while (match !== null) {
    if (match.index > cursor) {
      const loose = cost.slice(cursor, match.index).trim()
      if (loose) tokens.push({ kind: 'text', text: loose })
    }
    tokens.push(parseManaSymbol(match[1]))
    cursor = match.index + match[0].length
    match = pattern.exec(cost)
  }
  const tail = cost.slice(cursor).trim()
  if (tail) tokens.push({ kind: 'text', text: tail })
  return tokens
}

// ---------------------------------------------------------------------------
// Texto de regras
// ---------------------------------------------------------------------------

export type TextRun =
  | { kind: 'plain'; text: string }
  | { kind: 'reminder'; text: string }
  | { kind: 'symbol'; token: ManaToken }

/** Quebra o oracle text em linhas, e cada linha em trechos com símbolos. */
export function parseRulesText(text: string): TextRun[][] {
  return text.split('\n').map((line) => {
    const runs: TextRun[] = []
    const pattern = /\{([^}]*)\}|\(([^)]*)\)/g
    let cursor = 0
    let match = pattern.exec(line)
    while (match !== null) {
      if (match.index > cursor) {
        runs.push({ kind: 'plain', text: line.slice(cursor, match.index) })
      }
      if (match[1] !== undefined) {
        runs.push({ kind: 'symbol', token: parseManaSymbol(match[1]) })
      } else {
        runs.push({ kind: 'reminder', text: `(${match[2]})` })
      }
      cursor = match.index + match[0].length
      match = pattern.exec(line)
    }
    if (cursor < line.length) runs.push({ kind: 'plain', text: line.slice(cursor) })
    return runs
  })
}

// ---------------------------------------------------------------------------
// P/T, dano e marcadores
// ---------------------------------------------------------------------------

export interface PtDisplay {
  power: number
  toughness: number
  /** P/T impresso, só quando difere do atual. */
  printed: string | null
  buffed: boolean
  weakened: boolean
  damage: number
  /** Resistência que ainda sobra antes de morrer. */
  remaining: number
  lethal: boolean
}

export function ptDisplay(card: CardView): PtDisplay | null {
  if (card.power === null || card.toughness === null) return null
  const powerDelta = card.basePower === null ? 0 : card.power - card.basePower
  const toughnessDelta = card.baseToughness === null ? 0 : card.toughness - card.baseToughness
  const changed = powerDelta !== 0 || toughnessDelta !== 0
  const printed =
    changed && card.basePower !== null && card.baseToughness !== null
      ? `${card.basePower}/${card.baseToughness}`
      : null
  const damage = Math.max(0, card.damage)
  return {
    power: card.power,
    toughness: card.toughness,
    printed,
    buffed: changed && powerDelta + toughnessDelta >= 0,
    weakened: changed && powerDelta + toughnessDelta < 0,
    damage,
    remaining: card.toughness - damage,
    lethal: damage > 0 && damage >= card.toughness,
  }
}

export type CounterTone = 'plus' | 'minus' | 'loyalty' | 'neutral'

export interface CounterBadge {
  label: string
  tone: CounterTone
  count: number
}

const COUNTER_LABELS: Record<string, { label: string; tone: CounterTone }> = {
  plusoneplusone: { label: '+1/+1', tone: 'plus' },
  minusoneminusone: { label: '-1/-1', tone: 'minus' },
  loyalty: { label: 'LOY', tone: 'loyalty' },
  charge: { label: 'CHG', tone: 'neutral' },
  poison: { label: 'PSN', tone: 'minus' },
  stun: { label: 'STUN', tone: 'minus' },
  shield: { label: 'SHLD', tone: 'plus' },
}

export function counterBadges(card: CardView): CounterBadge[] {
  return card.counters
    .filter(([, count]) => count !== 0)
    .map(([rawKind, count]) => {
      const key = rawKind.toLowerCase().replace(/[^a-z0-9]/g, '')
      const known = COUNTER_LABELS[key]
      if (known) return { label: known.label, tone: known.tone, count }
      return {
        label: humanizeIdentifier(rawKind).slice(0, 5).toUpperCase(),
        tone: 'neutral' as const,
        count,
      }
    })
}

// ---------------------------------------------------------------------------
// Combate
// ---------------------------------------------------------------------------

export type CombatRole = 'attacking' | 'blocking' | 'blocked' | null

export function combatRole(card: CardView): CombatRole {
  if (card.attacking !== null) return 'attacking'
  if (card.blocking.length > 0) return 'blocking'
  if (card.blockedBy.length > 0) return 'blocked'
  return null
}

/**
 * `Defender` é enum Rust com payload, então chega como `{ "Player": 0 }`. Uma
 * variante sem payload chegaria como string simples, e o motor pode ganhar uma
 * dessas — por isso os dois formatos são tratados.
 */
export function defenderLabel(defender: Defender): string {
  if (typeof defender === 'string') return humanizeIdentifier(defender)
  const entry = Object.entries(defender as Record<string, unknown>)[0]
  if (!entry) return 'defender'
  const [variant, payload] = entry
  if (variant === 'Player') return `player ${String(payload)}`
  if (variant === 'Planeswalker') return 'planeswalker'
  if (variant === 'Battle') return 'battle'
  return humanizeIdentifier(variant)
}

// ---------------------------------------------------------------------------
// Palavras-chave
// ---------------------------------------------------------------------------

/** `"FirstStrike"` vira `"First Strike"`; `"first_strike"` também. */
export function humanizeIdentifier(raw: string): string {
  const spaced = raw.replace(/[_-]+/g, ' ').replace(/([a-z0-9])([A-Z])/g, '$1 $2')
  return spaced.charAt(0).toUpperCase() + spaced.slice(1)
}

const GLOSSARY: Record<string, string> = {
  flying: 'Só pode ser bloqueada por criaturas com voar ou alcance.',
  reach: 'Pode bloquear criaturas com voar.',
  trample: 'Dano além do letal dos bloqueadores passa para o defensor.',
  firststrike: 'Causa dano de combate antes das criaturas sem iniciativa.',
  doublestrike: 'Causa dano na etapa de iniciativa e de novo na etapa normal.',
  deathtouch: 'Qualquer dano que ela cause a uma criatura é letal.',
  lifelink: 'O dano que ela causa também faz você ganhar essa vida.',
  vigilance: 'Atacar não faz esta criatura virar.',
  haste: 'Pode atacar e usar habilidades de virar assim que entra em jogo.',
  menace: 'Só pode ser bloqueada por duas ou mais criaturas.',
  defender: 'Não pode atacar.',
  flash: 'Pode ser conjurada quando você puder conjurar um instantâneo.',
  hexproof: 'Não pode ser alvo de mágicas ou habilidades dos oponentes.',
  shroud: 'Não pode ser alvo de nenhuma mágica ou habilidade.',
  indestructible: 'Dano e efeitos de destruir não a destroem.',
  prowess: 'Recebe +1/+1 até o fim do turno quando você conjura mágica não-criatura.',
  intimidate: 'Só é bloqueada por artefatos e por criaturas que compartilhem uma cor.',
  fear: 'Só pode ser bloqueada por artefatos e por criaturas pretas.',
  skulk: 'Não pode ser bloqueada por criaturas de poder maior.',
  exalted: 'Se ela atacar sozinha, recebe +1/+1 até o fim do turno.',
  riot: 'Entra com um marcador +1/+1 ou com pressa, à sua escolha.',
  convoke: 'Vire criaturas para ajudar a pagar o custo.',
  delve: 'Exile cartas do cemitério para ajudar a pagar o custo.',
  cascade: 'Ao conjurar, exile até achar uma mágica mais barata e conjure-a grátis.',
  storm: 'Copie a mágica para cada mágica conjurada antes dela neste turno.',
  ward: 'Cobra um custo adicional de quem tenta mirá-la.',
  protection: 'Não é bloqueada, alvejada, encantada nem recebe dano daquela qualidade.',
  annihilator: 'Ao atacar, o defensor sacrifica permanentes.',
  afflict: 'Se ela for bloqueada, o defensor perde vida.',
  landwalk: 'Não pode ser bloqueada se o defensor controlar o terreno indicado.',
}

export interface KeywordEntry {
  keyword: string
  explanation: string | null
}

export function explainKeywords(keywords: string[]): KeywordEntry[] {
  return keywords.map((keyword) => {
    const key = keyword.toLowerCase().replace(/[^a-z]/g, '')
    // Ward(Cost) e Protection(Color) chegam com o payload colado, por isso o
    // casamento por prefixo depois da tentativa exata.
    const explanation =
      GLOSSARY[key] ?? Object.entries(GLOSSARY).find(([term]) => key.startsWith(term))?.[1] ?? null
    return { keyword: humanizeIdentifier(keyword), explanation }
  })
}

// ---------------------------------------------------------------------------
// Arte procedural, usada quando o Scryfall não responde
// ---------------------------------------------------------------------------

export function hashString(value: string): number {
  let hash = 2166136261
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  return hash >>> 0
}

/** Gerador determinístico, para a arte não tremer entre renders. */
function makeRandom(seed: number): () => number {
  let state = seed || 1
  return () => {
    state ^= state << 13
    state ^= state >>> 17
    state ^= state << 5
    state >>>= 0
    return state / 4294967296
  }
}

export interface ProceduralRidge {
  path: string
  fill: string
}

export interface ProceduralArt {
  skyTop: string
  skyBottom: string
  orb: string
  orbX: number
  orbY: number
  orbR: number
  ridges: ProceduralRidge[]
}

const ART_VIEWBOX_HEIGHT = 72

/**
 * Paisagem abstrata derivada do nome: mesma carta, mesma arte, sempre. Existe
 * para que uma falha de rede nunca vire caixa quebrada na mesa.
 */
export function proceduralArt(seedText: string, paint: FramePaint): ProceduralArt {
  const random = makeRandom(hashString(seedText))
  const near = paint.tone
  const far = paint.secondTone ?? paint.tone

  const ridges: ProceduralRidge[] = []
  const layerCount = 3
  for (let layer = 0; layer < layerCount; layer += 1) {
    const baseline = 30 + layer * 12
    const amplitude = 18 - layer * 5
    const points: string[] = []
    const steps = 5
    for (let step = 0; step <= steps; step += 1) {
      const x = (step / steps) * 100
      const y = baseline + (random() - 0.5) * amplitude
      points.push(`${x.toFixed(1)},${y.toFixed(1)}`)
    }
    const firstY = points[0].split(',')[1]
    ridges.push({
      path: `M-2,${ART_VIEWBOX_HEIGHT} L-2,${firstY} L${points.join(' L')} L102,${ART_VIEWBOX_HEIGHT} Z`,
      fill: mixHex(mixHex(far.dark, near.dark, 0.4), '#05040a', layer / layerCount),
    })
  }

  return {
    skyTop: lighten(near.light, 0.34),
    skyBottom: darken(far.dark, 0.14),
    orb: lighten(near.light, 0.6),
    orbX: 18 + random() * 64,
    orbY: 11 + random() * 12,
    orbR: 5 + random() * 7,
    ridges,
  }
}

// ---------------------------------------------------------------------------
// Arte do Scryfall
// ---------------------------------------------------------------------------

export function scryfallArtUrl(card: Pick<CardView, 'artKey' | 'name'>): string | null {
  const key = card.artKey ?? card.name
  if (!key) return null
  return `https://api.scryfall.com/cards/named?exact=${encodeURIComponent(key)}&format=image&version=art_crop`
}

// ---------------------------------------------------------------------------
// Raridade
// ---------------------------------------------------------------------------

const RARITY_INKS: Record<string, string> = {
  common: '#16161b',
  uncommon: '#b6c3ce',
  rare: '#d8bd6a',
  mythic: '#e07028',
  special: '#c58ce0',
}

export function rarityInk(rarity: string | null): string {
  if (!rarity) return RARITY_INKS.common
  return RARITY_INKS[rarity.toLowerCase()] ?? RARITY_INKS.common
}

/** Sigla do set para o selo; sem set, usa as iniciais do nome. */
export function setStamp(card: Pick<CardView, 'setCode' | 'name'>): string {
  if (card.setCode) return card.setCode.toUpperCase().slice(0, 3)
  if (!card.name) return '??'
  return card.name
    .split(/\s+/)
    .map((word) => word[0])
    .join('')
    .toUpperCase()
    .slice(0, 3)
}

// ---------------------------------------------------------------------------
// Variáveis CSS
// ---------------------------------------------------------------------------

/**
 * Todo o dimensionamento da carta é derivado de `--cw`, então uma única
 * variável escala tipografia, molduras e sombras juntas.
 */
export interface CardCssVars extends CSSProperties {
  '--cw'?: string
  '--frame-light'?: string
  '--frame-dark'?: string
  '--frame-light-2'?: string
  '--frame-dark-2'?: string
  '--bar'?: string
  '--bar-ink'?: string
  '--plate'?: string
  '--plate-ink'?: string
  '--accent'?: string
  '--rarity'?: string
}

const PLAYER_ACCENTS = ['#f2b544', '#4fc9f0', '#8de06a', '#ef6ac2']

/** Cor do jogador, usada no contorno de combate e nas bordas de controle. */
export function playerAccent(playerId: number): string {
  return PLAYER_ACCENTS[Math.abs(playerId) % PLAYER_ACCENTS.length]
}

export function cardCssVars(paint: FramePaint, size: CardSize, controller: number, rarity: string | null): CardCssVars {
  const second = paint.secondTone ?? paint.tone
  return {
    '--cw': `${CARD_SIZES[size].width}px`,
    '--frame-light': paint.tone.light,
    '--frame-dark': paint.tone.dark,
    '--frame-light-2': second.light,
    '--frame-dark-2': second.dark,
    '--bar': paint.tone.bar,
    '--bar-ink': paint.tone.barInk,
    '--plate': paint.tone.plate,
    '--plate-ink': paint.tone.plateInk,
    '--accent': playerAccent(controller),
    '--rarity': rarityInk(rarity),
  }
}
