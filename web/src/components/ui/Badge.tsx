import { clsx } from 'clsx'
import type { HTMLAttributes } from 'react'
import type { ManaKey } from '@/design/tokens'

export type BadgeTone =
  | ManaKey
  | 'neutral'
  | 'accent'
  | 'success'
  | 'danger'
  | 'warning'
  | 'plus'
  | 'minus'

export type BadgeSize = 'micro' | 'small' | 'medium'
export type BadgeShape = 'pill' | 'circle'

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone
  size?: BadgeSize
  shape?: BadgeShape
  /** Halo colorido. Reserve para o que acabou de mudar. */
  glow?: boolean
}

/*
 * Classes escritas por extenso de proposito: o Tailwind varre o codigo-fonte,
 * entao nome de classe montado por concatenacao nunca chega ao CSS final.
 */
const TONE: Record<BadgeTone, string> = {
  W: 'bg-mana-w-bg text-mana-w-bright ring-mana-w/35',
  U: 'bg-mana-u-bg text-mana-u-bright ring-mana-u/40',
  B: 'bg-mana-b-bg text-mana-b-bright ring-mana-b/40',
  R: 'bg-mana-r-bg text-mana-r-bright ring-mana-r/40',
  G: 'bg-mana-g-bg text-mana-g-bright ring-mana-g/40',
  C: 'bg-mana-c-bg text-mana-c-bright ring-mana-c/35',
  M: 'bg-mana-m-bg text-mana-m-bright ring-mana-m/40',
  neutral: 'bg-white/8 text-ink ring-edge',
  accent: 'bg-accent-bg text-accent-bright ring-accent/45',
  success: 'bg-success-bg text-success ring-success/40',
  danger: 'bg-danger-bg text-danger ring-danger/40',
  warning: 'bg-warning-bg text-warning ring-warning/40',
  plus: 'bg-success-bg text-success ring-success/45',
  minus: 'bg-danger-bg text-danger ring-danger/45',
}

const GLOW: Record<BadgeTone, string> = {
  W: 'shadow-[0_0_10px_-2px_var(--color-mana-w)]',
  U: 'shadow-[0_0_10px_-2px_var(--color-mana-u)]',
  B: 'shadow-[0_0_10px_-2px_var(--color-mana-b)]',
  R: 'shadow-[0_0_10px_-2px_var(--color-mana-r)]',
  G: 'shadow-[0_0_10px_-2px_var(--color-mana-g)]',
  C: 'shadow-[0_0_10px_-2px_var(--color-mana-c)]',
  M: 'shadow-[0_0_10px_-2px_var(--color-mana-m)]',
  neutral: 'shadow-[0_0_10px_-2px_rgba(255,255,255,0.5)]',
  accent: 'shadow-[0_0_12px_-2px_var(--accent)]',
  success: 'shadow-[0_0_10px_-2px_var(--color-success)]',
  danger: 'shadow-[0_0_10px_-2px_var(--color-danger)]',
  warning: 'shadow-[0_0_10px_-2px_var(--color-warning)]',
  plus: 'shadow-[0_0_10px_-2px_var(--color-success)]',
  minus: 'shadow-[0_0_10px_-2px_var(--color-danger)]',
}

const SIZE: Record<BadgeSize, string> = {
  micro: 'text-[10px] leading-none px-1.5 py-0.5 min-w-4',
  small: 'text-[11px] leading-none px-2 py-1 min-w-5',
  medium: 'text-[13px] leading-none px-2.5 py-1.5 min-w-7',
}

const CIRCLE: Record<BadgeSize, string> = {
  micro: 'size-4 p-0 text-[10px]',
  small: 'size-5 p-0 text-[11px]',
  medium: 'size-7 p-0 text-[13px]',
}

/** Selo compacto: contador, palavra-chave, simbolo de mana. */
export function Badge({
  tone = 'neutral',
  size = 'small',
  shape = 'pill',
  glow = false,
  className,
  ...rest
}: BadgeProps) {
  return (
    <span
      className={clsx(
        'inline-flex items-center justify-center font-semibold tabular-nums',
        'ring-1 ring-inset backdrop-blur-[2px]',
        shape === 'circle' ? 'rounded-full' : 'rounded-sm',
        shape === 'circle' ? CIRCLE[size] : SIZE[size],
        TONE[tone],
        glow && GLOW[tone],
        'transition-[background-color,box-shadow,color] duration-[var(--duration-fast)] ease-standard',
        className,
      )}
      {...rest}
    />
  )
}
