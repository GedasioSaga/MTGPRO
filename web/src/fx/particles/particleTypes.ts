import type { FxPoint, FxRect } from '../fxTypes'

/**
 * Vocabulario da camada de particulas. Um `FxBurst` e um PEDIDO — quantas
 * particulas ele vira, e com que fisica, e decisao do spawner. Quem emite so
 * descreve o que aconteceu na partida.
 */
export type FxBurst =
  /** Faisca radial no ponto de encontro; `power` e o dano. */
  | { kind: 'impact'; at: FxPoint; power: number; color: string }
  /** Dissolucao em brasa: a carta se desfaz para cima. */
  | { kind: 'ember'; rect: FxRect; color: string }
  /** Rastro luminoso varrendo o caminho da magia. */
  | { kind: 'trail'; from: FxPoint; to: FxPoint; color: string }
  /** Onda de choque no retrato do jogador. */
  | { kind: 'shock'; at: FxPoint; power: number; color: string }
  /** Materializacao: particulas convergindo para a carta. */
  | { kind: 'converge'; rect: FxRect; color: string }
  /** Explosao contida da tela final; `radius` e o vao livre no miolo,
   *  onde o painel de resultado fica — a luz estoura em volta dele. */
  | { kind: 'victory'; at: FxPoint; radius: number; color: string }

/** Teto duro do pool. Acima disso o quadro custa mais que o efeito entrega. */
export const MAX_PARTICLES = 720

/** Sprite (spark com halo), streak (estilhaco alongado), ring (onda). */
export const SHAPE = { spark: 0, streak: 1, ring: 2 } as const

export type ParticleShape = (typeof SHAPE)[keyof typeof SHAPE]

/**
 * Struct de particula reaproveitada pelo pool: nasce inerte e nunca e
 * realocada, entao o GC nao acorda no meio de uma rajada.
 */
export interface Particle {
  active: boolean
  shape: ParticleShape
  x: number
  y: number
  vx: number
  vy: number
  gravity: number
  drag: number
  /** Tempo decorrido e espera antes de acender, ambos em ms. */
  age: number
  delay: number
  life: number
  /** Raio no nascimento e no fim — interpolado com easing de saida. */
  a0: number
  a1: number
  /** Espessura do traco; so `ring` usa. */
  width: number
  /** Expoente da queda de alfa: >1 apaga cedo, <1 sustenta o brilho. */
  fade: number
  /** Peso do halo volumetrico, 0..1. */
  glow: number
  color: string
  sprite: HTMLCanvasElement
}
