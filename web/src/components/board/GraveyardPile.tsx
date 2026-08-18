import type { CardView, ObjectId } from '../../types/protocol'
import { GraveyardIcon } from './BoardIcons'
import { ZonePile } from './ZonePile'

export interface GraveyardPileProps {
  ids: ObjectId[]
  cards: Record<ObjectId, CardView>
  count: number
  side: 'top' | 'bottom'
}

/** Cemitério do jogador: face para cima, ordem do motor, topo visível. */
export function GraveyardPile({ ids, cards, count, side }: GraveyardPileProps) {
  return (
    <ZonePile
      label="cemitério"
      ids={ids}
      cards={cards}
      count={count}
      tone="grave"
      side={side}
      icon={<GraveyardIcon className="size-5" />}
    />
  )
}
