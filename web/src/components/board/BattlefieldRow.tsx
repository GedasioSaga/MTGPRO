import clsx from 'clsx'
import { isCreature, isLand } from '../../types/protocol'
import type { CardView, ObjectId, PlayerId } from '../../types/protocol'
import { CardSlot } from './CardSlot'
import { seatAccent, withAlpha } from './boardVisuals'

const CREATURE_WIDTH = 'clamp(70px, 5.2vw, 98px)'
const LAND_WIDTH = 'clamp(52px, 3.6vw, 68px)'
const OTHER_WIDTH = 'clamp(58px, 4.1vw, 78px)'

export interface BattlefieldRowProps {
  player: PlayerId
  side: 'top' | 'bottom'
  permanents: ObjectId[]
  cards: Record<ObjectId, CardView>
}

interface LandGroup {
  key: string
  ids: ObjectId[]
}

/**
 * Metade do campo de batalha de um jogador. Criaturas ficam junto da linha
 * central (é onde o combate acontece) e terrenos ficam atrás, agrupados por
 * nome — vinte Florestas soltas viram ruído e roubam a fileira das criaturas.
 */
export function BattlefieldRow({ player, side, permanents, cards }: BattlefieldRowProps) {
  const accent = seatAccent(player)
  const creatures: CardView[] = []
  const lands: CardView[] = []
  const others: CardView[] = []

  for (const id of permanents) {
    const card = cards[id]
    if (card === undefined) continue
    // Auras e equipamentos aparecem colados no hospedeiro, não soltos.
    if (card.attachedTo !== null) continue
    if (isCreature(card)) creatures.push(card)
    else if (isLand(card)) lands.push(card)
    else others.push(card)
  }

  const landGroups = groupLands(lands)

  return (
    <div
      className={clsx(
        'flex h-full min-h-0 flex-col gap-2 px-6 py-2',
        side === 'top' ? 'flex-col-reverse' : 'flex-col',
      )}
    >
      <div className="flex min-h-0 flex-1 flex-wrap content-center items-center justify-center gap-2">
        {creatures.length === 0 ? (
          <span
            className="rounded-full border border-dashed px-4 py-1 text-[11px] tracking-[0.16em] uppercase"
            style={{ borderColor: withAlpha(accent, 0.16), color: withAlpha(accent, 0.35) }}
          >
            sem criaturas
          </span>
        ) : (
          creatures.map((card) => (
            <CardSlot
              key={card.id}
              id={card.id}
              card={card}
              width={CREATURE_WIDTH}
              size="small"
              tapped={card.tapped}
              overlay={<AttachmentBadge count={card.attachments.length} />}
            />
          ))
        )}
      </div>

      <div className="flex shrink-0 items-end gap-5">
        <div className="flex flex-wrap items-end gap-3">
          {landGroups.map((group) => (
            <LandStack key={group.key} group={group} cards={cards} />
          ))}
        </div>
        <div className="ml-auto flex flex-wrap items-end justify-end gap-1.5">
          {others.map((card) => (
            <CardSlot
              key={card.id}
              id={card.id}
              card={card}
              width={OTHER_WIDTH}
              size="micro"
              tapped={card.tapped}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

function LandStack({ group, cards }: { group: LandGroup; cards: Record<ObjectId, CardView> }) {
  return (
    <div className="relative flex items-end">
      {group.ids.map((id, index) => {
        const card = cards[id]
        if (card === undefined) return null
        return (
          <CardSlot
            key={id}
            id={id}
            card={card}
            width={LAND_WIDTH}
            size="micro"
            tapped={card.tapped}
            className={index === 0 ? undefined : '-ml-[62%]'}
            style={{ zIndex: index }}
          />
        )
      })}
      {group.ids.length > 1 ? (
        <span className="pointer-events-none absolute -top-1.5 -right-1.5 z-20 grid size-5 place-items-center rounded-full bg-[#0d101a] text-[11px] font-semibold tabular-nums text-white/80 ring-1 ring-white/20">
          {group.ids.length}
        </span>
      ) : null}
    </div>
  )
}

function AttachmentBadge({ count }: { count: number }) {
  if (count === 0) return null
  return (
    <span
      className="pointer-events-none absolute -top-1 -left-1 grid size-4.5 place-items-center rounded-full bg-violet-400/90 text-[10px] font-bold text-black/80"
      title={`${count} anexo(s)`}
    >
      {count}
    </span>
  )
}

/**
 * Agrupa por nome + estado de virado: um monte só de "Forest" mistura o que
 * já foi gasto com o que ainda produz mana, e essa é a informação que importa.
 * Carta oculta (sem nome) fica sozinha.
 */
function groupLands(lands: CardView[]): LandGroup[] {
  const groups: LandGroup[] = []
  const index = new Map<string, LandGroup>()

  for (const card of lands) {
    const key = card.name === null ? `solo:${card.id}` : `${card.name}|${card.tapped}`
    const existing = index.get(key)
    if (existing === undefined) {
      const group: LandGroup = { key, ids: [card.id] }
      index.set(key, group)
      groups.push(group)
    } else {
      existing.ids.push(card.id)
    }
  }
  return groups
}
