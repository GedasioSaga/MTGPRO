import { clamp } from '../fxMotion'
import { glowSprite } from './glowSprite'
import type { FxBurst, Particle, ParticleShape } from './particleTypes'
import { SHAPE } from './particleTypes'

/** Branco quente do nucleo de qualquer faisca — nao e cor de mana, e calor. */
const HOT = '#fff4d6'

/** Dano acima disso ja saturou o efeito; mais particula so vira sopa. */
const MAX_POWER = 12

type Acquire = () => Particle

interface SpawnOptions {
  x: number
  y: number
  life: number
  a0: number
  color: string
  shape?: ParticleShape
  vx?: number
  vy?: number
  gravity?: number
  drag?: number
  delay?: number
  a1?: number
  width?: number
  fade?: number
  glow?: number
}

function put(acquire: Acquire, o: SpawnOptions): void {
  const p = acquire()
  p.shape = o.shape ?? SHAPE.spark
  p.x = o.x
  p.y = o.y
  p.vx = o.vx ?? 0
  p.vy = o.vy ?? 0
  p.gravity = o.gravity ?? 0
  p.drag = o.drag ?? 0
  p.age = 0
  p.delay = o.delay ?? 0
  p.life = o.life
  p.a0 = o.a0
  p.a1 = o.a1 ?? o.a0 * 0.35
  p.width = o.width ?? 2
  p.fade = o.fade ?? 1.4
  p.glow = o.glow ?? 1
  p.color = o.color
  p.sprite = glowSprite(o.color)
}

function rand(min: number, max: number): number {
  return min + Math.random() * (max - min)
}

function impact(acquire: Acquire, burst: Extract<FxBurst, { kind: 'impact' }>): void {
  const power = clamp(burst.power, 1, MAX_POWER)
  const sparks = Math.round(9 + power * 2.4)

  put(acquire, {
    x: burst.at.x,
    y: burst.at.y,
    life: 190,
    a0: 7 + power * 1.5,
    a1: 2,
    color: HOT,
    fade: 2.2,
  })

  put(acquire, {
    shape: SHAPE.ring,
    x: burst.at.x,
    y: burst.at.y,
    life: 260 + power * 14,
    a0: 5,
    a1: 24 + power * 9,
    width: 3,
    color: burst.color,
    fade: 1.5,
  })

  for (let i = 0; i < sparks; i += 1) {
    const angle = rand(0, Math.PI * 2)
    const speed = rand(70, 190) + power * 20
    put(acquire, {
      x: burst.at.x,
      y: burst.at.y,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      gravity: 900,
      drag: 1.9,
      life: rand(240, 470),
      a0: rand(1.3, 2.9),
      color: i % 3 === 0 ? HOT : burst.color,
      fade: 1.6,
    })
  }

  // Estilhaco: sai rapido, e puxado forte pela gravidade e cai fora do quadro.
  const shards = Math.round(clamp(2 + power * 0.9, 3, 11))
  for (let i = 0; i < shards; i += 1) {
    const angle = rand(-Math.PI, 0) + rand(-0.4, 0.4)
    const speed = rand(230, 430) + power * 16
    put(acquire, {
      shape: SHAPE.streak,
      x: burst.at.x,
      y: burst.at.y,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      gravity: 1500,
      drag: 1,
      life: rand(320, 560),
      a0: rand(1.8, 3),
      color: burst.color,
      fade: 1.3,
    })
  }
}

