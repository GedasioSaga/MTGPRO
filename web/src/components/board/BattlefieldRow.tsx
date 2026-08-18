import clsx from 'clsx'
import type { ReactNode } from 'react'
import { isCreature, isLand } from '../../types/protocol'
import type { CardView, ObjectId, PlayerId } from '../../types/protocol'
import { CardSlot } from './CardSlot'

/*
 * Largura das cartas do campo. Vem de variável da mesa (`App.css`) em vez de
 * `CARD_SIZES`, porque o que manda no tamanho aqui é a ALTURA da faixa: quem
 * está em jogo tem que dominar quem está na mão, e essa proporção é decisão de
 * composição, não do catálogo de tamanhos.
 */
const FIELD_WIDTH = 'var(--card-field)'
const LAND_WIDTH = 'var(--card-land)'
const OTHER_WIDTH = 'var(--card-other)'

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
 * Metade do campo de batalha de um jogador, desenhada como duas zonas da mesa:
 * a faixa de batalha, colada na linha central (é onde o combate acontece), e a
 * faixa de terrenos na borda externa. As duas existem mesmo vazias — o jogador
 * precisa ver ONDE as coisas vão cair antes de elas caírem.
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
      <Zone kind="battle" label="campo de batalha">
        <div className="board-field__creatures">
          {creatures.map((card) => (
            <CardSlot
              key={card.id}
              id={card.id}
              card={card}
              size="medium"
              width={FIELD_WIDTH}
              tapped={card.tapped}
              title={card.name ?? undefined}
              className={combatClass(card)}
              overlay={<AttachmentBadge count={card.attachments.length} />}
            />
          ))}
        </div>
      </Zone>

      <Zone kind="lands" label="terrenos">
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
                width={OTHER_WIDTH}
                tapped={card.tapped}
                title={card.name ?? undefined}
              />
            ))}
          </div>
        </div>
      </Zone>
    </div>
  )
}

/**
 * Área impressa na mesa. O rótulo é serigrafia de playmat: fica sempre lá,
 * apagado, e some sob as cartas quando a zona enche.
 */
function Zone({
  kind,
  label,
  children,
}: {
  kind: 'battle' | 'lands'
  label: string
  children: ReactNode
}) {
  return (
    <section className="board-zone" data-zone={kind} aria-label={label}>
      <span className="board-zone__label" aria-hidden="true">
        {label}
      </span>
      <div className="board-zone__inner">{children}</div>
    </section>
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
            width={LAND_WIDTH}
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
