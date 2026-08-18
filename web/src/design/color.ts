/*
 * Helpers de cor compartilhados por mesa, carta e FX.
 *
 * Existiam tres copias identicas destes dois (boardVisuals, cardVisuals,
 * fxColors) porque cada camada nasceu em uma rodada diferente. Duas copias de
 * um hash sao especialmente traicoeiras: se uma divergir, a mesma carta passa a
 * sortear arte diferente dependendo de quem desenhou.
 */

/** FNV-1a. Deterministico entre sessoes — a mesma carta cai sempre no mesmo tom. */
export function hashString(value: string): number {
  let hash = 2166136261
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  return hash >>> 0
}

/** Converte hex (#abc ou #aabbcc) para `rgba(...)` sem depender de biblioteca. */
export function withAlpha(hex: string, alpha: number): string {
  const clean = hex.replace('#', '')
  const full =
    clean.length === 3
      ? clean
          .split('')
          .map((channel) => channel + channel)
          .join('')
      : clean
  const value = Number.parseInt(full, 16)
  const r = (value >> 16) & 255
  const g = (value >> 8) & 255
  const b = value & 255
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}
