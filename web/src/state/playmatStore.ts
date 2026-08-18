import { create } from 'zustand'
import type { Color } from '../types/protocol'

/** Assento na mesa: `0` é o jogador de baixo, `1` o de cima. */
export type Seat = 0 | 1

/**
 * Chave versionada. Se a forma guardada mudar, sobe para `v2` e o conteúdo
 * velho passa a ser ignorado — mais barato e mais seguro que migrar.
 */
const STORAGE_KEY = 'mtgpro.playmat.v1'

/**
 * Uma data URL de 256 KB já é abuso para um tapete, e `localStorage` costuma
 * ter 5 MB no total: sem teto, uma imagem colada derruba a persistência toda.
 */
const MAX_URL_LENGTH = 262_144

/**
 * `svg+xml` fica de fora de propósito: é o único tipo de imagem que carrega
 * marcação, e o custo de errar aqui é XSS.
 */
const DATA_IMAGE = /^data:image\/(?:png|jpe?g|gif|webp|avif);base64,[A-Za-z0-9+/=\s]+$/i

/**
 * Portão de segurança da URL colada: o valor vai direto para `background-image`
 * e para `<img src>`, então só `https:` e `data:image/` passam. Qualquer outro
 * esquema — `javascript:`, `http:`, `blob:`, caminho relativo — é recusado.
 */
export function isSafeImageUrl(raw: string): boolean {
  const url = raw.trim()
  if (url.length === 0 || url.length > MAX_URL_LENGTH) return false
  if (url.slice(0, 5).toLowerCase() === 'data:') return DATA_IMAGE.test(url)
  try {
    return new URL(url).protocol === 'https:'
  } catch {
    return false
  }
}

/** Letras de identidade do baralho, como o servidor as manda em `DeckInfo`. */
const LETTER_COLOR: Readonly<Record<string, Color>> = {
  W: 'White',
  U: 'Blue',
  B: 'Black',
  R: 'Red',
  G: 'Green',
}

/** Converte a identidade de cor do baralho no pigmento do feltro. */
export function tintFromIdentity(identity: readonly string[]): Color[] {
  const out: Color[] = []
  for (const letter of identity) {
    const color = LETTER_COLOR[letter.toUpperCase()]
    if (color !== undefined && !out.includes(color)) out.push(color)
  }
  return out
}

export interface PlaymatStore {
  /** Arte do tapete por assento; `null` cai no gradiente procedural do deck. */
  art: [string | null, string | null]
  /**
   * Cores do baralho do assento. Só aparecem quando NÃO há arte — é o feltro
   * liso tingido. Ficam fora do `localStorage` porque pertencem à partida
   * corrente, não à preferência do usuário.
   */
  tint: [Color[], Color[]]
  setTint: (seat: Seat, colors: Color[]) => void
  /**
   * Assentos em que o usuário já decidiu — inclusive quando a decisão foi
   * "nenhuma arte". Sem isso, um `null` guardado seria indistinguível de
   * "nunca escolheu" e o seletor reimporia o padrão a cada recarga.
   */
  chosen: [boolean, boolean]
  /** Escolha do usuário: manda no que o tapete mostra e trava o padrão. */
  setArt: (seat: Seat, url: string | null) => void
  /**
   * Padrão proposto pela interface. Pinta o tapete sem se passar por decisão
   * do usuário, então continua cedendo lugar quando o baralho muda ou quando o
   * catálogo do servidor substitui o embutido.
   */
  suggestArt: (seat: Seat, url: string | null) => void
}

interface StoredPlaymat {
  art: [string | null, string | null]
  chosen: [boolean, boolean]
}

const EMPTY: StoredPlaymat = { art: [null, null], chosen: [false, false] }

function sanitize(value: unknown): string | null {
  if (typeof value !== 'string') return null
  return isSafeImageUrl(value) ? value.trim() : null
}

/**
 * O conteúdo do `localStorage` é entrada hostil como qualquer outra: pode ter
 * sido escrito por outro script na mesma origem, então revalida o esquema.
 */
function readStored(): StoredPlaymat {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    if (raw === null) return EMPTY
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed === null) return EMPTY
    const record = parsed as Record<string, unknown>
    const art = Array.isArray(record.art) ? record.art : []
    const chosen = Array.isArray(record.chosen) ? record.chosen : []
    return {
      art: [sanitize(art[0]), sanitize(art[1])],
      chosen: [chosen[0] === true, chosen[1] === true],
    }
  } catch {
    return EMPTY
  }
}

function writeStored(state: StoredPlaymat): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
  } catch {
    // Storage bloqueado, cheio ou em modo privado: a escolha continua valendo
    // nesta sessão, só não sobrevive ao recarregar.
  }
}

export const usePlaymatStore = create<PlaymatStore>((set, get) => ({
  ...readStored(),
  tint: [[], []],

  setTint: (seat, colors) => {
    const current = get().tint[seat]
    if (current.length === colors.length && current.every((c, i) => c === colors[i])) return
    const tint: [Color[], Color[]] = [...get().tint]
    tint[seat] = colors
    set({ tint })
  },

  setArt: (seat, url) => {
    apply(set, get, seat, url, true)
  },

  suggestArt: (seat, url) => {
    apply(set, get, seat, url, false)
  },
}))

type SetState = (partial: Partial<PlaymatStore>) => void
type GetState = () => PlaymatStore

function apply(
  set: SetState,
  get: GetState,
  seat: Seat,
  url: string | null,
  decided: boolean,
): void {
  const art: [string | null, string | null] = [...get().art]
  const chosen: [boolean, boolean] = [...get().chosen]
  art[seat] = url === null ? null : sanitize(url)
  if (decided) chosen[seat] = true
  set({ art, chosen })
  writeStored({ art, chosen })
}
