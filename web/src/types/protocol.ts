/**
 * Espelho TypeScript de `crates/mtg-core/src/view.rs`, `event.rs`, `card.rs`.
 *
 * Convenções de serialização que o motor usa e que este arquivo reproduz:
 * - structs anotadas com `rename_all = "camelCase"` (View*, GameView) → camelCase;
 * - `CardDef` NÃO tem `rename_all`, então chega em snake_case (ver `card.rs`);
 * - enum sem payload → string ("Untap"); enum com payload → objeto ({"Player": 0});
 * - `MatchEvent` é tag interna `type`, com o NOME DA VARIANTE em camelCase
 *   (os campos continuam com o nome Rust — todos são de uma palavra só).
 */

export type ObjectId = number
export type PlayerId = number

// ---------------------------------------------------------------------------
// Zonas, passos e fases
// ---------------------------------------------------------------------------

export type ZoneKind =
  | 'Library'
  | 'Hand'
  | 'Battlefield'
  | 'Graveyard'
  | 'Stack'
  | 'Exile'
  | 'Command'

export type Phase =
  | 'Beginning'
  | 'PrecombatMain'
  | 'Combat'
  | 'PostcombatMain'
  | 'Ending'

export type Step =
  | 'Untap'
  | 'Upkeep'
  | 'Draw'
  | 'PrecombatMain'
  | 'BeginCombat'
  | 'DeclareAttackers'
  | 'DeclareBlockers'
  | 'FirstStrikeDamage'
  | 'CombatDamage'
  | 'EndCombat'
  | 'PostcombatMain'
  | 'End'
  | 'Cleanup'

/** Mesma ordem de `Step::SEQUENCE`. */
export const STEP_SEQUENCE: readonly Step[] = [
  'Untap',
  'Upkeep',
  'Draw',
  'PrecombatMain',
  'BeginCombat',
  'DeclareAttackers',
  'DeclareBlockers',
  'FirstStrikeDamage',
  'CombatDamage',
  'EndCombat',
  'PostcombatMain',
  'End',
  'Cleanup',
]

export const STEP_LABEL: Readonly<Record<Step, string>> = {
  Untap: 'Untap',
  Upkeep: 'Upkeep',
  Draw: 'Draw',
  PrecombatMain: 'Main 1',
  BeginCombat: 'Begin Combat',
  DeclareAttackers: 'Declare Attackers',
  DeclareBlockers: 'Declare Blockers',
  FirstStrikeDamage: 'First Strike Damage',
  CombatDamage: 'Combat Damage',
  EndCombat: 'End Combat',
  PostcombatMain: 'Main 2',
  End: 'End Step',
  Cleanup: 'Cleanup',
}

/** Rótulo curto o bastante para caber na trilha de fases. */
export const STEP_SHORT_LABEL: Readonly<Record<Step, string>> = {
  Untap: 'UNTAP',
  Upkeep: 'UPKEEP',
  Draw: 'DRAW',
  PrecombatMain: 'MAIN 1',
  BeginCombat: 'COMBAT',
  DeclareAttackers: 'ATTACK',
  DeclareBlockers: 'BLOCK',
  FirstStrikeDamage: '1ST DMG',
  CombatDamage: 'DAMAGE',
  EndCombat: 'END CMB',
  PostcombatMain: 'MAIN 2',
  End: 'END',
  Cleanup: 'CLEANUP',
}

export const PHASE_OF_STEP: Readonly<Record<Step, Phase>> = {
  Untap: 'Beginning',
  Upkeep: 'Beginning',
  Draw: 'Beginning',
  PrecombatMain: 'PrecombatMain',
  BeginCombat: 'Combat',
  DeclareAttackers: 'Combat',
  DeclareBlockers: 'Combat',
  FirstStrikeDamage: 'Combat',
  CombatDamage: 'Combat',
  EndCombat: 'Combat',
  PostcombatMain: 'PostcombatMain',
  End: 'Ending',
  Cleanup: 'Ending',
}

