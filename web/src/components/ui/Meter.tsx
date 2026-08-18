import { clsx } from 'clsx'
import type { CSSProperties } from 'react'

export type MeterTone = 'life' | 'poison' | 'accent' | 'success' | 'danger'
export type MeterSize = 'sm' | 'md'

export interface MeterProps {
  value: number
  max: number
  tone?: MeterTone
  /** Nome acessivel da barra (ex.: "Life, Bot A"). */
  label: string
  /** Marca uma divisao a cada N unidades — le a vida em blocos, nao em pixels. */
  segment?: number
  size?: MeterSize
  className?: string
}

const FILL: Record<MeterTone, string> = {
  life: 'from-life/70 via-life to-life/85',
  poison: 'from-poison/70 via-poison to-poison/85',
  accent: 'from-accent-deep via-accent to-accent-bright',
  success: 'from-success/70 via-success to-success/85',
  danger: 'from-danger/70 via-danger to-danger/85',
}

const GLOW: Record<MeterTone, string> = {
  life: 'shadow-[0_0_12px_-2px_var(--color-life)]',
  poison: 'shadow-[0_0_12px_-2px_var(--color-poison)]',
  accent: 'shadow-[0_0_12px_-2px_var(--accent)]',
  success: 'shadow-[0_0_12px_-2px_var(--color-success)]',
  danger: 'shadow-[0_0_12px_-2px_var(--color-danger)]',
}

const HEIGHT: Record<MeterSize, string> = {
  sm: 'h-1.5',
  md: 'h-2.5',
}

/** Barra de recurso. Sem numero embutido: quem mostra o total e o HUD. */
export function Meter({
  value,
  max,
  tone = 'life',
  label,
  segment,
  size = 'md',
  className,
}: MeterProps) {
  const safeMax = Math.max(1, max)
  const clamped = Math.min(Math.max(value, 0), safeMax)
  const ratio = clamped / safeMax

  const ticks: CSSProperties | undefined =
    segment && segment > 0
      ? {
          backgroundImage:
            'repeating-linear-gradient(90deg, transparent 0, transparent calc(var(--seg) - 1px), rgba(0,0,0,0.55) calc(var(--seg) - 1px), rgba(0,0,0,0.55) var(--seg))',
          // CSSProperties nao tipa custom property; o cast e a saida padrao.
          '--seg': `${(segment / safeMax) * 100}%`,
        } as CSSProperties
      : undefined

  return (
    <div
      role="meter"
      aria-label={label}
      aria-valuenow={clamped}
      aria-valuemin={0}
      aria-valuemax={safeMax}
      className={clsx(
        'metal-well relative w-full overflow-hidden rounded-full',
        HEIGHT[size],
        className,
      )}
    >
      <div
        className={clsx(
          'h-full origin-left rounded-full bg-gradient-to-r',
          FILL[tone],
          GLOW[tone],
          'transition-[width] duration-[var(--duration-slow)] ease-out',
        )}
        style={{ width: `${ratio * 100}%` }}
      />
      {/* Gloss por cima do preenchimento: da volume sem escurecer a cor. */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 rounded-full bg-gradient-to-b from-white/22 via-transparent to-black/25"
      />
      {ticks ? (
        <div aria-hidden="true" className="pointer-events-none absolute inset-0" style={ticks} />
      ) : null}
    </div>
  )
}