function ember(acquire: Acquire, burst: Extract<FxBurst, { kind: 'ember' }>): void {
  const { rect } = burst
  const bottom = rect.y + rect.h / 2

  for (let i = 0; i < 30; i += 1) {
    const x = rect.x + rand(-rect.w / 2, rect.w / 2)
    const y = rect.y + rand(-rect.h / 2, rect.h / 2)
    put(acquire, {
      x,
      y,
      vx: rand(-16, 16),
      vy: -rand(26, 96),
      // Gravidade negativa: brasa quente sobe e acelera, nao flutua parada.
      gravity: -28,
      drag: 0.9,
      // A carta se desfaz de baixo para cima, como papel queimando.
      delay: ((bottom - y) / rect.h) * 220,
      life: rand(520, 940),
      a0: rand(1.2, 3.4),
      a1: 0.3,
      color: i % 4 === 0 ? HOT : burst.color,
      fade: 1.2,
    })
  }

  for (let i = 0; i < 6; i += 1) {
    put(acquire, {
      x: rect.x + rand(-rect.w / 3, rect.w / 3),
      y: rect.y + rand(-rect.h / 4, rect.h / 3),
      vx: rand(-10, 10),
      vy: -rand(40, 80),
      gravity: -20,
      drag: 1.1,
      delay: rand(0, 180),
      life: rand(700, 1050),
      a0: rand(3.2, 5),
      a1: 0.6,
      color: burst.color,
      fade: 1.6,
    })
  }
}

function trail(acquire: Acquire, burst: Extract<FxBurst, { kind: 'trail' }>): void {
  const dx = burst.to.x - burst.from.x
  const dy = burst.to.y - burst.from.y
  const length = Math.hypot(dx, dy) || 1
  const ux = dx / length
  const uy = dy / length
  const px = -uy
  const py = ux
  const sweepMs = 240

  for (let i = 0; i < 24; i += 1) {
    const t = i / 23
    const offset = rand(-5, 5)
    const drift = rand(-34, 34)
    put(acquire, {
      x: burst.from.x + dx * t + px * offset,
      y: burst.from.y + dy * t + py * offset,
      vx: ux * 40 + px * drift,
      vy: uy * 40 + py * drift,
      gravity: 60,
      drag: 1.4,
      delay: t * sweepMs,
      life: rand(260, 460),
      a0: rand(1.8, 3.4),
      color: i % 4 === 0 ? HOT : burst.color,
      fade: 1.5,
    })
  }

  // Nucleo volumetrico: quatro esferas de raio decrescente na mesma trilha
  // somam em `lighter` e produzem profundidade que um `box-shadow` nao tem.
  for (let i = 0; i < 4; i += 1) {
    const t = 0.55 + i * 0.15
    put(acquire, {
      x: burst.from.x + dx * t,
      y: burst.from.y + dy * t,
      vx: ux * 30,
      vy: uy * 30,
      drag: 2.2,
      delay: t * sweepMs,
      life: 340,
      a0: 8 - i * 1.4,
      a1: 1.5,
      color: burst.color,
      fade: 2,
    })
  }

  put(acquire, {
    shape: SHAPE.ring,
    x: burst.to.x,
    y: burst.to.y,
    delay: sweepMs,
    life: 320,
    a0: 4,
    a1: 24,
    width: 2,
    color: burst.color,
    fade: 1.6,
  })
}

function shock(acquire: Acquire, burst: Extract<FxBurst, { kind: 'shock' }>): void {
  const power = clamp(burst.power, 1, MAX_POWER)
  const reach = 46 + power * 13

  for (let i = 0; i < 2; i += 1) {
    put(acquire, {
      shape: SHAPE.ring,
      x: burst.at.x,
      y: burst.at.y,
      delay: i * 80,
      life: 420 + i * 110,
      a0: 9,
      a1: reach * (1 - i * 0.32),
      width: 4 - i * 1.5,
      color: burst.color,
      fade: 1.4,
    })
  }

  for (let i = 0; i < 12; i += 1) {
    const angle = rand(0, Math.PI * 2)
    const speed = 110 + power * 15
    put(acquire, {
      x: burst.at.x,
      y: burst.at.y,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      gravity: 520,
      drag: 2.2,
      life: rand(300, 500),
      a0: rand(1.4, 2.6),
      color: i % 3 === 0 ? HOT : burst.color,
      fade: 1.6,
    })
  }
}

