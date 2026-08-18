import { motion } from 'motion/react'
import { FxNode } from './FxNode'
import { FX_TONES } from './fxColors'
import { clamp, EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Sinal de menos tipografico — o hifen do teclado some no meio do numero. */
const MINUS = '\u2212'

/** Numero que sobe e some. Em modo reduzido ele aparece e some sem viajar. */
export function DamageNumber({ at, amount, tone, lethal, durationMs }: FxOf<'damageNumber'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)
  const color = tone === 'life' ? FX_TONES.life : FX_TONES.damage
  const size = clamp(1.9 + Math.min(amount, 12) * 0.11, 1.9, 3.3)
  const pop = lethal ? 1.45 : 1.24

  return (
    <FxNode at={at}>
      <motion.div
        className={lethal ? 'fx-number fx-number--lethal' : 'fx-number'}
        style={{ position: 'relative', color, fontSize: `${size}rem` }}
        initial={{ opacity: 0 }}
        animate={
          reduced
            ? { opacity: [0, 1, 1, 0] }
            : { opacity: [0, 1, 1, 0], scale: [pop, 1, 1, 1], y: [8, -12, -24, -36] }
        }
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.14, 0.62, 1] }}
      >
        {tone === 'life' ? '+' : MINUS}
        {amount}
      </motion.div>
    </FxNode>
  )
}
