import { motion } from 'motion/react'
import { FxNode } from './FxNode'
import { EASE, useFxMotion } from './fxMotion'
import type { FxOf } from './fxTypes'

/** Duas laminas que se cruzam no ponto de encontro entre atacante e bloqueador. */
export function BlockClash({ at, durationMs }: FxOf<'blockClash'>) {
  const { reduced, d } = useFxMotion()
  const dur = d(durationMs)
  const blades = [
    { angle: 36, from: -34 },
    { angle: -36, from: 34 },
  ]

  return (
    <FxNode at={at}>
      {blades.map((blade) => (
        <motion.div
          key={blade.angle}
          className="fx-clash__blade"
          aria-hidden
          style={{ rotate: blade.angle }}
          initial={{ opacity: 0 }}
          animate={
            reduced
              ? { opacity: [0, 1, 0] }
              : { opacity: [0, 1, 0], x: [blade.from, 0, 0], scaleX: [0.5, 1, 1] }
          }
          transition={{ duration: dur, ease: EASE.out, times: [0, 0.34, 1] }}
        />
      ))}
      <motion.div
        className="fx-clash__spark"
        aria-hidden
        initial={{ opacity: 0 }}
        animate={reduced ? { opacity: [0, 1, 0] } : { opacity: [0, 1, 0], scale: [0.3, 1.15, 1.45] }}
        transition={{ duration: dur, ease: EASE.out, times: [0, 0.3, 1] }}
      />
    </FxNode>
  )
}
