import { motion } from 'motion/react'
import type { CSSProperties } from 'react'
import { FX_TONES, withAlpha } from './fxColors'
import { clamp, EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Tinta nas bordas da tela. Persistente fica; efemera pulsa e sai. */
export function Vignette({ tone, intensity, durationMs, persistent }: FxOf<'vignette'>) {
  const { d } = useFxMotion()
  const peak = clamp(intensity, 0, 1)
  const style = {
    '--fx-vignette-color': withAlpha(FX_TONES[tone], 0.55),
  } as CSSProperties

  if (persistent === true) {
    return (
      <motion.div
        className="fx-vignette"
        aria-hidden
        style={style}
        initial={{ opacity: 0 }}
        animate={{ opacity: peak }}
        exit={{ opacity: 0 }}
        transition={{ duration: d(420), ease: EASE.out }}
      />
    )
  }

  return (
    <motion.div
      className="fx-vignette"
      aria-hidden
      style={style}
      initial={{ opacity: 0 }}
      animate={{ opacity: [0, peak, peak, 0] }}
      exit={{ opacity: 0 }}
      transition={{ duration: d(durationMs), ease: EASE.out, times: [0, 0.2, 0.55, 1] }}
    />
  )
}
