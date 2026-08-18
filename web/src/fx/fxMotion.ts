import { useReducedMotion } from 'motion/react'

/**
 * Curvas unicas da camada de efeitos. Nenhuma delas passa de 1.06 no overshoot:
 * carta de Magic nao quica.
 */
type Bezier = [number, number, number, number]

export const EASE: Record<'out' | 'in' | 'inOut' | 'snap' | 'linear', Bezier> = {
  /** Entrada padrao: rapido no comeco, assenta sem quicar. */
  out: [0.16, 1, 0.3, 1],
  /** Saida padrao. */
  in: [0.5, 0, 0.85, 0],
  inOut: [0.65, 0, 0.35, 1],
  /** Overshoot minimo, so o suficiente para o olho registrar impacto. */
  snap: [0.34, 1.12, 0.64, 1],
  linear: [0, 0, 1, 1],
}

/** Toda duracao da camada vive entre 120ms e 700ms. */
export const DUR = {
  micro: 120,
  fast: 180,
  base: 260,
  slow: 420,
  long: 700,
} as const

export const MIN_DURATION_MS = 120
export const MAX_DURATION_MS = 700

export function clampDuration(ms: number): number {
  return Math.min(MAX_DURATION_MS, Math.max(MIN_DURATION_MS, Math.round(ms)))
}

/** Duracao em segundos, como o motion espera, ja limitada. */
export function seconds(ms: number): number {
  return clampDuration(ms) / 1000
}

/**
 * No modo reduzido nada se move, mas o efeito continua LEGIVEL: aparece e some
 * em opacidade, num tempo curto e fixo.
 */
export function useFxMotion(): { reduced: boolean; d: (ms: number) => number } {
  const reduced = useReducedMotion() === true
  return {
    reduced,
    d: (ms: number) => (reduced ? MIN_DURATION_MS / 1000 : seconds(ms)),
  }
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t
}
