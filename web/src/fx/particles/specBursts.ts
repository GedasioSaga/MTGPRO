import { viewportCenter } from '../fxAnchors'
import { FX_TONES } from '../fxColors'
import type { FxSpec } from '../fxTypes'
import type { FxBurst } from './particleTypes'

/** Aco batendo em aco: a faisca do bloqueio nao tem cor de mana. */
const STEEL = '#ffe0a3'
/** O choque do bloqueio ainda nao sabe o dano; vale um golpe medio. */
const CLASH_POWER = 3

/** Meia-diagonal aproximada do painel de fim de jogo, com folga. */
function panelHalo(): number {
  if (typeof window === 'undefined') return 300
  return Math.max(260, Math.min(window.innerWidth, window.innerHeight) * 0.32)
}

/**
 * Ponte entre o que a camada de efeitos ja decidiu desenhar e a camada de
 * particulas. Ler o `FxSpec` (e nao o `MatchEvent`) mantem UM ponto de ligacao
 * e faz a demo de efeitos ganhar particula de graca.
 *
 * Todo ponto usado aqui e LOCAL — o ponto do duelo, o retrato, a carta. Nada
 * atravessa a tela.
 */
export function burstsForSpec(spec: FxSpec): FxBurst[] {
  switch (spec.kind) {
    case 'castBeam':
      return [{ kind: 'trail', from: spec.from, to: spec.to, color: spec.color }]

    case 'resolveBurst':
      return [{ kind: 'impact', at: spec.at, power: 2, color: spec.color }]

    case 'counterShatter':
      return [{ kind: 'impact', at: spec.at, power: 5, color: FX_TONES.counter }]

    case 'blockClash':
      return [{ kind: 'impact', at: spec.at, power: CLASH_POWER, color: STEEL }]

    case 'damageNumber': {
      if (spec.tone !== 'damage') return []
      if (spec.subject === 'player') {
        return [
          { kind: 'shock', at: spec.at, power: spec.amount, color: FX_TONES.damage },
          { kind: 'impact', at: spec.at, power: spec.amount * 0.6, color: FX_TONES.damage },
        ]
      }
      return [
        {
          kind: 'impact',
          at: spec.at,
          power: spec.amount,
          color: spec.lethal ? FX_TONES.death : FX_TONES.damage,
        },
      ]
    }

    case 'deathDissolve':
      return [
        {
          kind: 'ember',
          rect: spec.rect,
          color: spec.tone === 'exile' ? FX_TONES.exile : FX_TONES.death,
        },
      ]

    case 'tokenSpawn':
      return [{ kind: 'converge', rect: spec.rect, color: FX_TONES.trigger }]

    case 'gameOver':
      return [{ kind: 'victory', at: viewportCenter(), radius: panelHalo(), color: FX_TONES.victory }]

    default:
      return []
  }
}
