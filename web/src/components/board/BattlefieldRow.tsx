import { isCreature, isLand } from '../../types/protocol'
import type { CardView, ObjectId, PlayerId } from '../../types/protocol'
import { CardSlot } from './CardSlot'
import { cssVars } from './boardVisuals'
import { MatZone } from './MatZone'
import type { Seat } from './playmatZones'
import { EMPTY_COMBAT_PLAN } from './combatPlan'
import type { CombatPlan } from './combatPlan'

/*
 * Larguras de fallback. Quem manda de verdade no tamanho é a ALTURA da zona
 * impressa (ver `App.css`): a carta ocupa a faixa que o mat reservou para ela,
 * então mat mais baixo dá carta menor sem ninguém tocar em constante.
 */
const FIELD_WIDTH = 'var(--card-field)'
const LAND_WIDTH = 'var(--card-land)'
const OTHER_WIDTH = 'var(--card-other)'

export interface BattlefieldRowProps {
  player: PlayerId
  /** Assento cujo mat recebe estas cartas. */
  seat: Seat
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
 * O que este jogador tem em jogo, pousado nas duas zonas impressas do mat dele:
 * criaturas dentro de CAMPO DE BATALHA, terrenos e demais permanentes dentro de
 * TERRENOS.
 *
 * A fileira não decide mais onde fica: o mat decide. Aqui só se decide o que
 * vai em cada zona e em que ordem — inclusive a ordem das raias de combate, que
 * é a mesma nas duas metades da mesa e é o que faz o atacante cair embaixo de
 * quem o bloqueou.
 */
export function BattlefieldRow({
  player,
  seat,
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
    <>
      <MatZone seat={seat} zone="battlefield" className="mat-zone--battlefield" pinned>
        {plan.active ? (
          <CombatLanes creatures={creatures} plan={plan} />
        ) : (
          <div className="mat-row" data-seat={player}>
            {creatures.map((card) => (
              <Creature key={card.id} card={card} plan={plan} />
            ))}
          </div>
        )}
      </MatZone>

      <MatZone seat={seat} zone="lands" className="mat-zone--lands">
        <div className="mat-row mat-row--lands" data-seat={player}>
          {landGroups.map((group) => (
            <LandStack key={group.key} group={group} cards={cards} />
          ))}
          {others.map((card) => (
            <CardSlot
              key={card.id}
              id={card.id}
              card={card}
              size="board"
              width={OTHER_WIDTH}
              tapped={card.tapped}
              title={card.name ?? undefined}
              className="mat-other"
            />
          ))}
        </div>
      </MatZone>
    </>
  )
}

function LandStack({ group, cards }: { group: LandGroup; cards: Record<ObjectId, CardView> }) {
  return (
    <div className="mat-lands__stack">
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
            className={index === 0 ? undefined : 'mat-lands__overlap'}
            style={{ zIndex: index }}
          />
        )
      })}
      {group.ids.length > 1 ? (
        <span className="mat-lands__count">{group.ids.length}</span>
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
    <div className="mat-row mat-row--combat">
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
    <span className="mat-attached" title={`${count} anexo(s)`}>
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
