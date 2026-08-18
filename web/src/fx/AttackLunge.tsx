import { motion } from 'motion/react'
import { FxNode } from './FxNode'
import { clamp, EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Investida do atacante: o fantasma da carta avanca na direcao do defensor. */
export function AttackLunge({ rect, toward, durationMs }: FxOf<'attackLunge'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)

  const dx = toward.x - rect.x
  const dy = toward.y - rect.y
  const length = Math.hypot(dx, dy) || 1
  const travel = clamp(length * 0.22, 18, 64)
  const ux = (dx / length) * travel
  const uy = (dy / length) * travel

  return (
    <FxNode at={{ x: rect.x, y: rect.y }}>
      <motion.div
        className="fx-lunge"
        aria-hidden
        style={{ width: rect.w, height: rect.h }}
        initial={{ opacity: 0 }}
        animate={
          reduced
            ? { opacity: [0, 1, 0.8, 0] }
            : { opacity: [0, 1, 0.8, 0], x: [0, ux, ux * 0.3, 0], y: [0, uy, uy * 0.3, 0] }
        }
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.3, 0.62, 1] }}
      />
    </FxNode>
  )
}
