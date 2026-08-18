import clsx from 'clsx'
import type { ReactNode } from 'react'
import { isCreature, isLand } from '../../types/protocol'
import type { CardView, ObjectId, PlayerId } from '../../types/protocol'
import { CardSlot } from './CardSlot'
import { cssVars } from './boardVisuals'
import { EMPTY_COMBAT_PLAN } from './combatPlan'
import type { CombatPlan } from './combatPlan'

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
  /** Pareamento de combate. Fora de combate vem vazio e a fileira é uma fila só. */
  plan?: CombatPlan
}

interface LandGroup {
  key: string
  ids: ObjectId[]
}

/**
 * Metade do campo de batalha de um jogador, desenhada como duas zonas da mesa:
 * a faixa de batalha, colada na linha central (é onde o combate acontece), e a
 * faixa de terrenos na borda externa. Nenhuma das duas é desenhada com linha:
 * a diferença entre elas é de luz e de altura, não de moldura.
 */
export function BattlefieldRow({
  player,
  side,
  permanents,
  cards,
  plan = EMPTY_COMBAT_PLAN,
}: BattlefieldRowProps) {
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
        {plan.active ? (
          <CombatLanes creatures={creatures} plan={plan} />
        ) : (
          <div className="board-field__creatures">
            {creatures.map((card) => (
              <Creature key={card.id} card={card} plan={plan} />
            ))}
          </div>
        )}
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
                size="board"
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
 * Trecho da mesa, não caixa. Não desenha moldura nem rótulo: quem separa a
 * zona é a LUZ (ver `.board-zone` em `App.css`) — a faixa de batalha sobe meio
 * tom, a de terrenos afunda. O nome da zona sobrevive só como `aria-label`,
 * para leitor de tela, que é onde ele de fato faz falta.
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
            size="board"
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

/**
 * Fileira de combate: uma coluna por duelo, na ordem do plano, com largura
 * idêntica nas duas metades da mesa. É a coluna que diz quem bloqueia quem —
 * sem ela seria preciso uma linha atravessando a tela para dizer o mesmo.
 */
function CombatLanes({ creatures, plan }: { creatures: CardView[]; plan: CombatPlan }) {
  const inLane: CardView[][] = plan.lanes.map(() => [])
  const reserve: CardView[] = []
  for (const card of creatures) {
    const index = plan.laneOf.get(card.id)
    if (index === undefined) reserve.push(card)
    else inLane[index].push(card)
  }

  return (
    <div className="board-field__creatures board-field__creatures--combat">
      <div className="combat-rail">
        {plan.lanes.map((lane, index) => (
          <div
            key={lane.key}
            className={`combat-cell combat-cell--${lane.kind}`}
            style={cssVars({ '--slots': String(lane.slots) })}
          >
            {inLane[index].map((card) => (
              <Creature key={card.id} card={card} plan={plan} />
            ))}
          </div>
        ))}
      </div>
      <div className="combat-reserve">
        {reserve.map((card) => (
          <Creature key={card.id} card={card} plan={plan} />
        ))}
      </div>
    </div>
  )
}

function Creature({ card, plan }: { card: CardView; plan: CombatPlan }) {
  const incoming = plan.incoming.get(card.id) ?? 0
  return (
    <CardSlot
      id={card.id}
      card={card}
      size="board"
      width={FIELD_WIDTH}
      tapped={card.tapped}
      title={card.name ?? undefined}
      className={combatClass(card)}
      doomed={plan.doomed.has(card.id)}
      incoming={incoming}
      overlay={<AttachmentBadge count={card.attachments.length} />}
    />
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
