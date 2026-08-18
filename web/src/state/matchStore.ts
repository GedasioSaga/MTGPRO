import { create } from 'zustand'
import { MatchSocket } from '../net/matchSocket'
import type { MatchSocketStatus } from '../net/matchSocket'
import { suggestedDurationMs } from '../types/protocol'
import type {
  CardView,
  DoneFrame,
  GameView,
  MatchEvent,
  ObjectId,
  Outcome,
  ServerFrame,
} from '../types/protocol'

export type ConnectionState = 'idle' | 'connecting' | 'live' | 'done' | 'error'

/**
 * Um evento e, quando ele fecha um frame do servidor, o `GameView` que passa a
 * valer depois dele. Aplicar a view só no fim do frame é o que permite animar:
 * se ela entrasse na chegada do frame, o tabuleiro pularia para o resultado
 * antes de qualquer animação rodar.
 */
interface QueuedEvent {
  event: MatchEvent
  viewAfter: GameView | null
}

export interface MatchStore {
  view: GameView | null
  cards: Record<ObjectId, CardView>
  playerNames: string[]
  seed: number | null
  connection: ConnectionState
  /** Verdadeiro quando o replay de demonstração assumiu no lugar do servidor. */
  offline: boolean
  error: string | null
  playing: boolean
  speed: number
  currentEvent: MatchEvent | null
  eventQueue: MatchEvent[]
  log: string[]
  outcome: Outcome
  turnsPlayed: number
  durationMs: number

  start: (deckA: string, deckB: string, seed: number, speed: number) => void
  pause: () => void
  resume: () => void
  setSpeed: (speed: number) => void
  step: () => void
  teardown: () => void
}

const LOG_CAP = 400
const MIN_FRAME_MS = 40
const MIN_SPEED = 0.25
const MAX_SPEED = 8

// Estado de transporte: fica fora da store porque não é renderizável e trocá-lo
// não deve custar um re-render.
let socket: MatchSocket | null = null
let timer: ReturnType<typeof setTimeout> | null = null
let queue: QueuedEvent[] = []
let pendingDone: DoneFrame | null = null

export const useMatchStore = create<MatchStore>((set, get) => ({
  view: null,
  cards: {},
  playerNames: [],
  seed: null,
  connection: 'idle',
  offline: false,
  error: null,
  playing: false,
  speed: 1,
  currentEvent: null,
  eventQueue: [],
  log: [],
  outcome: 'Ongoing',
  turnsPlayed: 0,
  durationMs: 0,

  start: (deckA, deckB, seed, speed) => {
    clearTimer()
    queue = []
    pendingDone = null
    socket?.close()

    set({
      view: null,
      cards: {},
      playerNames: [],
      seed,
      connection: 'connecting',
      offline: false,
      error: null,
      playing: true,
      speed: clampSpeed(speed),
      currentEvent: null,
      eventQueue: [],
      log: [],
      outcome: 'Ongoing',
      turnsPlayed: 0,
      durationMs: 0,
    })

    socket = new MatchSocket({
      onFrame: (frame) => handleFrame(frame, set, get),
      onStatus: (status) => handleStatus(status, set),
    })
    socket.start({ type: 'start', deckA, deckB, seed, speed: clampSpeed(speed) })
  },

  pause: () => {
    if (!get().playing) return
    clearTimer()
    set({ playing: false })
    socket?.send({ type: 'pause' })
  },

  resume: () => {
    if (get().playing || get().connection === 'done') return
    set({ playing: true })
    socket?.send({ type: 'resume' })
    kick(set, get)
  },

  setSpeed: (speed) => {
    const next = clampSpeed(speed)
    if (next === get().speed) return
    clearTimer()
    set({ speed: next })
    kick(set, get)
  },

  step: () => {
    clearTimer()
    if (get().playing) set({ playing: false })
    socket?.send({ type: 'step' })
    consumeOne(set, get)
  },

  teardown: () => {
    clearTimer()
    socket?.close()
    socket = null
    queue = []
    pendingDone = null
  },
}))

// ---------------------------------------------------------------------------
// Frames do servidor
// ---------------------------------------------------------------------------

type SetState = (partial: Partial<MatchStore>) => void
type GetState = () => MatchStore

