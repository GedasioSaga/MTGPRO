import { clsx } from 'clsx'
import type { ReactNode } from 'react'

export type DividerTone = 'hair' | 'soft' | 'accent'
export type DividerOrientation = 'horizontal' | 'vertical'

export interface DividerProps {
  orientation?: DividerOrientation
  tone?: DividerTone
  /** Rotulo centrado; so vale na orientacao horizontal. */
  label?: ReactNode
  className?: string
}

const TONE: Record<DividerTone, string> = {
  hair: 'bg-edge-hair',
  soft: 'rule-soft',
  accent: 'bg-accent/45',
}

/** Separador fino. Com `label`, vira cabecalho de secao em caixa alta. */
export function Divider({
  orientation = 'horizontal',
  tone = 'soft',
  label,
  className,
}: DividerProps) {
  if (orientation === 'vertical') {
    return (
      <div
        role="separator"
        aria-orientation="vertical"
        className={clsx('w-px shrink-0 self-stretch', TONE[tone], className)}
      />
    )
  }

  if (label === undefined) {
    return (
      <div
        role="separator"
        aria-orientation="horizontal"
        className={clsx('h-px w-full shrink-0', TONE[tone], className)}
      />
    )
  }

  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      className={clsx('flex w-full items-center gap-3', className)}
    >
      <span className={clsx('h-px flex-1', TONE[tone])} />
      <span className="caps text-hud whitespace-nowrap text-ink-muted">{label}</span>
      <span className={clsx('h-px flex-1', TONE[tone])} />
    </div>
  )
}