function converge(acquire: Acquire, burst: Extract<FxBurst, { kind: 'converge' }>): void {
  const { rect } = burst
  const reach = Math.max(rect.w, rect.h) * 0.85
  const travelMs = 360

  for (let i = 0; i < 26; i += 1) {
    const angle = rand(0, Math.PI * 2)
    const radius = reach * rand(0.9, 1.3)
    // Velocidade calculada para chegar ao centro no fim da vida — a particula
    // nao "para" no alvo, ela apaga exatamente ao encostar.
    const speed = (radius / travelMs) * 1000
    put(acquire, {
      x: rect.x + Math.cos(angle) * radius,
      y: rect.y + Math.sin(angle) * radius,
      vx: -Math.cos(angle) * speed,
      vy: -Math.sin(angle) * speed,
      delay: rand(0, 140),
      life: travelMs,
      a0: rand(1.4, 3),
      a1: 0.8,
      color: i % 4 === 0 ? HOT : burst.color,
      fade: 0.9,
    })
  }

  put(acquire, {
    shape: SHAPE.ring,
    x: rect.x,
    y: rect.y,
    life: 420,
    a0: reach * 1.15,
    a1: reach * 0.12,
    width: 2.4,
    color: burst.color,
    fade: 1.1,
  })

  put(acquire, {
    x: rect.x,
    y: rect.y,
    delay: 330,
    life: 240,
    a0: 9,
    a1: 1,
    color: HOT,
    fade: 2.2,
  })
}

function victory(acquire: Acquire, burst: Extract<FxBurst, { kind: 'victory' }>): void {
  for (let i = 0; i < 3; i += 1) {
    put(acquire, {
      shape: SHAPE.ring,
      x: burst.at.x,
      y: burst.at.y,
      delay: i * 110,
      life: 620 + i * 120,
      a0: 12,
      a1: 150 + i * 46,
      width: 4 - i,
      color: burst.color,
      fade: 1.5,
    })
  }

  for (let i = 0; i < 80; i += 1) {
    const angle = rand(0, Math.PI * 2)
    const speed = rand(160, 560)
    put(acquire, {
      x: burst.at.x,
      y: burst.at.y,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      gravity: 620,
      drag: 1.15,
      delay: rand(0, 90),
      life: rand(700, 1300),
      a0: rand(1.6, 4),
      color: i % 3 === 0 ? HOT : burst.color,
      fade: 1.25,
    })
  }

  for (let i = 0; i < 14; i += 1) {
    const angle = rand(-Math.PI, 0)
    const speed = rand(420, 720)
    put(acquire, {
      shape: SHAPE.streak,
      x: burst.at.x,
      y: burst.at.y,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      gravity: 900,
      drag: 0.9,
      life: rand(520, 900),
      a0: rand(2, 3.4),
      color: burst.color,
      fade: 1.2,
    })
  }

  // Poeira lenta que sobe depois do estouro: da o "assentar" da explosao.
  for (let i = 0; i < 22; i += 1) {
    put(acquire, {
      x: burst.at.x + rand(-160, 160),
      y: burst.at.y + rand(-60, 90),
      vx: rand(-24, 24),
      vy: -rand(18, 54),
      gravity: -14,
      drag: 0.6,
      delay: rand(120, 520),
      life: rand(900, 1400),
      a0: rand(1.2, 2.4),
      color: burst.color,
      fade: 1.1,
    })
  }
}

/** Traduz um pedido em particulas concretas, tirando cada uma do pool. */
export function spawnBurst(burst: FxBurst, acquire: Acquire): void {
  switch (burst.kind) {
    case 'impact':
      impact(acquire, burst)
      return
    case 'ember':
      ember(acquire, burst)
      return
    case 'trail':
      trail(acquire, burst)
      return
    case 'shock':
      shock(acquire, burst)
      return
    case 'converge':
      converge(acquire, burst)
      return
    case 'victory':
      victory(acquire, burst)
      return
    default: {
      const unreachable: never = burst
      return unreachable
    }
  }
}
