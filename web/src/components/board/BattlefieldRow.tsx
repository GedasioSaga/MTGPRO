import clsx from 'clsx'
import { isCreature, isLand } from '../../types/protocol'
import type { CardView, ObjectId, PlayerId } from '../../types/protocol'
import { CardSlot } from './CardSlot'

const LAND_SCALE = 0.78
const OTHER_SCALE = 0.82

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
        'board-field',
        side === 'top' ? 'board-field--top' : 'board-field--bottom',
      )}
      data-seat={player}
    >
      {/* Campo sem criaturas não vira cartaz de vazio: a fileira não é
          renderizada, e a linha de combate recua absorvendo o espaço. */}
      <div className="board-field__creatures">
        {creatures.map((card) => (
          <CardSlot
            key={card.id}
            id={card.id}
            card={card}
            size="small"
            tapped={card.tapped}
            title={card.name ?? undefined}
            className={combatClass(card)}
            overlay={<AttachmentBadge count={card.attachments.length} />}
          />
        ))}
      </div>

      <div className="board-field__back">
        <div className="board-field__lands">
          {landGroups.map((group) => (
            <LandStack key={group.key} group={group} cards={cards} />
          ))}
        </div>
        <div className="board-field__others">
          {others.map((card) => (
            <CardSlot
              key={card.id}
              id={card.id}
              card={card}
              size="small"
              scale={OTHER_SCALE}
              tapped={card.tapped}
              title={card.name ?? undefined}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

function LandStack({ group, cards }: { group: LandGroup; cards: Record<ObjectId, CardView> }) {
  return (
    <div className="board-lands__stack">
      {group.ids.map((id, index) => {
        const card = cards[id]
        if (card === undefined) return null
        return (
          <CardSlot
            key={id}
            id={id}
            card={card}
            size="small"
            scale={LAND_SCALE}
            tapped={card.tapped}
            title={card.name ?? undefined}
            className={index === 0 ? undefined : 'board-lands__overlap'}
            style={{ zIndex: index }}
          />
        )
      })}
      {group.ids.length > 1 ? (
        <span className="board-lands__count">{group.ids.length}</span>
      ) : null}
    </div>
  )
}

/** Marca a criatura que está em combate: o realce visual sai daqui (App.css). */
function combatClass(card: CardView): string {
  if (card.attacking !== null) return 'board-creature board-creature--attacking'
  if (card.blocking.length > 0) return 'board-creature board-creature--blocking'
  return 'board-creature'
}

function AttachmentBadge({ count }: { count: number }) {
  if (count === 0) return null
  return (
    <span className="board-field__attached" title={`${count} anexo(s)`}>
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
