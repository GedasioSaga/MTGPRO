import { clsx } from 'clsx'
import { useEffect, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { Divider } from '../ui/Divider'
import { useMatchStore } from '../../state/matchStore'
import { outcomeWinner } from '../../types/protocol'
import type { CardView, GameView, MatchEvent, ObjectId, PlayerId } from '../../types/protocol'
import { seatAccent } from '../../design/tokens'

const LOG_CAP = 300
const BOTTOM_THRESHOLD_PX = 28

type IconKind =
  | 'turn'
  | 'move'
  | 'draw'
  | 'cast'
  | 'trigger'
  | 'attack'
  | 'block'
  | 'damage'
  | 'life'
  | 'counter'
  | 'token'
  | 'death'
  | 'exile'
  | 'lost'
  | 'over'
  | 'note'

interface LogEntry {
  seq: number
  turn: number
  player: PlayerId | null
  icon: IconKind
  text: string
}

/**
 * `cardMoved` cobre pousar terreno, mas tambem os saltos internos entre mao,
 * pilha e cemiterio que `spellCast`/`died`/`destroyed` ja narram — mostrar os
 * dois duplicaria a linha. So o pouso de terreno (mao -> campo direto) fica.
 */
function isLandPlay(event: Extract<MatchEvent, { type: 'cardMoved' }>): boolean {
  return event.from === 'Hand' && event.to === 'Battlefield'
}

const REASON_LABEL: Record<string, string> = {
  ZeroLife: 'sem vida',
  DrewFromEmptyLibrary: 'comprou de biblioteca vazia',
  PoisonCounters: 'veneno',
  Effect: 'efeito',
  Concede: 'desistência',
}

function describeEvent(
  event: MatchEvent,
  view: GameView | null,
  cards: Record<ObjectId, CardView>,
): { icon: IconKind; player: PlayerId | null; text: string } | null {
  const cardName = (id: ObjectId): string => cards[id]?.name ?? 'uma carta'
  const playerName = (id: PlayerId): string => view?.players[id]?.name ?? `Jogador ${id + 1}`
  const controllerOf = (id: ObjectId): PlayerId | null => cards[id]?.controller ?? null

  switch (event.type) {
    case 'turnStart':
      return { icon: 'turn', player: event.player, text: `Turno ${event.turn} — vez de ${playerName(event.player)}` }

    case 'cardMoved':
      if (!isLandPlay(event)) return null
      return { icon: 'move', player: event.owner, text: `${playerName(event.owner)} joga ${cardName(event.card)}` }

    case 'cardDrawn':
      return { icon: 'draw', player: event.player, text: `${playerName(event.player)} compra uma carta` }

    case 'spellCast': {
      const target = event.targets[0]
      const aim = target !== undefined ? ` mirando ${cardName(target)}` : ''
      return { icon: 'cast', player: event.player, text: `${playerName(event.player)} conjura ${cardName(event.card)}${aim}` }
    }

    case 'spellCountered':
      return { icon: 'death', player: controllerOf(event.card), text: `${cardName(event.card)} é anulada` }

    case 'abilityTriggered':
      return { icon: 'trigger', player: controllerOf(event.source), text: `${cardName(event.source)}: ${event.text}` }

    case 'attackersDeclared': {
      if (event.attackers.length === 0) return null
      const player = controllerOf(event.attackers[0][0])
      const names = event.attackers.map(([id]) => cardName(id))
      const who = names.length <= 3 ? names.join(', ') : `${names.length} criaturas`
      return { icon: 'attack', player, text: `${playerName(player ?? 0)} ataca com ${who}` }
    }

    case 'blockersDeclared': {
      const blockerIds = event.blocks.flatMap(([, ids]) => ids)
      if (blockerIds.length === 0) return null
      const player = controllerOf(blockerIds[0])
      const names = blockerIds.map((id) => cardName(id))
      const who = names.length <= 3 ? names.join(', ') : `${names.length} criaturas`
      return { icon: 'block', player, text: `${playerName(player ?? 0)} bloqueia com ${who}` }
    }

    case 'damageDealt':
      if (event.amount <= 0) return null
      return {
        icon: 'damage',
        player: controllerOf(event.source),
        text: `${cardName(event.source)} causa ${event.amount} de dano a ${cardName(event.target)}${event.lethal ? ' (letal)' : ''}`,
      }

    case 'damageToPlayer':
      if (event.amount <= 0) return null
      return {
        icon: 'damage',
        player: event.player,
        text: `${cardName(event.source)} causa ${event.amount} de dano a ${playerName(event.player)}`,
      }

    case 'lifeChanged':
      if (event.delta <= 0) return null // perda ja narrada por `damageToPlayer`
      return { icon: 'life', player: event.player, text: `${playerName(event.player)} ganha ${event.delta} de vida (total ${event.total})` }

    case 'countersChanged':
      if (event.delta === 0) return null
      return {
        icon: 'counter',
        player: controllerOf(event.card),
        text: `${cardName(event.card)} ${event.delta > 0 ? 'ganha' : 'perde'} ${Math.abs(event.delta)} marcador(es) de ${event.kind}`,
      }

    case 'tokenCreated':
      return { icon: 'token', player: event.controller, text: `${playerName(event.controller)} cria um token: ${cardName(event.card)}` }

    case 'died':
      return { icon: 'death', player: controllerOf(event.card), text: `${cardName(event.card)} morre` }

    case 'destroyed':
      return { icon: 'death', player: controllerOf(event.card), text: `${cardName(event.card)} é destruída` }

    case 'exiled':
      return { icon: 'exile', player: controllerOf(event.card), text: `${cardName(event.card)} é exilada` }

    case 'playerLost':
      return { icon: 'lost', player: event.player, text: `${playerName(event.player)} perde a partida (${REASON_LABEL[event.reason] ?? event.reason})` }

    case 'gameOver': {
      const winner = outcomeWinner(event.outcome)
      return {
        icon: 'over',
        player: winner,
        text: winner !== null ? `${playerName(winner)} vence a partida!` : 'A partida termina em empate.',
      }
    }

    case 'log':
      return { icon: 'note', player: null, text: event.text }

    default:
      return null
  }
}

/**
 * Log rolavel da partida. Constroi seu proprio historico observando
 * `currentEvent` — o unico jeito de saber turno, jogador e tipo de cada
 * linha, ja que `matchStore.log` so guarda texto plano.
 */
export function GameLog() {
  const currentEvent = useMatchStore((s) => s.currentEvent)
  const view = useMatchStore((s) => s.view)
  const cards = useMatchStore((s) => s.cards)
  const seed = useMatchStore((s) => s.seed)

  const [entries, setEntries] = useState<LogEntry[]>([])
  const seedRef = useRef(seed)
  const lastEventRef = useRef<MatchEvent | null>(null)
  const seqRef = useRef(0)

  useEffect(() => {
    if (seedRef.current === seed) return
    seedRef.current = seed
    lastEventRef.current = null
    seqRef.current = 0
    setEntries([])
  }, [seed])

  useEffect(() => {
    if (currentEvent === null || currentEvent === lastEventRef.current) return
    lastEventRef.current = currentEvent
    const described = describeEvent(currentEvent, view, cards)
    if (described === null) return
    seqRef.current += 1
    const entry: LogEntry = { seq: seqRef.current, turn: view?.turn ?? 0, ...described }
    setEntries((list) => (list.length >= LOG_CAP ? [...list.slice(1), entry] : [...list, entry]))
  }, [currentEvent, view, cards])

  const scrollRef = useRef<HTMLDivElement>(null)
  const [atBottom, setAtBottom] = useState(true)

  useEffect(() => {
    if (!atBottom) return
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [entries, atBottom])

  function handleScroll(): void {
    const el = scrollRef.current
    if (!el) return
    setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < BOTTOM_THRESHOLD_PX)
  }

  function jumpToEnd(): void {
    const el = scrollRef.current
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
    setAtBottom(true)
  }

  let lastTurn = -1

  return (
    <div className="relative flex min-h-0 flex-col">
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-3 py-2"
      >
        {entries.length === 0 ? (
          <p className="text-hud py-6 text-center text-ink-faint">A partida ainda não começou.</p>
        ) : null}
        {entries.map((entry) => {
          const showDivider = entry.turn !== lastTurn
          lastTurn = entry.turn
          return (
            <div key={entry.seq} className="contents">
              {showDivider ? <Divider tone="hair" label={`Turno ${entry.turn}`} className="my-1.5" /> : null}
              <LogRow entry={entry} />
            </div>
          )
        })}
      </div>

      {!atBottom && entries.length > 0 ? (
        <button
          type="button"
          onClick={jumpToEnd}
          className={clsx(
            'glass absolute bottom-2 left-1/2 -translate-x-1/2 rounded-full px-3 py-1',
            'text-hud caps cursor-pointer text-ink-strong hover:brightness-125',
          )}
        >
          ↓ ir para o fim
        </button>
      ) : null}
    </div>
  )
}

function LogRow({ entry }: { entry: LogEntry }) {
  const accent = entry.player !== null ? seatAccent[entry.player].core : undefined
  return (
    <div
      className="flex items-start gap-2 rounded-sm border-l-2 py-0.5 pr-1 pl-2 text-[12.5px] leading-snug text-ink"
      style={{ borderColor: accent ?? 'var(--color-edge)' }}
    >
      <span className="mt-[1px] shrink-0 text-ink-muted" style={{ color: accent }} aria-hidden="true">
        <LogIcon kind={entry.icon} />
      </span>
      <span className="min-w-0 break-words">{entry.text}</span>
    </div>
  )
}

function LogIcon({ kind }: { kind: IconKind }) {
  const Icon = ICONS[kind]
  return <Icon />
}

function Svg({ children }: { children: ReactNode }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className="size-3.5" aria-hidden="true">
      {children}
    </svg>
  )
}

