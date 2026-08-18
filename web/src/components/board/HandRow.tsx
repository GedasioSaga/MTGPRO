import clsx from 'clsx'
import { motion, useReducedMotion } from 'motion/react'
import { CARD_SIZES } from '../card/cardVisuals'
import type { CardView, ObjectId, PlayerId } from '../../types/protocol'
import { CardSlot } from './CardSlot'
import type { CardSlotSize } from './CardSlot'

export interface HandRowProps {
  player: PlayerId
  ids: ObjectId[]
  cards: Record<ObjectId, CardView>
  side: 'top' | 'bottom'
  /** Usado quando a mão está redigida e só conhecemos o tamanho. */
  count: number
}

/**
 * Mão em leque. O leque não é enfeite: com dez cartas empilhadas retas não dá
 * para contar quantas são, e a curva devolve a contagem num relance.
 */
export function HandRow({ player, ids, cards, side, count }: HandRowProps) {
  const reduceMotion = useReducedMotion()
  const isOwn = side === 'bottom'
  const slots: (ObjectId | null)[] =
    ids.length > 0 ? ids : Array.from({ length: count }, () => null)

  const size: CardSlotSize = isOwn ? 'medium' : 'small'
  const scale = isOwn ? 1 : 0.62

  if (slots.length === 0) {
    return (
      <div
        className="board-hand board-hand--empty"
        data-fx-hand={player}
        aria-label="Mão vazia"
      />
    )
  }

  const total = slots.length
  const center = (total - 1) / 2
  const spread = Math.min(4.2, 26 / Math.max(1, total))
  const overlapRatio = total <= 6 ? 0.16 : Math.min(0.62, 0.16 + (total - 6) * 0.07)
  const step = `calc(${CARD_SIZES[size].width}px * var(--board-scale, 1) * ${scale} * -${overlapRatio})`
  const lift = isOwn ? 5 : 3

  return (
    <div
      className={clsx('board-hand', isOwn ? 'board-hand--own' : 'board-hand--foe')}
      data-fx-hand={player}
      aria-label={
        isOwn ? `Mão de baixo: ${total} cartas` : `Mão de cima: ${total} cartas`
      }
    >
      {slots.map((id, index) => {
        const offset = index - center
        const rotate = (isOwn ? 1 : -1) * offset * spread
        const drop = Math.abs(offset) ** 1.8 * lift
        const translateY = isOwn ? drop : -drop
        const card = id === null ? null : (cards[id] ?? null)

        return (
          <motion.div
            key={id ?? `back-${index}`}
            className="board-hand__card"
            style={{
              zIndex: index,
              marginLeft: index === 0 ? 0 : step,
              transformOrigin: isOwn ? 'bottom center' : 'top center',
            }}
            initial={false}
            animate={{ rotate, y: translateY }}
            whileHover={
              reduceMotion
                ? undefined
                : { y: translateY + (isOwn ? -30 : 30), rotate: 0, scale: 1.06, zIndex: 60 }
            }
            transition={
              reduceMotion ? { duration: 0 } : { type: 'spring', stiffness: 340, damping: 30 }
            }
          >
            <CardSlot
              id={id ?? -1 - index}
              card={card}
              size={size}
              scale={scale}
              revealed={card !== null && card.name !== null}
              title={card?.name ?? undefined}
            />
          </motion.div>
        )
      })}
    </div>
  )
}
