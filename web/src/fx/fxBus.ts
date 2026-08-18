/**
 * Canal minimo para efeitos globais disparados de fora da fila de eventos —
 * hoje so o tremor de tela, que o `LifeTicker` do HUD precisa acionar sem
 * conhecer o coreografo.
 */
type ShakeListener = (intensity: number) => void

const listeners = new Set<ShakeListener>()

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
