import clsx from 'clsx'
import type { CSSProperties, ReactNode } from 'react'
import { Card } from '../card/Card'
import { CARD_SIZES } from '../card/cardVisuals'
import type { CardView, ObjectId } from '../../types/protocol'
import { CardBack } from './CardBack'
import { cssVars } from './boardVisuals'

export type CardSlotSize = 'micro' | 'small' | 'medium' | 'large'

export interface CardSlotProps {
  id: ObjectId
  card: CardView | null
  size: CardSlotSize
  /** Multiplicador local sobre `--board-scale`, para afinar uma fileira só. */
  scale?: number
  /** Largura CSS explícita; ignora a escala da mesa. Para uso fora do tabuleiro. */
  width?: string
  revealed?: boolean
  /** Reserva a folga lateral da carta virada; quem gira 90° é `card.css`. */
  tapped?: boolean
  className?: string
  style?: CSSProperties
  overlay?: ReactNode
  title?: string
}

/**
 * Moldura de tamanho previsível em volta de `Card`.
 *
 * Quem manda no tamanho é o slot: ele é um container query (ver `App.css`), e
 * `card.css` faz a carta derivar tudo — tipografia, molduras, badges — de
 * `100cqw`. Assim uma carta encolhe inteira, em vez de virar um retângulo com
 * letra grande demais. É também este nó que as setas de alvo e a camada de
 * efeitos encontram, por `data-card-id` / `data-fx-card`.
 */
export function CardSlot({
  id,
  card,
  size,
  scale = 1,
  width,
  revealed = true,
  tapped = false,
  className,
  style,
  overlay,
  title,
}: CardSlotProps) {
  const natural = CARD_SIZES[size].width
  const ring = ringFor(card)

  return (
    <div
      data-card-id={id}
      data-fx-card={id}
      data-tapped={tapped ? 'true' : 'false'}
      title={title}
      className={clsx('board-card-slot', className)}
      style={cssVars(
        {
          '--slot-natural': width ?? `${natural}px`,
          '--slot-factor':
            width === undefined ? `calc(var(--board-scale, 1) * ${scale})` : '1',
        },
        style,
      )}
    >
      {/* Girar aqui somaria com a rotação que `card.css` já aplica na carta
          virada, e o resultado eram 180° — carta de cabeça para baixo. */}
      <div className="board-card-slot__pivot">
        {card === null ? (
          <CardBack seed={String(id)} />
        ) : (
          <Card card={card} size={size} revealed={revealed} />
        )}
        {ring !== null ? (
          <span
            aria-hidden="true"
            className="board-card-slot__ring"
            style={{ boxShadow: ring }}
          />
        ) : null}
        {card !== null && card.summoningSick ? (
          <span aria-hidden="true" className="board-card-slot__sick" />
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
