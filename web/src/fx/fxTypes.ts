import type { ObjectId, PlayerId } from '../types/protocol'

/** Ponto em coordenadas de viewport (o overlay e `position: fixed`). */
export interface FxPoint {
  x: number
  y: number
}

/** Retangulo com origem no CENTRO — casa direto com `translate3d`. */
export interface FxRect {
  x: number
  y: number
  w: number
  h: number
}

export interface FxBase {
  id: string
  startedAt: number
  durationMs: number
  /** Efeitos persistentes ficam ate `clear()` — o fim de jogo nao expira sozinho. */
  persistent?: boolean
}

export type FxTone = 'damage' | 'life' | 'victory' | 'defeat'

export type FxBody =
  | { kind: 'turnBanner'; turn: number; player: PlayerId; playerName: string; accent: string }
  | { kind: 'stepPulse'; rect: FxRect; label: string }
  | { kind: 'castBeam'; from: FxPoint; to: FxPoint; color: string; name: string }
  | { kind: 'resolveBurst'; at: FxPoint; color: string }
  | { kind: 'counterShatter'; at: FxPoint }
  | { kind: 'attackLunge'; card: ObjectId; rect: FxRect; toward: FxPoint }
  | { kind: 'blockClash'; at: FxPoint }
  | {
      kind: 'damageNumber'
      at: FxPoint
      amount: number
      tone: 'damage' | 'life'
      lethal: boolean
      /** Quem levou: carta apanha faisca de impacto, jogador leva onda de choque. */
      subject: 'card' | 'player'
    }
  | { kind: 'lifePulse'; at: FxPoint; tone: 'damage' | 'life' }
  | { kind: 'deathDissolve'; rect: FxRect; tone: 'death' | 'exile' }
  | { kind: 'tokenSpawn'; rect: FxRect }
  | { kind: 'counterPop'; at: FxPoint; label: string; delta: number }
  | { kind: 'triggerFlash'; rect: FxRect; text: string }
  | { kind: 'screenShake'; intensity: number }
  | { kind: 'vignette'; tone: FxTone; intensity: number }
  | {
      kind: 'gameOver'
      title: string
      subtitle: string
      scoreboard: { name: string; life: number; won: boolean }[]
      turns: number
    }

export type FxEffect = FxBase & FxBody

/** `Omit` precisa ser distributivo, senao a uniao colapsa nas chaves comuns. */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never

/** O que os tradutores produzem; o motor carimba `id` e `startedAt`. */
export type FxSpec = DistributiveOmit<FxEffect, 'id' | 'startedAt'>

export type FxKind = FxBody['kind']

/** Acesso a uma variante especifica sem repetir `Extract` em cada componente. */
export type FxOf<K extends FxKind> = Extract<FxEffect, { kind: K }>

/**
 * Prioridade decide quem sobrevive quando a rajada estoura o teto de
 * animacoes simultaneas. Maior = mais importante.
 */
export const FX_PRIORITY: Record<FxKind, number> = {
  gameOver: 100,
  turnBanner: 90,
  damageNumber: 80,
  deathDissolve: 75,
  counterShatter: 70,
  castBeam: 65,
  attackLunge: 60,
  blockClash: 58,
  resolveBurst: 55,
  triggerFlash: 50,
  tokenSpawn: 48,
  lifePulse: 40,
  counterPop: 35,
  vignette: 30,
  screenShake: 25,
  stepPulse: 10,
}
