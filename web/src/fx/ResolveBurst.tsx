import { motion } from 'motion/react'
import type { CSSProperties } from 'react'
import { FxNode } from './FxNode'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Estouro de luz na cor da magia quando ela resolve. */
export function ResolveBurst({ at, color, durationMs }: FxOf<'resolveBurst'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)
  const vars = { '--fx-burst-color': color } as CSSProperties

  return (
    <FxNode at={at}>
      <motion.div
        className="fx-burst__ring"
        aria-hidden
        style={vars}
        initial={{ opacity: 0 }}
        animate={
          reduced ? { opacity: [0, 0.9, 0] } : { opacity: [0, 0.95, 0], scale: [0.35, 1.05, 1.5] }
        }
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.34, 1] }}
      />
      <motion.div
        className="fx-burst__core"
        aria-hidden
        style={vars}
        initial={{ opacity: 0 }}
        animate={reduced ? { opacity: [0, 1, 0] } : { opacity: [0, 1, 0], scale: [0.2, 1, 1.3] }}
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.28, 1] }}
      />
    </FxNode>
  )
}
