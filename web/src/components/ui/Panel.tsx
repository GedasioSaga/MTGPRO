import { clsx } from 'clsx'
import type { HTMLAttributes } from 'react'

/** Quanto o painel se afasta da mesa. */
export type PanelElevation = 'flush' | 'raised' | 'floating' | 'overlay'

/** De que o painel e feito. `plain` e superficie lisa; o resto tem textura. */
export type PanelMaterial = 'plain' | 'metal' | 'well' | 'glass'

export interface PanelProps extends HTMLAttributes<HTMLDivElement> {
  elevation?: PanelElevation
  material?: PanelMaterial
  /** Marca o painel como o do jogador ativo: ganha anel de acento. */
  active?: boolean
}

const ELEVATION: Record<PanelElevation, string> = {
  flush: 'bg-surface-base shadow-e1',
  raised: 'bg-surface-raised shadow-e2',
  floating: 'bg-surface-raised shadow-e3',
  overlay: 'bg-surface-overlay shadow-e4',
}

const MATERIAL: Record<PanelMaterial, string> = {
  plain: 'border border-edge-hair',
  metal: 'metal-plate',
  well: 'metal-well',
  glass: 'glass',
}

/**
 * Caixa neutra do sistema. Nao sabe nada de partida — so de material,
 * elevacao e de quem esta com o turno.
 */
export function Panel({
  elevation = 'raised',
  material = 'plain',
  active = false,
  className,
  ...rest
}: PanelProps) {
  return (
    <div
      className={clsx(
        'relative rounded-lg',
        // `metal` e `glass` ja trazem sombra e borda proprias.
        material === 'plain' || material === 'well' ? ELEVATION[elevation] : null,
        MATERIAL[material],
        active && 'accent-ring',
        'transition-shadow duration-[var(--duration-base)] ease-standard',
        className,
      )}
      {...rest}
    />
  )
}
