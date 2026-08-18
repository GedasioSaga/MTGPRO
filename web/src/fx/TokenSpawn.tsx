import { motion } from 'motion/react'
import { FxNode } from './FxNode'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Moldura de luz que se fecha sobre a ficha recem-criada. */
export function TokenSpawn({ rect, durationMs }: FxOf<'tokenSpawn'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)

  return (
    <FxNode at={{ x: rect.x, y: rect.y }}>
      <motion.div
        className="fx-spawn"
        aria-hidden
        style={{ width: rect.w, height: rect.h }}
        initial={{ opacity: 0 }}
        animate={
          reduced
            ? { opacity: [0, 1, 0.9, 0] }
            : { opacity: [0, 1, 0.9, 0], scale: [1.14, 1, 1, 1] }
        }
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.28, 0.6, 1] }}
      />
    </FxNode>
  )
}
