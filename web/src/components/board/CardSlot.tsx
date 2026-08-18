import clsx from 'clsx'
import type { CSSProperties, ReactNode } from 'react'
import { Card } from '../card/Card'
import type { CardView, ObjectId } from '../../types/protocol'
import { CardBack } from './CardBack'

export type CardSlotSize = 'micro' | 'small' | 'medium' | 'large'

export interface CardSlotProps {
  id: ObjectId
  card: CardView | null
  width: string
  size: CardSlotSize
  revealed?: boolean
  /** Gira 90° como uma carta virada de verdade, sem alargar a fileira. */
  tapped?: boolean
  className?: string
  style?: CSSProperties
  overlay?: ReactNode
}

/**
 * Moldura de tamanho fixo em volta de `Card`. A geometria mora aqui porque o
 * tabuleiro precisa de fileiras previsíveis e porque as setas de alvo procuram
 * o elemento por `data-card-id`.
 */
export function CardSlot({
  id,
  card,
  width,
  size,
  revealed = true,
  tapped = false,
  className,
  style,
  overlay,
}: CardSlotProps) {
  const ring = ringFor(card)

  return (
    <div
      data-card-id={id}
      className={clsx('board-card-slot relative shrink-0', className)}
      style={{ width, aspectRatio: '63 / 88', ...style }}
    >
      <div
        className={clsx(
          'relative size-full origin-center transition-transform duration-300 ease-out',
          tapped && 'rotate-90 scale-[0.72]',
        )}
      >
        {card === null ? (
          <CardBack seed={String(id)} />
        ) : (
          <Card card={card} size={size} revealed={revealed} className="size-full" />
        )}
        {ring !== null ? (
          <span
            aria-hidden="true"
            className="pointer-events-none absolute -inset-px rounded-[7px]"
            style={{ boxShadow: ring }}
          />
        ) : null}
        {card !== null && card.summoningSick ? (
          <span
            aria-hidden="true"
            className="pointer-events-none absolute inset-0 rounded-[7px] bg-sky-200/8 mix-blend-screen"
          />
        ) : null}
      </div>
      {overlay}
    </div>
  )
}

/** Ataque, bloqueio e alvo legal precisam ser lidos num relance, sem hover. */
function ringFor(card: CardView | null): string | null {
  if (card === null) return null
  if (card.attacking !== null) {
    return '0 0 0 2px rgba(242,104,94,0.85), 0 0 22px rgba(242,104,94,0.45)'
  }
  if (card.blocking.length > 0) {
    return '0 0 0 2px rgba(111,156,245,0.85), 0 0 22px rgba(111,156,245,0.4)'
  }
  if (card.isLegalTarget) {
    return '0 0 0 2px rgba(232,169,76,0.7), 0 0 18px rgba(232,169,76,0.35)'
  }
  return null
}
