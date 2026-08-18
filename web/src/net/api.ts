import type { CardDef, CardDefWire, ManaCost, Rarity, TypeLine } from '../types/protocol'

/** Origem do `mtg-server`. Sobrescreva com `VITE_API_BASE` ao servir fora do localhost. */
export const API_BASE: string =
  import.meta.env.VITE_API_BASE ?? 'http://localhost:8787'

export const MATCH_SOCKET_URL: string = API_BASE.replace(/^http/, 'ws') + '/ws/match'

export interface DeckInfo {
  id: string
  name: string
  /**
   * Identidade de cor em letras WUBRG. Opcional porque só o servidor a manda
   * (`DeckSummary.colors`) — as listas embutidas de fallback podem omitir.
   */
  colorIdentity?: readonly string[]
}

/** `Color` do motor chega por extenso; a UI trabalha com a letra. */
const COLOR_LETTER: Readonly<Record<string, string>> = {
  white: 'W',
  blue: 'U',
  black: 'B',
  red: 'R',
  green: 'G',
  w: 'W',
  u: 'U',
  b: 'B',
  r: 'R',
  g: 'G',
}

function normalizeColors(value: unknown): readonly string[] | undefined {
  const raw: unknown[] = typeof value === 'string' ? [...value] : Array.isArray(value) ? value : []
  const letters: string[] = []
  for (const item of raw) {
    if (typeof item !== 'string') continue
    const letter = COLOR_LETTER[item.toLowerCase()]
    if (letter !== undefined && !letters.includes(letter)) letters.push(letter)
  }
  return letters.length > 0 ? letters : undefined
}

const EMPTY_TYPE_LINE: TypeLine = { supertypes: [], types: [], subtypes: [] }

async function getJson(path: string, signal?: AbortSignal): Promise<unknown> {
  const res = await fetch(API_BASE + path, { signal })
  if (!res.ok) throw new Error(`${path} respondeu ${res.status}`)
  return (await res.json()) as unknown
}

/**
 * `GET /api/decks`. O contrato não fixa a forma do item, então aceitamos tanto
 * uma lista de ids quanto objetos `{id, name}` e normalizamos.
 */
export async function fetchDecks(signal?: AbortSignal): Promise<DeckInfo[]> {
  const body = await getJson('/api/decks', signal)
  if (!Array.isArray(body)) return []
  const decks: DeckInfo[] = []
  for (const item of body) {
    if (typeof item === 'string') {
      decks.push({ id: item, name: prettifyDeckId(item) })
      continue
    }
    if (typeof item !== 'object' || item === null) continue
    const record = item as Record<string, unknown>
    const id = typeof record.id === 'string' ? record.id : null
    if (id === null) continue
    const name = typeof record.name === 'string' ? record.name : prettifyDeckId(id)
    decks.push({ id, name, colorIdentity: normalizeColors(record.colors) })
  }
  return decks
}

/**
 * `GET /api/catalog` — o catálogo curado, como array de `CardDef`.
 *
 * Não é `/api/cards`: aquela rota virou busca paginada quando o catálogo do
 * Scryfall entrou (32 mil cartas), e responde um objeto de página, não um
 * array. Quem monta partida quer as cartas jogáveis inteiras, que é o que
 * `/api/catalog` entrega.
 *
 * Aceita snake_case (forma atual do `CardDef`) e camelCase.
 */
export async function fetchCardCatalog(signal?: AbortSignal): Promise<CardDef[]> {
  const body = await getJson('/api/catalog', signal)
  if (!Array.isArray(body)) return []
  return body
    .filter((item): item is CardDefWire => isCardDefWire(item))
    .map(normalizeCardDef)
}

function isCardDefWire(value: unknown): boolean {
  if (typeof value !== 'object' || value === null) return false
  const record = value as Record<string, unknown>
  return typeof record.id === 'number' && typeof record.name === 'string'
}

export function normalizeCardDef(wire: CardDefWire): CardDef {
  return {
    id: wire.id,
    name: wire.name,
    manaCost: (wire.mana_cost ?? wire.manaCost ?? []) as ManaCost,
    typeLine: wire.type_line ?? wire.typeLine ?? EMPTY_TYPE_LINE,
    colorOverride: wire.color_override ?? wire.colorOverride ?? null,
    power: wire.power ?? null,
    toughness: wire.toughness ?? null,
    loyalty: wire.loyalty ?? null,
    oracleText: wire.oracle_text ?? wire.oracleText ?? '',
    flavorText: wire.flavor_text ?? wire.flavorText ?? null,
    rarity: (wire.rarity ?? 'Common') as Rarity,
    setCode: wire.set_code ?? wire.setCode ?? '',
    collectorNumber: wire.collector_number ?? wire.collectorNumber ?? '',
    artist: wire.artist ?? null,
    artKey: wire.art_key ?? wire.artKey ?? null,
  }
}

