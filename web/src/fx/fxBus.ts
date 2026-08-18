import type { FxBurst } from './particles/particleTypes'

/**
 * Canal minimo para efeitos globais disparados de fora da fila de eventos:
 * o tremor de tela, que o `LifeTicker` do HUD precisa acionar sem conhecer o
 * coreografo, e as rajadas de particula, que o motor de efeitos publica sem
 * saber se existe um canvas montado para ouvi-las.
 */
type ShakeListener = (intensity: number) => void
type BurstListener = (burst: FxBurst) => void

const listeners = new Set<ShakeListener>()
const burstListeners = new Set<BurstListener>()

export function onShake(listener: ShakeListener): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/** `intensity` em 0..1; acima de 0.6 o tremor ja e violento. */
export function emitShake(intensity: number): void {
  const clamped = Math.min(1, Math.max(0, intensity))
  for (const listener of listeners) listener(clamped)
}

export function onBurst(listener: BurstListener): () => void {
  burstListeners.add(listener)
  return () => {
    burstListeners.delete(listener)
  }
}

/** Sem ouvinte (modo reduzido, canvas ausente) a rajada apenas se perde. */
export function emitBursts(bursts: readonly FxBurst[]): void {
  if (bursts.length === 0 || burstListeners.size === 0) return
  for (const burst of bursts) {
    for (const listener of burstListeners) listener(burst)
  }
}
