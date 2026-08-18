import { withAlpha } from '../../design/color'
import { glowSprite } from './glowSprite'
import { spawnBurst } from './spawnBurst'
import type { FxBurst, Particle } from './particleTypes'
import { MAX_PARTICLES, SHAPE } from './particleTypes'

/** Passo maximo de INTEGRACAO: acima disso a fisica salta e a curva quebra. */
const MAX_STEP_S = 0.032
/**
 * Tempo real maximo recuperado num quadro so.
 *
 * O passo de integracao e curto de proposito, mas o tempo decorrido nao pode
 * ser jogado fora com ele: se cada quadro so envelhece 32ms, a vida da
 * particula passa a ser contada em QUADROS e nao em milissegundos, e num
 * ambiente com poucos quadros por segundo — maquina fraca, aba em segundo
 * plano, captura automatizada — um anel de 288ms fica varios segundos parado na
 * tela. Era assim que o halo verde do rodape parecia preso por cima do END.
 * Aqui o quadro recupera ate 250ms de tempo real em sub-passos, e o que passar
 * disso e descartado para a volta de uma aba dormente nao teleportar tudo.
 */
const MAX_CATCHUP_S = 0.25
/** Retina custa 4x fill rate por nada; 2 ja e o teto util. */
const MAX_DPR = 2

export interface ParticleField {
  emit: (burst: FxBurst) => void
  resize: () => void
  dispose: () => void
}

function blankParticle(): Particle {
  return {
    active: false,
    shape: SHAPE.spark,
    x: 0,
    y: 0,
    vx: 0,
    vy: 0,
    gravity: 0,
    drag: 0,
    age: 0,
    delay: 0,
    life: 1,
    a0: 1,
    a1: 1,
    width: 1,
    fade: 1,
    glow: 1,
    color: '#ffffff',
    sprite: glowSprite('#ffffff'),
  }
}

function easeOut(t: number): number {
  const inv = 1 - t
  return 1 - inv * inv * inv
}

/**
 * Camada de particulas da partida inteira: UM canvas, UM pool, UM
 * `requestAnimationFrame`. O laco so existe enquanto ha particula viva — sem
 * efeito na tela nao ha quadro agendado, e a bateria fica quieta.
 */