function prettifyDeckId(id: string): string {
  return id
    .split(/[-_\s]+/)
    .filter((part) => part.length > 0)
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join(' ')
}

// ---------------------------------------------------------------------------
// Catálogo amplo (Scryfall) — `GET /api/cards`
// ---------------------------------------------------------------------------

/**
 * Um item de `GET /api/cards`. É o `CardSummary` de `mtg-db`: metadado para
 * busca e navegação, NÃO a IR que o motor executa (essa só vem em
 * `/api/cards/:oracle_id`). Campos em snake_case porque é o que o serde manda.
 */
export interface CardSummary {
  oracleId: string | null
  name: string
  manaCost: string
  manaValue: number
  /** Letras WUBRG, como o servidor serializa `Color`. */
  colors: readonly string[]
  typeLine: string
  power: number | null
  toughness: number | null
  rarity: string
  setCode: string
  oracleText: string
  /** `false` quando o compilador não traduziu o texto — a carta existe no
   *  catálogo, mas sem habilidade nenhuma e fora de qualquer deck. */
  playable: boolean
  unsupportedReason: string | null
  /** URL direta do CDN do Scryfall; `null` para carta curada em Lua. */
  imageArtCrop: string | null
}

/** Uma página de resultado. `total` é o tamanho do resultado inteiro. */
export interface CardPage {
  total: number
  limit: number
  offset: number
  items: CardSummary[]
}

/** Filtros de `GET /api/cards`. Campo ausente é filtro não aplicado. */
export interface CardSearch {
  text?: string
  /** Letras WUBRG. Vazio não filtra. */
  colors?: readonly string[]
  playable?: boolean
  limit: number
  offset: number
}

const EMPTY_PAGE: CardPage = { total: 0, limit: 0, offset: 0, items: [] }

function asString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback
}

function asNullableString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null
}

function asNumber(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function asNullableNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function normalizeCardSummary(value: unknown): CardSummary | null {
  if (typeof value !== 'object' || value === null) return null
  const r = value as Record<string, unknown>
  // Sem nome a carta não é renderizável nem identificável — descartar é melhor
  // que exibir um buraco na grade.
  if (typeof r.name !== 'string' || r.name.length === 0) return null
  return {
    oracleId: asNullableString(r.oracle_id),
    name: r.name,
    manaCost: asString(r.mana_cost),
    manaValue: asNumber(r.mana_value),
    colors: Array.isArray(r.colors) ? r.colors.filter((c): c is string => typeof c === 'string') : [],
    typeLine: asString(r.type_line),
    power: asNullableNumber(r.power),
    toughness: asNullableNumber(r.toughness),
    rarity: asString(r.rarity, 'common'),
    setCode: asString(r.set_code),
    oracleText: asString(r.oracle_text),
    playable: r.playable === true,
    unsupportedReason: asNullableString(r.unsupported_reason),
    imageArtCrop: asNullableString(r.image_art_crop),
  }
}

/**
 * `GET /api/cards` — busca paginada no catálogo inteiro (~32 mil cartas).
 *
 * O servidor limita `limit` a 200 e responde 400 em parâmetro malformado, então
 * o erro sobe para quem chamou em vez de virar página vazia silenciosa.
 */
export async function fetchCardPage(search: CardSearch, signal?: AbortSignal): Promise<CardPage> {
  const params = new URLSearchParams()
  const text = search.text?.trim() ?? ''
  if (text.length > 0) params.set('q', text)
  if (search.colors !== undefined && search.colors.length > 0) params.set('colors', search.colors.join(','))
  if (search.playable !== undefined) params.set('playable', String(search.playable))
  params.set('limit', String(search.limit))
  params.set('offset', String(search.offset))

  const body = await getJson(`/api/cards?${params.toString()}`, signal)
  if (typeof body !== 'object' || body === null) return EMPTY_PAGE
  const r = body as Record<string, unknown>
  const items = Array.isArray(r.items)
    ? r.items.map(normalizeCardSummary).filter((c): c is CardSummary => c !== null)
    : []
  return {
    total: asNumber(r.total),
    limit: asNumber(r.limit, search.limit),
    offset: asNumber(r.offset, search.offset),
    items,
  }
}

/** `GET /api/stats` — os números do catálogo inteiro, curadas incluídas. */
export interface CatalogStats {
  total: number
  playable: number
  unsupported: number
}

export async function fetchCatalogStats(signal?: AbortSignal): Promise<CatalogStats> {
  const body = await getJson('/api/stats', signal)
  if (typeof body !== 'object' || body === null) return { total: 0, playable: 0, unsupported: 0 }
  const r = body as Record<string, unknown>
  return {
    total: asNumber(r.total),
    playable: asNumber(r.playable),
    unsupported: asNumber(r.unsupported),
  }
}
