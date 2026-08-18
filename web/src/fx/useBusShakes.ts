import { useEffect, useState } from 'react'
import { onShake } from './fxBus'

export interface BusShake {
  id: number
  intensity: number
}

/** Tremor pedido pelo `fxBus` (HUD) tem duracao propria, curta. */
export const BUS_SHAKE_MS = 280

let sequence = 0

/**
 * Ponte do `fxBus` para a camada: o HUD pede tremor sem conhecer o coreografo.
 * Cada pedido vive `BUS_SHAKE_MS` e some sozinho — nada acumula na lista.
 */
export function useBusShakes(): BusShake[] {
  const [shakes, setShakes] = useState<BusShake[]>([])

  useEffect(() => {
    const timers = new Set<number>()
    const off = onShake((intensity) => {
      sequence += 1
      const id = sequence
      setShakes((prev) => [...prev, { id, intensity }])
      const timer = window.setTimeout(() => {
        timers.delete(timer)
        setShakes((prev) => prev.filter((shake) => shake.id !== id))
      }, BUS_SHAKE_MS + 60)
      timers.add(timer)
    })
    return () => {
      off()
      for (const timer of timers) window.clearTimeout(timer)
    }
  }, [])

  return shakes
}
