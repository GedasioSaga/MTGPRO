import { motion } from 'motion/react'
import type { CSSProperties } from 'react'
import { FxNode } from './FxNode'
import { FX_TONES } from './fxColors'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Halo no assento do jogador quando a vida muda — o numero quem conta e o ticker. */
export function LifePulse({ at, tone, durationMs }: FxOf<'lifePulse'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)
  const color = tone === 'life' ? FX_TONES.life : FX_TONES.damage

  return (
    <FxNode at={at}>
      <motion.div
        className="fx-life-pulse"
        aria-hidden
        style={{ '--fx-pulse-color': color } as CSSProperties}
        initial={{ opacity: 0 }}
        animate={reduced ? { opacity: [0, 0.9, 0] } : { opacity: [0, 0.9, 0], scale: [0.6, 1, 1.28] }}
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.3, 1] }}
      />
    </FxNode>
  )
}
