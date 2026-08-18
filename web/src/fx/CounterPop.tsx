import { motion } from 'motion/react'
import type { CSSProperties } from 'react'
import { FxNode } from './FxNode'
import { FX_TONES } from './fxColors'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

const MINUS = '\u2212'

/** Pilula de marcador ganhando ou perdendo carga sobre a carta. */
export function CounterPop({ at, label, delta, durationMs }: FxOf<'counterPop'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)
  const positive = delta > 0
  const bg = positive ? FX_TONES.counter : FX_TONES.damage

  return (
    <FxNode at={at}>
      <motion.div
        className="fx-pop"
        style={{ '--fx-pop-bg': bg } as CSSProperties}
        initial={{ opacity: 0 }}
        animate={
          reduced
            ? { opacity: [0, 1, 1, 0] }
            : { opacity: [0, 1, 1, 0], scale: [0.72, 1.05, 1, 1], y: [4, -6, -14, -22] }
        }
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.26, 0.62, 1] }}
      >
        <span>
          {positive ? '+' : MINUS}
          {Math.abs(delta)}
        </span>
        <span className="fx-pop__label">{label}</span>
      </motion.div>
    </FxNode>
  )
}
