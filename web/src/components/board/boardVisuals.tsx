import { useEffect, useState } from 'react'
import type { CSSProperties } from 'react'
import { seatAccent } from '../../design/tokens'
import type { SeatSwatch } from '../../design/tokens'

/**
 * Cor de identidade do assento. A paleta mora em `design/tokens` — aqui só se
 * traduz o número do jogador, que o motor emite, para o índice do assento. Duas
 * tabelas de cor divergiam no primeiro ajuste: a mesa acendia num âmbar e o HUD
 * noutro.
 */
export function seatSwatch(player: number): SeatSwatch {
  return seatAccent[player % seatAccent.length]
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
 * Teto de ampliação. A mesa era travada em 1, então num monitor 4K ela ficava
 * do tamanho de 1920 no meio da tela com tarja preta em volta — o jogo parecia
 * pequeno num display grande, que é o oposto do esperado. O teto existe para a
 * carta não virar cartaz em tela ultrawide.
 */
const MAX_BOARD_SCALE = 1.6

/**
 * Fator único de escala da mesa. A mesa é desenhada para 1920x1080 e acompanha
 * o viewport nos dois sentidos: encolhe até 0.62 para 1280x800 caber sem
 * rolagem, e cresce até 1.6 para ocupar um monitor maior. Uma variável só, em
 * vez de cada componente inventar o próprio breakpoint.
 */
export function boardScaleFor(width: number, height: number): number {
  const byWidth = width / BOARD_REFERENCE.width
  const byHeight = height / BOARD_REFERENCE.height
  const raw = Math.min(byWidth, byHeight)
  return Math.max(MIN_BOARD_SCALE, Math.min(MAX_BOARD_SCALE, Number(raw.toFixed(3))))
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
