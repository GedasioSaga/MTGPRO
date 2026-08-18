import clsx from 'clsx'
import { hashString } from './boardVisuals'

export interface CardBackProps {
  /** Semente do padrão: cartas diferentes ganham verso levemente diferente. */
  seed: string
  className?: string
}

/**
 * Verso de carta desenhado em CSS. Existe para a mão do oponente e para
 * qualquer id sem `CardView` — nunca mostramos caixa quebrada.
 */
export function CardBack({ seed, className }: CardBackProps) {
  const hash = hashString(seed)
  const angle = 30 + (hash % 60)

  return (
    <div
      className={clsx(
        'size-full overflow-hidden rounded-[7px] ring-1 ring-white/10',
        className,
      )}
      style={{
        background: `linear-gradient(${angle}deg, #1c1526 0%, #2a2038 42%, #14101d 100%)`,
        boxShadow: 'inset 0 0 0 3px rgba(12,9,17,0.9), 0 6px 18px -10px rgba(0,0,0,0.9)',
      }}
      aria-hidden="true"
    >
      <div className="grid size-full place-items-center p-[14%]">
        <div
          className="size-full rounded-full"
          style={{
            background:
              'radial-gradient(circle at 38% 32%, rgba(214,178,110,0.42) 0%, rgba(214,178,110,0.1) 38%, transparent 62%)',
            border: '1px solid rgba(214,178,110,0.28)',
          }}
        />
      </div>
    </div>
  )
}
