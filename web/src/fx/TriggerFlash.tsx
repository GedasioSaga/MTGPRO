import { motion } from 'motion/react'
import type { CSSProperties } from 'react'
import { FxNode } from './FxNode'
import { FX_TONES } from './fxColors'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Contorno dourado na fonte do gatilho, com o texto da habilidade acima. */
export function TriggerFlash({ rect, text, durationMs }: FxOf<'triggerFlash'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)
  const base = -(rect.h / 2) - 22

  return (
    <FxNode at={{ x: rect.x, y: rect.y }}>
      <motion.div
        className="fx-flash"
        aria-hidden
        style={{ width: rect.w, height: rect.h }}
        initial={{ opacity: 0 }}
        animate={
          reduced
            ? { opacity: [0, 1, 0.55, 0] }
            : { opacity: [0, 1, 0.55, 0], scale: [1.05, 1, 1, 1] }
        }
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.2, 0.6, 1] }}
      />
      <motion.div
        className="fx-tag"
        style={{ '--fx-tag-bg': FX_TONES.trigger } as CSSProperties}
        initial={{ opacity: 0, y: base }}
        animate={
          reduced
            ? { opacity: [0, 1, 1, 0], y: [base, base, base, base] }
            : { opacity: [0, 1, 1, 0], y: [base + 10, base, base, base - 8] }
        }
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.2, 0.7, 1] }}
      >
        <span className="fx-tag__text">{text}</span>
      </motion.div>
    </FxNode>
  )
}
