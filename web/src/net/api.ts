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
