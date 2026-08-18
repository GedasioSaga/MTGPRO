import { MATCH_SOCKET_URL } from './api'
import { isGameOver, isServerFrame } from '../types/protocol'
import type {
  ClientFrame,
  DoneFrame,
  EventsFrame,
  ServerFrame,
  StartFrame,
} from '../types/protocol'

export type MatchSocketStatus =
  | { kind: 'connecting'; attempt: number }
  | { kind: 'live' }
  /** O servidor não respondeu; o replay de demonstração assumiu a partida. */
  | { kind: 'offline' }
  | { kind: 'closed' }
  | { kind: 'error'; message: string }

export interface MatchSocketOptions {
  url?: string
  /** Tentativas de conexão antes de cair para o replay local. */
  maxAttempts?: number
  onFrame: (frame: ServerFrame) => void
  onStatus: (status: MatchSocketStatus) => void
}

const BASE_RETRY_MS = 400
const MAX_RETRY_MS = 4000

/**
 * Cliente do `/ws/match`. Reconecta com backoff exponencial e, se o servidor
 * não subir, troca para o replay de demonstração — a mesa nunca fica vazia,
 * que é o pior estado possível para um simulador que se assiste.
 */
export class MatchSocket {
  private readonly url: string
  private readonly maxAttempts: number
  private readonly onFrame: (frame: ServerFrame) => void
  private readonly onStatus: (status: MatchSocketStatus) => void

  private socket: WebSocket | null = null
  private retryTimer: ReturnType<typeof setTimeout> | null = null
  private attempt = 0
  private disposed = false
  private offline = false
  private finished = false
  private startFrame: StartFrame | null = null

  constructor(options: MatchSocketOptions) {
    this.url = options.url ?? MATCH_SOCKET_URL
    this.maxAttempts = options.maxAttempts ?? 3
    this.onFrame = options.onFrame
    this.onStatus = options.onStatus
  }

  /** Abre a conexão e pede a partida. Reentrante: reinicia o que estiver ativo. */
  start(frame: StartFrame): void {
    this.startFrame = frame
    this.attempt = 0
    this.finished = false
    this.offline = false
    this.teardownSocket()
    this.open()
  }

  send(frame: ClientFrame): void {
    // No modo offline o ritmo é local: a store pausa/avança sozinha.
    if (this.offline) return
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(frame))
    }
  }

  close(): void {
    this.disposed = true
    this.clearRetry()
    this.teardownSocket()
    this.onStatus({ kind: 'closed' })
  }

  private open(): void {
    if (this.disposed) return
    this.onStatus({ kind: 'connecting', attempt: this.attempt })

    let socket: WebSocket
    try {
      socket = new WebSocket(this.url)
    } catch {
      this.scheduleRetry()
      return
    }
    this.socket = socket

    socket.onopen = () => {
      if (this.disposed) return
      this.attempt = 0
      this.onStatus({ kind: 'live' })
      if (this.startFrame !== null) {
        socket.send(JSON.stringify(this.startFrame))
      }
    }

    socket.onmessage = (message: MessageEvent<string>) => {
      if (this.disposed) return
      const frame = parseFrame(message.data)
      if (frame === null) return
      if (frame.type === 'done') this.finished = true
      this.onFrame(frame)
    }

    socket.onerror = () => {
      // `onclose` sempre segue; o retry mora lá para não duplicar.
    }

    socket.onclose = () => {
      if (this.disposed || this.socket !== socket) return
      this.socket = null
      if (this.finished) {
        this.onStatus({ kind: 'closed' })
        return
      }
      this.scheduleRetry()
    }
  }

  private scheduleRetry(): void {
    this.attempt += 1
    if (this.attempt >= this.maxAttempts) {
      void this.fallbackToDemo()
      return
    }
    const delay = Math.min(BASE_RETRY_MS * 2 ** (this.attempt - 1), MAX_RETRY_MS)
    this.clearRetry()
    this.retryTimer = setTimeout(() => {
      this.retryTimer = null
      this.open()
    }, delay)
  }

  private async fallbackToDemo(): Promise<void> {
    if (this.disposed || this.offline) return
    this.offline = true
    try {
      const { demoMatch } = await import('../mock/demoMatch')
      if (this.disposed) return
      this.onStatus({ kind: 'offline' })
      this.onFrame(demoMatch.init)
      for (const frame of demoMatch.frames) this.onFrame(frame)
      this.onFrame(demoDoneFrame(demoMatch.frames))
      this.finished = true
    } catch {
      if (this.disposed) return
      this.onStatus({
        kind: 'error',
        message: 'Servidor indisponível e replay de demonstração ausente.',
      })
    }
  }

  private clearRetry(): void {
    if (this.retryTimer !== null) {
      clearTimeout(this.retryTimer)
      this.retryTimer = null
    }
  }

  private teardownSocket(): void {
    const socket = this.socket
    this.socket = null
    if (socket === null) return
    socket.onopen = null
    socket.onmessage = null
    socket.onerror = null
    socket.onclose = null
    if (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING) {
      socket.close()
    }
  }
}

function parseFrame(raw: string): ServerFrame | null {
  try {
    const parsed: unknown = JSON.parse(raw)
    return isServerFrame(parsed) ? parsed : null
  } catch {
    return null
  }
}

/** O replay guarda só `init` + `events`; o `done` é derivado do último estado. */
function demoDoneFrame(frames: readonly EventsFrame[]): DoneFrame {
  const last = frames.at(-1)
  const outcome = last !== undefined && isGameOver(last.view.outcome) ? last.view.outcome : 'Draw'
  return { type: 'done', outcome, turns: last?.view.turn ?? 0, durationMs: 0 }
}
