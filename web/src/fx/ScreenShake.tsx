import { useEffect } from 'react'
import { shakeTarget } from './fxAnchors'
import { clamp, clampDuration, useFxMotion } from './fxMotion'

interface ScreenShakeProps {
  /** 0..1 — acima de 0.6 o tremor ja e violento. */
  intensity: number
  durationMs: number
}

const MAX_AMPLITUDE_PX = 11

/**
 * Tremor da cena. Nao desenha nada: escreve `transform` direto no alvo, quadro
 * a quadro, porque o no sacudido pertence a mesa e nao a esta arvore.
 */
export function ScreenShake({ intensity, durationMs }: ScreenShakeProps) {
  const { reduced } = useFxMotion()

  useEffect(() => {
    if (reduced) return
    const target = shakeTarget()
    if (target === null) return

    const dur = clampDuration(durationMs)
    const amplitude = clamp(intensity, 0, 1) * MAX_AMPLITUDE_PX
    const start = performance.now()
    let frame = 0

    const step = (): void => {
      const t = clamp((performance.now() - start) / dur, 0, 1)
      const decay = (1 - t) * (1 - t)
      const dx = Math.sin(t * Math.PI * 9) * amplitude * decay
      const dy = Math.cos(t * Math.PI * 13) * amplitude * 0.6 * decay
      target.style.transform = `translate3d(${dx.toFixed(2)}px, ${dy.toFixed(2)}px, 0)`
      if (t < 1) {
        frame = requestAnimationFrame(step)
        return
      }
      target.style.transform = ''
    }

    frame = requestAnimationFrame(step)
    return () => {
      cancelAnimationFrame(frame)
      target.style.transform = ''
    }
  }, [intensity, durationMs, reduced])

  return null
}
