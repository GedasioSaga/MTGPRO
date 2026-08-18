import { hashString } from '../design/color'
import type { PlayerId } from '../types/protocol'

/** Bits do `ColorSet` do motor: WUBRG, nesta ordem (mana.rs `Color::index`). */
const COLOR_BITS: { bit: number; hex: string }[] = [
  { bit: 1 << 0, hex: '#f4e6c3' },
  { bit: 1 << 1, hex: '#4a9fe0' },
  { bit: 1 << 2, hex: '#9b7fb0' },
  { bit: 1 << 3, hex: '#e8543f' },
  { bit: 1 << 4, hex: '#4fb573' },
]

const COLORLESS = '#b9c2cc'
const GOLD = '#e6c34f'

/**
 * Cor luminosa de um `ColorSet`. Multicolorido vira ouro, como a moldura da
 * carta — misturar canais na mao daria um marrom sujo.
 */
export function colorSetHex(colors: number): string {
  const present = COLOR_BITS.filter((c) => (colors & c.bit) !== 0)
  if (present.length === 0) return COLORLESS
  if (present.length === 1) return present[0]!.hex
  return GOLD
}

/** Cor de identidade de cada jogador — usada em faixa de turno e halo de vida. */
const PLAYER_ACCENTS = ['#4fa8ff', '#ff7a4f', '#8f7bff', '#4fd6a8']

export function playerAccent(player: PlayerId): string {
  return PLAYER_ACCENTS[player % PLAYER_ACCENTS.length] ?? PLAYER_ACCENTS[0]!
}

export const FX_TONES = {
  damage: '#ff4d4d',
  life: '#5ce6a0',
  exile: '#c58bff',
  death: '#ff8a4c',
  counter: '#6fd5ff',
  trigger: '#ffd76a',
  victory: '#ffd76a',
  defeat: '#ff5a5a',
} as const

export function proceduralHue(seed: string): number {
  return hashString(seed) % 360
}

