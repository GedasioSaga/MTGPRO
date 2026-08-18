import clsx from 'clsx'
import { useEffect, useRef, useState } from 'react'
import { emitShake } from './fxBus'
import { clamp, clampDuration, lerp, useFxMotion } from './fxMotion'

export interface LifeTickerProps {
  value: number
  className?: string
  /** Perda a partir daqui treme o numero e sacode a cena. */
  shakeThreshold?: number
}

const BASE_MS = 150
const MS_PER_POINT = 42
/** Quanto tempo a cor de tendencia fica depois que a contagem termina. */
const TREND_HOLD_MS = 900
const TREMOR_X_PX = 4.5
const TREMOR_Y_PX = 3

type Trend = 'up' | 'down' | 'flat'

/**
 * Contador de vida do HUD. Ele CONTA ate o novo total em vez de trocar o
 * numero: é assim que o espectador percebe o tamanho do golpe sem ler o log.
 * Dano grande ainda treme o numero e pede tremor de cena pelo `fxBus`.
 */
export function LifeTicker({ value, className, shakeThreshold = 3 }: LifeTickerProps) {
  const { reduced } = useFxMotion()
  const [shown, setShown] = useState(value)
  const [trend, setTrend] = useState<Trend>('flat')
  const shownRef = useRef(value)
  const nodeRef = useRef<HTMLSpanElement | null>(null)

  useEffect(() => {
    const node = nodeRef.current
    const from = shownRef.current
    const delta = value - from
    if (delta === 0) return

    const magnitude = Math.abs(delta)
    setTrend(delta > 0 ? 'up' : 'down')
    if (delta < 0 && magnitude >= shakeThreshold) emitShake(clamp(magnitude / 12, 0.2, 1))

    let holdTimer = 0
    const settle = (): void => {
      holdTimer = window.setTimeout(() => setTrend('flat'), TREND_HOLD_MS)
    }

    if (reduced) {
      shownRef.current = value
      setShown(value)
      settle()
      return () => window.clearTimeout(holdTimer)
    }

    const duration = clampDuration(BASE_MS + magnitude * MS_PER_POINT)
    const tremor = magnitude >= shakeThreshold ? clamp(magnitude / 9, 0.3, 1) : 0
    const start = performance.now()
    let frame = 0

    const step = (): void => {
      const t = clamp((performance.now() - start) / duration, 0, 1)
      const eased = 1 - (1 - t) ** 3
      const current = Math.round(lerp(from, value, eased))
      shownRef.current = current
      setShown(current)

      if (node !== null && tremor > 0) {
        const decay = (1 - t) * (1 - t)
        const dx = Math.sin(t * Math.PI * 11) * TREMOR_X_PX * tremor * decay
        const dy = Math.cos(t * Math.PI * 14) * TREMOR_Y_PX * tremor * decay
        node.style.transform = `translate3d(${dx.toFixed(2)}px, ${dy.toFixed(2)}px, 0)`
      }

      if (t < 1) {
        frame = requestAnimationFrame(step)
        return
      }
      shownRef.current = value
      if (node !== null) node.style.transform = ''
      settle()
    }

    frame = requestAnimationFrame(step)
    return () => {
      cancelAnimationFrame(frame)
      window.clearTimeout(holdTimer)
      if (node !== null) node.style.transform = ''
    }
  }, [value, reduced, shakeThreshold])

  return (
    <span ref={nodeRef} className={clsx('fx-life-ticker', className)} data-trend={trend}>
      {shown}
    </span>
  )
}
