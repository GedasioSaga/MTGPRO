import type { CardView, ObjectId } from '../../types/protocol'
import { ExileIcon } from './BoardIcons'
import { ZonePile } from './ZonePile'

export interface ExilePileProps {
  ids: ObjectId[]
  cards: Record<ObjectId, CardView>
  count: number
  side: 'top' | 'bottom'
}

/** Exílio: mesma pilha do cemitério, tom frio — daqui a carta não costuma voltar. */
export function ExilePile({ ids, cards, count, side }: ExilePileProps) {
  return (
    <ZonePile
      label="exílio"
      ids={ids}
      cards={cards}
      count={count}
      tone="exile"
      side={side}
      icon={<ExileIcon className="size-5" />}
    />
  )
}
