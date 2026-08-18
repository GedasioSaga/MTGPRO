import { motion } from 'motion/react'
import { FxNode } from './FxNode'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Batida discreta na trilha de passos: marca o ritmo sem roubar a cena. */
export function StepPulse({ rect, label, durationMs }: FxOf<'stepPulse'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)

  return (
    <FxNode at={{ x: rect.x, y: rect.y }}>
      <motion.div
        className="fx-step-pulse"
        aria-hidden
        style={{ width: rect.w, height: rect.h }}
        initial={{ opacity: 0 }}
        animate={
          reduced ? { opacity: [0, 0.85, 0] } : { opacity: [0, 0.85, 0], scaleX: [0.94, 1, 1] }
        }
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.3, 1] }}
      />
      <motion.span
        className="fx-step-pulse__label"
        style={{ y: -(rect.h / 2) - 16 }}
        initial={{ opacity: 0 }}
        animate={{ opacity: [0, 1, 0] }}
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.3, 1] }}
      >
        {label}
      </motion.span>
    </FxNode>
  )
}