export const PHASE_LABEL: Readonly<Record<Phase, string>> = {
  Beginning: 'Beginning',
  PrecombatMain: 'Main Phase',
  Combat: 'Combat',
  PostcombatMain: 'Second Main',
  Ending: 'Ending',
}

// ---------------------------------------------------------------------------
// Enums com payload
// ---------------------------------------------------------------------------

/** `Defender` (CR 506.2) — enum externamente marcado. */
export type Defender =
  | { Player: PlayerId }
  | { Planeswalker: ObjectId }
  | { Battle: ObjectId }

export function defenderPlayer(d: Defender): PlayerId | null {
  return 'Player' in d ? d.Player : null
}

export function defenderObject(d: Defender): ObjectId | null {
  if ('Planeswalker' in d) return d.Planeswalker
  if ('Battle' in d) return d.Battle
  return null
}

/** `GameOutcome`. `Ongoing`/`Draw` são unit; `Winner` carrega o jogador. */
export type Outcome = 'Ongoing' | 'Draw' | { Winner: PlayerId }

export function outcomeWinner(o: Outcome): PlayerId | null {
  return typeof o === 'object' ? o.Winner : null
}

export function isGameOver(o: Outcome): boolean {
  return o !== 'Ongoing'
}

export type LossReason =
  | 'ZeroLife'
  | 'DrewFromEmptyLibrary'
  | 'PoisonCounters'
  | 'Effect'
  | 'Concede'

// ---------------------------------------------------------------------------
// Cores e mana
// ---------------------------------------------------------------------------

export type Color = 'White' | 'Blue' | 'Black' | 'Red' | 'Green'

/** Ordem WUBRG de `Color::ALL`; o índice é o bit em `ColorSet`. */
export const COLOR_ORDER: readonly Color[] = [
  'White',
  'Blue',
  'Black',
  'Red',
  'Green',
]

/** `ColorSet` é `#[serde(transparent)]` sobre `u8`: chega como bitmask. */
export type ColorSet = number

export function colorsOf(mask: ColorSet): Color[] {
  return COLOR_ORDER.filter((_, i) => (mask & (1 << i)) !== 0)
}

export type ManaSymbol =
  | 'Colorless'
  | 'Snow'
  | 'X'
  | { Generic: number }
  | { Colored: Color }
  | { Hybrid: [Color, Color] }
  | { MonoHybrid: Color }
  | { Phyrexian: Color }

/** `ManaCost` é transparente sobre `Vec<ManaSymbol>`. */
export type ManaCost = ManaSymbol[]

export interface ManaPool {
  /** WUBRG, mesma ordem de `COLOR_ORDER`. */
  colored: [number, number, number, number, number]
  colorless: number
}

export function manaPoolTotal(pool: ManaPool): number {
  return pool.colored.reduce((a, b) => a + b, 0) + pool.colorless
}

// ---------------------------------------------------------------------------
// Tipos de carta
// ---------------------------------------------------------------------------

export type CardType =
  | 'Artifact'
  | 'Battle'
  | 'Creature'
  | 'Enchantment'
  | 'Instant'
  | 'Land'
  | 'Planeswalker'
  | 'Sorcery'
  | 'Kindred'

export type Supertype = 'Basic' | 'Legendary' | 'Snow' | 'World'

export interface TypeLine {
  supertypes: Supertype[]
  types: CardType[]
  subtypes: string[]
}

export type Rarity = 'Common' | 'Uncommon' | 'Rare' | 'Mythic' | 'Special'

// ---------------------------------------------------------------------------
// Projeção para a UI
// ---------------------------------------------------------------------------

