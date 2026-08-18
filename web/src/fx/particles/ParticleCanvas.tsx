import { useEffect, useRef } from 'react'
import { useReducedMotion } from 'motion/react'
import { onBurst } from '../fxBus'
import { createParticleField } from './particleField'

/**
 * Camada de particulas: UM canvas para a tela inteira, montado uma unica vez
 * junto da `FxLayer`.
 *
 * Em `prefers-reduced-motion` o canvas nem chega ao DOM — a partida continua
 * legivel pelos efeitos estaticos que a camada CSS ja desenha, e nenhum quadro
 * e agendado.
 */
export function ParticleCanvas() {
  const reduced = useReducedMotion() === true
  const canvasRef = useRef<HTMLCanvasElement | null>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (reduced || canvas === null) return

    const field = createParticleField(canvas)
    if (field === null) return

    const offBurst = onBurst(field.emit)
    window.addEventListener('resize', field.resize)
    return () => {
      offBurst()
      window.removeEventListener('resize', field.resize)
      field.dispose()
    }
  }, [reduced])

  if (reduced) return null
  return <canvas ref={canvasRef} className="fx-particles" aria-hidden />
}
