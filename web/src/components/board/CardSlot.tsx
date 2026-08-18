import clsx from 'clsx'
import type { CSSProperties, ReactNode } from 'react'
import { Card } from '../card/Card'
import { CARD_SIZES } from '../card/cardVisuals'
import type { CardSize } from '../card/cardVisuals'
import type { CardView, ObjectId } from '../../types/protocol'
import { CardBack } from './CardBack'
import { cssVars } from './boardVisuals'

export type CardSlotSize = CardSize

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
  /** A criatura não sobrevive ao combate em curso. */
  doomed?: boolean
  /** Dano que ainda vai chegar nela neste combate. */
  incoming?: number
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
  doomed = false,
  incoming = 0,
  className,
  style,
  overlay,
  title,
}: CardSlotProps) {
  const natural = CARD_SIZES[size].width
  const ring = ringFor(card, doomed)

  return (
    <div
      data-card-id={id}
      data-fx-card={id}
      data-tapped={tapped ? 'true' : 'false'}
      data-doomed={doomed ? 'true' : 'false'}
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
          <Card card={card} size={size} revealed={revealed} doomed={doomed} incoming={incoming} />
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

/**
 * Realce de combate: LUZ, não contorno.
 *
 * O `0 0 0 2px` de antes era literalmente o que um navegador desenha ao redor
 * de um campo em foco, e a mesa inteira lia como formulário selecionado. Aqui
 * sobra só o halo difuso na cor do papel, com um traço de temperatura para
 * separar quem bate de quem apara: a carta parece ACESA, e quem está atacando
 * também está LEVANTADA da mesa (a elevação mora em `App.css`).
 *
 * Morrer continua sendo vermelho e continua vencendo os dois — mas em brilho,
 * não em moldura; a caveira já diz o resto.
 */
function ringFor(card: CardView | null, doomed: boolean): string | null {
  if (card === null) return null
  if (doomed) {
    return '0 0 30px 9px rgba(255, 84, 60, 0.34)'
  }
  if (card.attacking !== null) {
    return '0 0 34px 11px rgba(255, 226, 184, 0.24)'
  }
  if (card.blocking.length > 0) {
    return '0 0 30px 9px rgba(214, 232, 255, 0.2)'
  }
  if (card.isLegalTarget) {
    return '0 0 24px 6px rgba(240, 206, 140, 0.18)'
  }
  return null
}