function handleFrame(frame: ServerFrame, set: SetState, get: GetState): void {
  switch (frame.type) {
    case 'init':
      set({
        playerNames: frame.players,
        seed: frame.seed,
        ...viewPatch(frame.view, get().log),
      })
      break

    case 'events': {
      if (frame.events.length === 0) {
        set(viewPatch(frame.view, get().log))
        break
      }
      const added: QueuedEvent[] = frame.events.map((event, index) => ({
        event,
        viewAfter: index === frame.events.length - 1 ? frame.view : null,
      }))
      queue = queue.concat(added)
      set({ eventQueue: queue.map((q) => q.event) })
      break
    }

    case 'done':
      pendingDone = frame
      break
  }
  kick(set, get)
}

function handleStatus(status: MatchSocketStatus, set: SetState): void {
  switch (status.kind) {
    case 'connecting':
      set({ connection: 'connecting' })
      break
    case 'live':
      set({ connection: 'live', offline: false, error: null })
      break
    case 'offline':
      set({ connection: 'live', offline: true, error: null })
      break
    case 'error':
      set({ connection: 'error', error: status.message, playing: false })
      break
    case 'closed':
      break
  }
}

// ---------------------------------------------------------------------------
// Ritmo da fila
// ---------------------------------------------------------------------------

/** Retoma o consumo se houver o que consumir e nada agendado. */
function kick(set: SetState, get: GetState): void {
  if (timer !== null) return
  if (!get().playing) return
  if (queue.length === 0) {
    finalizeIfDone(set)
    return
  }
  timer = setTimeout(() => tick(set, get), MIN_FRAME_MS)
}

function tick(set: SetState, get: GetState): void {
  timer = null
  if (!get().playing) return
  const consumed = consumeOne(set, get)
  if (consumed === null) {
    finalizeIfDone(set)
    return
  }
  const hold = Math.max(MIN_FRAME_MS, suggestedDurationMs(consumed) / get().speed)
  timer = setTimeout(() => tick(set, get), hold)
}

function consumeOne(set: SetState, get: GetState): MatchEvent | null {
  const head = queue.shift()
  if (head === undefined) {
    finalizeIfDone(set)
    return null
  }

  const patch: Partial<MatchStore> = {
    currentEvent: head.event,
    eventQueue: queue.map((q) => q.event),
  }

  let log = get().log
  if (head.event.type === 'log') log = appendLog(log, [head.event.text])

  if (head.viewAfter !== null) {
    Object.assign(patch, viewPatch(head.viewAfter, log))
  } else if (log !== get().log) {
    patch.log = log
  }

  set(patch)
  if (queue.length === 0) finalizeIfDone(set)
  return head.event
}

function finalizeIfDone(set: SetState): void {
  if (pendingDone === null || queue.length > 0) return
  const done = pendingDone
  pendingDone = null
  clearTimer()
  set({
    connection: 'done',
    playing: false,
    outcome: done.outcome,
    turnsPlayed: done.turns,
    durationMs: done.durationMs,
  })
}

function clearTimer(): void {
  if (timer !== null) {
    clearTimeout(timer)
    timer = null
  }
}

// ---------------------------------------------------------------------------
// Aplicação de estado
// ---------------------------------------------------------------------------

function viewPatch(view: GameView, log: string[]): Partial<MatchStore> {
  const cards: Record<ObjectId, CardView> = {}
  for (const card of view.cards) cards[card.id] = card
  return {
    view,
    cards,
    outcome: view.outcome,
    turnsPlayed: view.turn,
    log: appendLog(log, view.logTail),
  }
}

/**
 * `logTail` é o rabo do log completo, então frames consecutivos se sobrepõem.
 * Casamos o maior sufixo do que já temos com o prefixo da cauda para não
 * duplicar linha nem perder linha quando o servidor pula um frame.
 */
function appendLog(existing: string[], tail: string[]): string[] {
  if (tail.length === 0) return existing
  const maxOverlap = Math.min(existing.length, tail.length)
  for (let overlap = maxOverlap; overlap > 0; overlap -= 1) {
    let matches = true
    for (let i = 0; i < overlap; i += 1) {
      if (existing[existing.length - overlap + i] !== tail[i]) {
        matches = false
        break
      }
    }
    if (matches) {
      if (overlap === tail.length) return existing
      return cap(existing.concat(tail.slice(overlap)))
    }
  }
  return cap(existing.concat(tail))
}

function cap(entries: string[]): string[] {
  return entries.length > LOG_CAP ? entries.slice(entries.length - LOG_CAP) : entries
}

function clampSpeed(speed: number): number {
  if (!Number.isFinite(speed)) return 1
  return Math.min(MAX_SPEED, Math.max(MIN_SPEED, speed))
}