const ICONS: Record<IconKind, () => ReactNode> = {
  turn: () => (
    <Svg>
      <path d="M6 3v18M6 4.5h11l-2.5 3.5L17 11.5H6" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
    </Svg>
  ),
  move: () => (
    <Svg>
      <path d="M12 3v13M7 12l5 5 5-5M5 20.5h14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </Svg>
  ),
  draw: () => (
    <Svg>
      <rect x="6" y="4" width="12" height="16" rx="1.5" stroke="currentColor" strokeWidth="1.5" />
      <path d="M9 8.5h6M9 12h6M9 15.5h3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
    </Svg>
  ),
  cast: () => (
    <Svg>
      <path d="M12 3.5 14 9l5.5 1-4 3.7L16.5 19 12 16l-4.5 3 1-5.3-4-3.7L9 9l3-5.5Z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    </Svg>
  ),
  trigger: () => (
    <Svg>
      <path d="M13 3 6 13.5h5.2L11 21 18 10.5h-5.2L13 3Z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    </Svg>
  ),
  attack: () => (
    <Svg>
      <path d="M5 19 15 9M13 5.5 18.5 5.5 18.5 11M9 15l-1 4 4-1" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </Svg>
  ),
  block: () => (
    <Svg>
      <path d="M12 3.5 19 6v6c0 4.5-3 7-7 8.5-4-1.5-7-4-7-8.5V6l7-2.5Z" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
    </Svg>
  ),
  damage: () => (
    <Svg>
      <path d="M13.2 3s-5.4 5.4-5.4 9.7a4.4 4.4 0 0 0 8.8 0c0-1.7-1-3-1-3 .3 2-1 3-2 3s-1.6-1-1-2.4c.6-1.4 1-3.2.6-7.3Z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    </Svg>
  ),
  life: () => (
    <Svg>
      <path d="M12 20s-7-4.4-7-9.8A4.2 4.2 0 0 1 12 7a4.2 4.2 0 0 1 7 3.2C19 15.6 12 20 12 20Z" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
    </Svg>
  ),
  counter: () => (
    <Svg>
      <circle cx="12" cy="12" r="7.2" stroke="currentColor" strokeWidth="1.5" />
      <path d="M12 8.3v7.4M8.3 12h7.4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </Svg>
  ),
  token: () => (
    <Svg>
      <circle cx="12" cy="12" r="7.2" stroke="currentColor" strokeWidth="1.5" strokeDasharray="2.4 2.4" />
    </Svg>
  ),
  death: () => (
    <Svg>
      <circle cx="12" cy="11" r="7" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="9.3" cy="10.5" r="1.1" fill="currentColor" />
      <circle cx="14.7" cy="10.5" r="1.1" fill="currentColor" />
      <path d="M9.5 14.5h5M9 18l1-2M15 18l-1-2" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
    </Svg>
  ),
  exile: () => (
    <Svg>
      <circle cx="12" cy="12" r="7.5" stroke="currentColor" strokeWidth="1.5" />
      <path d="M12 7.5v9M7.5 12h9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" transform="rotate(45 12 12)" />
    </Svg>
  ),
  lost: () => (
    <Svg>
      <path d="M5 5l14 14M19 5 5 19" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    </Svg>
  ),
  over: () => (
    <Svg>
      <path d="M7 4h10v3.5a5 5 0 0 1-5 5 5 5 0 0 1-5-5V4Z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
      <path d="M12 12.5V17M9 20.5h6M5 5.5H3.5A2 2 0 0 0 5.3 8M19 5.5h1.5A2 2 0 0 1 18.7 8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </Svg>
  ),
  note: () => (
    <Svg>
      <circle cx="12" cy="12" r="1.3" fill="currentColor" />
    </Svg>
  ),
}