export interface CardView {
  id: ObjectId
  /** `null` quando a carta está oculta para o observador. */
  name: string | null
  manaCost: string | null
  manaValue: number
  typeLine: string | null
  oracleText: string | null
  flavorText: string | null
  colors: ColorSet
  power: number | null
  toughness: number | null
  basePower: number | null
  baseToughness: number | null
  loyalty: number | null
  damage: number
  tapped: boolean
  faceDown: boolean
  summoningSick: boolean
  attacking: Defender | null
  blocking: ObjectId[]
  blockedBy: ObjectId[]
  counters: [string, number][]
  keywords: string[]
  attachedTo: ObjectId | null
  attachments: ObjectId[]
  isToken: boolean
  controller: PlayerId
  owner: PlayerId
  zone: ZoneKind
  artKey: string | null
  rarity: string | null
  setCode: string | null
  isLegalTarget: boolean
  isActionable: boolean
}

export interface PlayerView {
  id: PlayerId
  name: string
  life: number
  poison: number
  manaPool: ManaPool
  handCount: number
  libraryCount: number
  graveyardCount: number
  exileCount: number
  landsPlayedThisTurn: number
  maxLandsPerTurn: number
  hasLost: boolean
  isActive: boolean
  hasPriority: boolean
}

export interface StackItemView {
  id: ObjectId
  name: string
  text: string
  controller: PlayerId
  targets: ObjectId[]
  targetPlayers: PlayerId[]
  isAbility: boolean
  sourceCard: ObjectId
  artKey: string | null
}

export interface GameView {
  turn: number
  step: Step
  stepLabel: string
  phase: Phase
  activePlayer: PlayerId
  priorityPlayer: PlayerId | null
  outcome: Outcome
  players: PlayerView[]
  cards: CardView[]
  /** Indexado por `PlayerId`. */
  battlefield: ObjectId[][]
  hands: ObjectId[][]
  graveyards: ObjectId[][]
  exiles: ObjectId[][]
  stack: StackItemView[]
  prompt: string | null
  logTail: string[]
}

// ---------------------------------------------------------------------------
// Eventos de animação
// ---------------------------------------------------------------------------

export type MatchEvent =
  | { type: 'turnStart'; turn: number; player: PlayerId }
  | { type: 'stepChange'; step: Step; label: string }
  | {
      type: 'cardMoved'
      card: ObjectId
      from: ZoneKind
      to: ZoneKind
      owner: PlayerId
      reveal: boolean
    }
  | { type: 'cardDrawn'; card: ObjectId; player: PlayerId }
  | { type: 'spellCast'; card: ObjectId; player: PlayerId; targets: ObjectId[] }
  | { type: 'spellResolved'; card: ObjectId }
  | { type: 'spellCountered'; card: ObjectId }
  | { type: 'abilityTriggered'; source: ObjectId; text: string }
  | { type: 'tapped'; card: ObjectId }
  | { type: 'untapped'; card: ObjectId }
  | { type: 'attackersDeclared'; attackers: [ObjectId, Defender][] }
  | { type: 'blockersDeclared'; blocks: [ObjectId, ObjectId[]][] }
  | {
      type: 'damageDealt'
      source: ObjectId
      target: ObjectId
      amount: number
      lethal: boolean
    }
  | {
      type: 'damageToPlayer'
      source: ObjectId
      player: PlayerId
      amount: number
    }
  | { type: 'lifeChanged'; player: PlayerId; delta: number; total: number }
  | { type: 'countersChanged'; card: ObjectId; kind: string; delta: number }
  | { type: 'tokenCreated'; card: ObjectId; controller: PlayerId }
  | { type: 'died'; card: ObjectId }
  | { type: 'destroyed'; card: ObjectId }
  | { type: 'exiled'; card: ObjectId }
  | { type: 'playerLost'; player: PlayerId; reason: string }
  | { type: 'gameOver'; outcome: Outcome }
  | { type: 'log'; text: string }

export type MatchEventType = MatchEvent['type']

/**
 * Porte de `MatchEvent::suggested_duration_ms`. O motor não serializa o método,
 * então o ritmo precisa viver aqui — e precisa bater com o Rust, senão a UI
 * dessincroniza do log do servidor.
 */
