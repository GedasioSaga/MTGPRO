import { withAlpha } from '../fxColors'

/** Lado do sprite em px. 64 ja e maior que qualquer particula desenhada. */
const SPRITE_PX = 64

const cache = new Map<string, HTMLCanvasElement>()

/**
 * Sprite radial de uma cor: nucleo branco quente, corpo na cor, borda que
 * some. Desenhar UM gradiente por cor e reusar via `drawImage` custa uma
 * fracao de recriar `createRadialGradient` a cada particula, a cada quadro.
 */
export function glowSprite(color: string): HTMLCanvasElement {
  const cached = cache.get(color)
  if (cached) return cached

  const canvas = document.createElement('canvas')
  canvas.width = SPRITE_PX
  canvas.height = SPRITE_PX
  const ctx = canvas.getContext('2d')
  if (ctx) {
    const half = SPRITE_PX / 2
    const gradient = ctx.createRadialGradient(half, half, 0, half, half, half)
    gradient.addColorStop(0, 'rgba(255, 255, 255, 1)')
    gradient.addColorStop(0.18, withAlpha(color, 0.95))
    gradient.addColorStop(0.45, withAlpha(color, 0.42))
    gradient.addColorStop(0.75, withAlpha(color, 0.12))
    gradient.addColorStop(1, withAlpha(color, 0))
    ctx.fillStyle = gradient
    ctx.fillRect(0, 0, SPRITE_PX, SPRITE_PX)
  }

  cache.set(color, canvas)
  return canvas
}
