import { clsx } from 'clsx'
import type { ButtonHTMLAttributes, ReactNode } from 'react'

export type IconButtonTone = 'ghost' | 'metal' | 'accent' | 'danger'

export interface IconButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children' | 'aria-label'> {
  /** Vira `aria-label`: o botao so tem icone, entao o nome e obrigatorio. */
  label: string
  icon: ReactNode
  tone?: IconButtonTone
  /** Estado ligado (ex.: play ativo). Espelhado em `aria-pressed`. */
  pressed?: boolean
}

const TONE: Record<IconButtonTone, string> = {
  ghost: 'text-ink-muted hover:text-ink-strong hover:bg-white/6 active:bg-white/10',
  metal: 'metal-plate text-ink hover:text-ink-strong hover:brightness-125',
  accent:
    'metal-plate text-accent-bright hover:brightness-125 [box-shadow:var(--shadow-e2),inset_0_1px_0_var(--color-edge-gloss),0_0_16px_-6px_var(--accent)]',
  danger: 'metal-plate text-danger hover:text-danger hover:brightness-125',
}

/**
 * Botao so de icone. Area de toque fixa em 40px mesmo com icone pequeno —
 * abaixo disso o alvo fica menor que a ponta do dedo.
 */
export function IconButton({
  label,
  icon,
  tone = 'ghost',
  pressed,
  className,
  type = 'button',
  ...rest
}: IconButtonProps) {
  return (
    <button
      type={type}
      aria-label={label}
      aria-pressed={pressed}
      className={clsx(
        'inline-flex min-h-10 min-w-10 shrink-0 items-center justify-center rounded-md',
        'cursor-pointer select-none',
        'transition-[color,background-color,filter,transform] duration-[var(--duration-micro)] ease-standard',
        'active:scale-[0.96]',
        'disabled:pointer-events-none disabled:opacity-40',
        TONE[tone],
        pressed && tone === 'ghost' && 'bg-white/10 text-ink-strong',
        className,
      )}
      {...rest}
    >
      <span aria-hidden="true" className="grid place-items-center [&>svg]:size-[18px]">
        {icon}
      </span>
    </button>
  )
}