export function suggestedDurationMs(event: MatchEvent): number {
  switch (event.type) {
    case 'turnStart':
      return 900
    case 'stepChange':
      return 220
    case 'spellCast':
      return 700
    case 'spellResolved':
      return 450
    case 'spellCountered':
      return 650
    case 'abilityTriggered':
      return 600
    case 'cardMoved':
    case 'cardDrawn':
      return 380
    case 'attackersDeclared':
      return 800
    case 'blockersDeclared':
      return 700
    case 'damageDealt':
    case 'damageToPlayer':
      return 520
    case 'lifeChanged':
      return 400
    case 'died':
    case 'destroyed':
      return 600
    case 'gameOver':
      return 1600
    default:
      return 260
  }
}

// ---------------------------------------------------------------------------
// Catálogo (`GET /api/cards`)
// ---------------------------------------------------------------------------

/**
 * `CardDef` chega em snake_case (a struct em `card.rs` não tem `rename_all`).
 * `abilities`/`spell_effect`/`spell_targets` são a IR do motor; a UI não a
 * interpreta, então ficam como `unknown` em vez de um espelho que apodrece.
 */
export interface CardDefWire {
  id: number
  name: string
  mana_cost?: ManaCost
  manaCost?: ManaCost
  type_line?: TypeLine
  typeLine?: TypeLine
  color_override?: ColorSet | null
  colorOverride?: ColorSet | null
  power?: number | null
  toughness?: number | null
  loyalty?: number | null
  abilities?: unknown[]
  spell_effect?: unknown
  spell_targets?: unknown[]
  oracle_text?: string
  oracleText?: string
  flavor_text?: string | null
  flavorText?: string | null
  rarity?: Rarity
  set_code?: string
  setCode?: string
  collector_number?: string
  collectorNumber?: string
  artist?: string | null
  art_key?: string | null
  artKey?: string | null
}

/** Forma normalizada que a UI consome, sempre camelCase e sem opcionais. */
export interface CardDef {
  id: number
  name: string
  manaCost: ManaCost
  typeLine: TypeLine
  colorOverride: ColorSet | null
  power: number | null
  toughness: number | null
  loyalty: number | null
  oracleText: string
  flavorText: string | null
  rarity: Rarity
  setCode: string
  collectorNumber: string
  artist: string | null
  artKey: string | null
}

// ---------------------------------------------------------------------------
// Protocolo de rede (`/ws/match`)
// ---------------------------------------------------------------------------

export interface InitFrame {
  type: 'init'
  view: GameView
  players: string[]
  seed: number
}

export interface EventsFrame {
  type: 'events'
  events: MatchEvent[]
  view: GameView
}

export interface DoneFrame {
  type: 'done'
  outcome: Outcome
  turns: number
  durationMs: number
}

export type ServerFrame = InitFrame | EventsFrame | DoneFrame

export interface StartFrame {
  type: 'start'
  deckA: string
  deckB: string
  seed: number
  speed: number
}

export type ClientFrame =
  | StartFrame
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'step' }

/** Guarda de runtime: JSON de rede não é confiável só porque tem tipo. */
export function isServerFrame(value: unknown): value is ServerFrame {
  if (typeof value !== 'object' || value === null) return false
  const tag = (value as { type?: unknown }).type
  return tag === 'init' || tag === 'events' || tag === 'done'
}

// ---------------------------------------------------------------------------
// Ajudas de leitura de `CardView`
// ---------------------------------------------------------------------------

export function isCreature(card: CardView): boolean {
  return (card.typeLine ?? '').toLowerCase().includes('creature')
}

export function isLand(card: CardView): boolean {
  return (card.typeLine ?? '').toLowerCase().includes('land')
}

export function isPlaneswalker(card: CardView): boolean {
  return (card.typeLine ?? '').toLowerCase().includes('planeswalker')
}