export function createParticleField(canvas: HTMLCanvasElement): ParticleField | null {
  const ctx = canvas.getContext('2d', { alpha: true })
  if (ctx === null) return null

  const pool: Particle[] = Array.from({ length: MAX_PARTICLES }, blankParticle)
  const free: number[] = pool.map((_, index) => index)
  // Cursor de reciclagem: com o pool cheio, a rajada nova rouba a particula
  // mais antiga em vez de ser engolida pela fumaca da anterior.
  let recycleCursor = 0

  let width = 0
  let height = 0
  /** Quadro agendado, ou `null` quando o laco esta parado. */
  let frame: number | null = null
  let lastTs = 0

  const resize = (): void => {
    const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR)
    width = window.innerWidth
    height = window.innerHeight
    canvas.width = Math.round(width * dpr)
    canvas.height = Math.round(height * dpr)
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  }

  const acquire = (): Particle => {
    const index = free.pop()
    if (index !== undefined) {
      const particle = pool[index]!
      particle.active = true
      return particle
    }
    recycleCursor = (recycleCursor + 1) % MAX_PARTICLES
    const particle = pool[recycleCursor]!
    particle.active = true
    return particle
  }

  const release = (index: number): void => {
    const particle = pool[index]!
    if (!particle.active) return
    particle.active = false
    free.push(index)
  }

  /** Avanca a fisica e devolve quantas particulas continuam vivas. */
  const step = (dt: number): number => {
    let live = 0
    for (let i = 0; i < MAX_PARTICLES; i += 1) {
      const p = pool[i]!
      if (!p.active) continue
      p.age += dt * 1000
      if (p.age - p.delay >= p.life) {
        release(i)
        continue
      }
      live += 1
      if (p.age < p.delay) continue
      const damping = 1 - Math.min(p.drag * dt, 0.95)
      p.vy = (p.vy + p.gravity * dt) * damping
      p.vx *= damping
      p.x += p.vx * dt
      p.y += p.vy * dt
    }
    return live
  }

  /** Pinta o quadro e devolve quantas particulas foram de fato desenhadas. */
  const draw = (): number => {
    let painted = 0
    ctx.globalCompositeOperation = 'lighter'
    for (let i = 0; i < MAX_PARTICLES; i += 1) {
      const p = pool[i]!
      if (!p.active || p.age < p.delay) continue
      const t = (p.age - p.delay) / p.life
      // Acende em 10% do tempo de vida; o resto e queda controlada pelo `fade`.
      const alpha = Math.min(1, t / 0.1) * Math.pow(1 - t, p.fade)
      if (alpha <= 0.01) continue
      const radius = p.a0 + (p.a1 - p.a0) * easeOut(t)
      painted += 1

      if (p.shape === SHAPE.ring) {
        // O anel pinta pelo `strokeStyle`, entao precisa zerar o `globalAlpha`
        // que a particula anterior deixou — senao pisca conforme a ordem.
        ctx.globalAlpha = 1
        // Duas passadas de espessura diferente fazem o brilho ter volume —
        // um traco unico com sombra sairia chapado e caro.
        ctx.strokeStyle = withAlpha(p.color, alpha * 0.22)
        ctx.lineWidth = p.width * 3.2
        ctx.beginPath()
        ctx.arc(p.x, p.y, radius, 0, Math.PI * 2)
        ctx.stroke()
        ctx.strokeStyle = withAlpha(p.color, alpha * 0.9)
        ctx.lineWidth = Math.max(0.6, p.width * (1 - t))
        ctx.beginPath()
        ctx.arc(p.x, p.y, radius, 0, Math.PI * 2)
        ctx.stroke()
        continue
      }

      if (p.shape === SHAPE.streak) {
        const speed = Math.hypot(p.vx, p.vy)
        const length = Math.max(radius * 3, Math.min(speed * 0.05, 54))
        ctx.save()
        ctx.translate(p.x, p.y)
        ctx.rotate(Math.atan2(p.vy, p.vx))
        ctx.globalAlpha = alpha * 0.3 * p.glow
        ctx.drawImage(p.sprite, -length * 0.75, -radius * 2.6, length * 1.5, radius * 5.2)
        ctx.globalAlpha = alpha
        ctx.drawImage(p.sprite, -length * 0.5, -radius, length, radius * 2)
        ctx.restore()
        continue
      }

      const halo = radius * 3.4
      ctx.globalAlpha = alpha * 0.34 * p.glow
      ctx.drawImage(p.sprite, p.x - halo, p.y - halo, halo * 2, halo * 2)
      ctx.globalAlpha = alpha
      ctx.drawImage(p.sprite, p.x - radius, p.y - radius, radius * 2, radius * 2)
    }
    ctx.globalAlpha = 1
    ctx.globalCompositeOperation = 'source-over'
    return painted
  }

  /*
   * O laco so pode parar num quadro que LIMPOU e nao pintou nada.
   *
   * A versao anterior parava assim que um contador de vivos zerava, e esse
   * contador era mantido a mao pelo par acquire/release. Bastava ele dessincar
   * um passo para o laco morrer no quadro em que a ultima particula ainda
   * aparecia — e o desenho dela ficava congelado na tela ate a proxima rajada.
   * Era isso o anel verde parado no canto do rodape, por cima do marco END.
   * Agora quem decide sao os dois numeros que o proprio quadro produziu, entao
   * nao ha estado paralelo para divergir da tela.
   */
  const tick = (ts: number): void => {
    frame = null
    let remaining = Math.min(Math.max((ts - lastTs) / 1000, 0), MAX_CATCHUP_S)
    lastTs = ts
    ctx.clearRect(0, 0, width, height)
    let live = step(0)
    while (remaining > 0) {
      const dt = Math.min(remaining, MAX_STEP_S)
      live = step(dt)
      remaining -= dt
    }
    const painted = draw()
    if (live > 0 || painted > 0) schedule()
  }

  /** Agenda o proximo quadro; chamar duas vezes no mesmo quadro nao duplica. */
  function schedule(): void {
    if (frame !== null) return
    frame = requestAnimationFrame(tick)
  }

  const emit = (burst: FxBurst): void => {
    spawnBurst(burst, acquire)
    schedule()
  }

  const dispose = (): void => {
    if (frame !== null) cancelAnimationFrame(frame)
    frame = null
    for (let i = 0; i < MAX_PARTICLES; i += 1) release(i)
    ctx.clearRect(0, 0, width, height)
  }

  resize()
  return { emit, resize, dispose }
}
