import { useEffect, useState } from 'react'
import type { CSSProperties } from 'react'

/** Cor de identidade de cada assento. P0 é quente (embaixo), P1 é frio (em cima). */
const SEAT_ACCENT: readonly string[] = ['#e8a94c', '#6f9cf5']

export function seatAccent(player: number): string {
  return SEAT_ACCENT[player % SEAT_ACCENT.length]
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

/** Iniciais para o retrato: "Bot Aggro" -> "BA". */
export function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (parts.length === 0) return '??'
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase()
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase()
}

/**
 * Variáveis CSS em `style` sem `any`: `CSSProperties` não declara custom
 * properties, então a conversão acontece num lugar só em vez de espalhada.
 */
export function cssVars(
  vars: Record<string, string | number>,
  base?: CSSProperties,
): CSSProperties {
  return { ...base, ...vars } as CSSProperties
}

/** Largura de referência da mesa; abaixo disso tudo encolhe junto. */
const BOARD_REFERENCE = { width: 1920, height: 1080 } as const
const MIN_BOARD_SCALE = 0.62

/**
 * Fator único de escala da mesa. A mesa é desenhada para 1920x1080 e encolhe
 * proporcionalmente — é o que garante que 1280x800 continue cabendo sem rolagem
 * e sem que cada componente invente o próprio breakpoint.
 */
export function boardScaleFor(width: number, height: number): number {
  const byWidth = width / BOARD_REFERENCE.width
  const byHeight = height / BOARD_REFERENCE.height
  const raw = Math.min(byWidth, byHeight)
  return Math.max(MIN_BOARD_SCALE, Math.min(1, Number(raw.toFixed(3))))
}

/** Acompanha o viewport e devolve `boardScaleFor` já reativo. */
export function useBoardScale(): number {
  const [scale, setScale] = useState(() =>
    typeof window === 'undefined'
      ? 1
      : boardScaleFor(window.innerWidth, window.innerHeight),
  )

  useEffect(() => {
    const update = (): void => {
      setScale(boardScaleFor(window.innerWidth, window.innerHeight))
    }
    update()
    window.addEventListener('resize', update)
    return () => window.removeEventListener('resize', update)
  }, [])

  return scale
}
