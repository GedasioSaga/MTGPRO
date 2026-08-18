import type { ColorSet } from '../../types/protocol'

/** Cor de identidade de cada assento. P0 é quente (embaixo), P1 é frio (em cima). */
export const SEAT_ACCENT: readonly string[] = ['#e8a94c', '#6f9cf5']
export const SEAT_ACCENT_SOFT: readonly string[] = [
  'rgba(232, 169, 76, 0.18)',
  'rgba(111, 156, 245, 0.18)',
]

export function seatAccent(player: number): string {
  return SEAT_ACCENT[player % SEAT_ACCENT.length]
}

export function seatAccentSoft(player: number): string {
  return SEAT_ACCENT_SOFT[player % SEAT_ACCENT_SOFT.length]
}

/** WUBRG na ordem dos bits de `ColorSet`. */
export const MANA_HEX: readonly string[] = [
  '#f5edd6',
  '#a8d0f0',
  '#a99ba6',
  '#efa08a',
  '#a5cfa0',
]
export const COLORLESS_HEX = '#c3c6cf'

/** FNV-1a. Determinístico entre sessões — a mesma carta cai sempre no mesmo tom. */
export function hashString(text: string): number {
  let hash = 0x811c9dc5
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i)
    hash = Math.imul(hash, 0x01000193)
  }
  return hash >>> 0
}

/**
 * Fundo procedural para quando não há arte. Deriva do nome (matiz e ângulo) e
 * das cores da carta (paleta), então duas cartas diferentes nunca ficam iguais
 * e uma carta vermelha nunca fica azul.
 */
export function proceduralArt(name: string, colors: ColorSet): string {
  const hash = hashString(name)
  const angle = 100 + (hash % 140)
  const palette: string[] = []
  for (let bit = 0; bit < MANA_HEX.length; bit += 1) {
    if ((colors & (1 << bit)) !== 0) palette.push(MANA_HEX[bit])
  }
  if (palette.length === 0) palette.push(COLORLESS_HEX)

  const tint = palette[hash % palette.length]
  const second = palette[(hash >> 5) % palette.length]
  const lift = 8 + ((hash >> 11) % 22)

  return [
    `radial-gradient(120% 80% at ${lift}% 12%, ${withAlpha(tint, 0.55)} 0%, transparent 62%)`,
    `radial-gradient(90% 70% at ${100 - lift}% 92%, ${withAlpha(second, 0.42)} 0%, transparent 58%)`,
    `linear-gradient(${angle}deg, #171b26 0%, #232838 48%, #12151f 100%)`,
  ].join(', ')
}

export function withAlpha(hex: string, alpha: number): string {
  const raw = hex.replace('#', '')
  const value = Number.parseInt(raw, 16)
  const r = (value >> 16) & 0xff
  const g = (value >> 8) & 0xff
  const b = value & 0xff
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

/** Iniciais para o retrato: "Bot Aggro" -> "BA". */
export function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (parts.length === 0) return '??'
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase()
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase()
}
