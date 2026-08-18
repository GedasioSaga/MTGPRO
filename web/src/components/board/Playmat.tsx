import type { CSSProperties, ReactElement } from 'react'
import clsx from 'clsx'
import type { Color } from '../../types/protocol'
import { ManaSymbol } from '../card/ManaCost'
import {
  PLAYMAT_VIEWBOX,
  manaPipsForSeat,
  playmatRect,
  rectCenter,
  rectStyle,
  zonesForSeat,
} from './playmatZones'
import type { NormRect, PlaymatZone, Seat } from './playmatZones'
import './playmat.css'

/*
 * O PLAYMAT — a terceira saída.
 *
 * As seis rodadas anteriores oscilaram entre dois erros que são o mesmo erro:
 * demarcar zona com contorno (a tela vira painel de controle) ou não demarcar
 * (a tela vira vazio). Os dois pressupõem que a linha é INTERFACE.
 *
 * Aqui ela não é. É tinta impressa sobre um objeto — o tapete de feltro que se
 * estende na mesa antes de embaralhar. Linha branca fina sobre arte, rótulo em
 * versalete rente à linha, borda costurada, sombra própria sobre o tampo.
 * Ninguém olha um playmat e pensa "que painel"; olha e pensa "essa é a mesa".
 * E a arte de fundo — escolhida pelo jogador, como o mat de verdade — resolve o
 * vazio sem exigir asset pintado nosso.
 *
 * CAMADAS, de baixo para cima:
 *   0. o próprio elemento: gradiente procedural das cores do deck (o "chão")
 *   1. `__art`: a imagem escolhida, `cover`, escurecida e dessaturada
 *   2. `__wash`: véu escuro + vinheta + a trama do feltro em `soft-light`
 *   3. `__print`: as linhas serigrafadas, em SVG
 *   4. `__labels` e `__pips`: tipografia e símbolos, em HTML, SEMPRE em pé
 *   5. `::before`/`::after`: costura e aresta do mat
 *
 * O chão vive no elemento raiz, e não numa camada própria, de propósito: assim
 * uma `artUrl` quebrada não tem como produzir caixa vazia — a imagem some e o
 * gradiente que já estava embaixo aparece. Nenhum `onError`, nenhum estado.
 */

export interface PlaymatProps {
  seat: Seat
  /**
   * Arte escolhida pelo jogador. `null` — ou ausente, enquanto a partida ainda
   * não disse qual mat é de quem — cai no gradiente procedural.
   */
  artUrl?: string | null
  /** Cores do deck, que pigmentam o chão quando não há arte. */
  deckColors?: Color[]
  className?: string
}

/** Tinta de cada cor no chão do mat: mais funda que o token de HUD, porque
 *  aqui ela é pigmento de tecido, não sinal de leitura. */
const DECK_INK: Record<Color, string> = {
  White: '#c9bd92',
  Blue: '#2b5f9e',
  Black: '#4b3d63',
  Red: '#8f2f22',
  Green: '#2a6b45',
}

const COLORLESS_INK = '#2c3542'
const GROUND_BASE = 'linear-gradient(176deg, #14202a 0%, #0d161d 52%, #070c11 100%)'

/**
 * Mancha de pigmento por cor do deck. Duas cores dão um mat bicolor, cinco dão
 * um mat que parece tie-dye de guilda — que é exatamente o que um mat de deck
 * de cinco cores parece.
 */
function groundPaint(colors: readonly Color[], seat: Seat): string {
  const inks = colors.length > 0 ? colors.map((color) => DECK_INK[color]) : [COLORLESS_INK]
  const blobs = inks.map((ink, index) => {
    const spread = inks.length === 1 ? 0.5 : index / (inks.length - 1)
    const x = 14 + spread * 72
    // Alterna a altura da mancha para o campo não virar faixa horizontal, e
    // inverte no assento de cima para os dois mats não ficarem idênticos.
    const high = index % 2 === 0
    const y = seat === 0 ? (high ? 30 : 74) : high ? 70 : 26
    const tint = `color-mix(in oklab, ${ink} 62%, transparent)`
    return `radial-gradient(62% 128% at ${x}% ${y}%, ${tint}, transparent 72%)`
  })
  return [...blobs, GROUND_BASE].join(', ')
}

/**
 * A URL vem de fora (escolha do jogador, arquivo de configuração), então entra
 * numa `url()` com aspas e sem os caracteres que fechariam a função — sem isso
 * uma aspa no meio do texto encerra o `url()` e o resto vira declaração CSS.
 */
function cssUrl(raw: string): string {
  return `url("${raw.replace(/["'()\\\r\n]/g, '')}")`
}

/** Elipse de centro do campo de batalha, como o círculo do mat de referência.
 *  Sob `preserveAspectRatio="none"` ela achata junto com o mat — o que é o
 *  desejado: numa metade de mesa larga e baixa, círculo pleno viraria bolha. */
function medallion(rect: NormRect): { cx: number; cy: number; rx: number; ry: number } {
  const center = rectCenter(rect)
  return {
    cx: center.x * PLAYMAT_VIEWBOX,
    cy: center.y * PLAYMAT_VIEWBOX,
    rx: rect.w * 0.19 * PLAYMAT_VIEWBOX,
    ry: rect.h * 0.42 * PLAYMAT_VIEWBOX,
  }
}

function ZonePrint({ zone }: { zone: PlaymatZone }): ReactElement {
  const main = zone.id === 'battlefield' || zone.id === 'lands'
  return (
    <rect
      x={zone.rect.x * PLAYMAT_VIEWBOX}
      y={zone.rect.y * PLAYMAT_VIEWBOX}
      width={zone.rect.w * PLAYMAT_VIEWBOX}
      height={zone.rect.h * PLAYMAT_VIEWBOX}
      rx={5}
      ry={5}
      fill="none"
      stroke="currentColor"
      strokeWidth={main ? 1.6 : 1.2}
      opacity={main ? 0.4 : 0.28}
      vectorEffect="non-scaling-stroke"
    />
  )
}

export function Playmat({
  seat,
  artUrl = null,
  deckColors = [],
  className,
}: PlaymatProps): ReactElement {
  const zones = zonesForSeat(seat)
  const pips = manaPipsForSeat(seat)
  const ring = medallion(playmatRect(seat, 'battlefield'))

  const surface: CSSProperties = { backgroundImage: groundPaint(deckColors, seat) }
  const art: CSSProperties | undefined =
    artUrl === null ? undefined : { backgroundImage: cssUrl(artUrl) }

  return (
    <div
      className={clsx('playmat', className)}
      data-seat={seat}
      style={surface}
      aria-hidden="true"
    >
      {art !== undefined && <div className="playmat__art" style={art} />}
      <div className="playmat__wash" />

      {/* `preserveAspectRatio="none"` estica o viewBox na caixa do mat, que é
          muito mais larga que alta. `non-scaling-stroke` é o que impede a linha
          de virar um traço grosso em X e um fio de cabelo em Y. */}
      <svg
        className="playmat__print"
        viewBox={`0 0 ${PLAYMAT_VIEWBOX} ${PLAYMAT_VIEWBOX}`}
        preserveAspectRatio="none"
        focusable="false"
      >
        <ellipse
          cx={ring.cx}
          cy={ring.cy}
          rx={ring.rx}
          ry={ring.ry}
          fill="none"
          stroke="currentColor"
          strokeWidth={1.2}
          opacity={0.18}
          vectorEffect="non-scaling-stroke"
        />
        {zones.map((zone) => (
          <ZonePrint key={zone.id} zone={zone} />
        ))}
      </svg>

      {/* Tipografia e símbolos ficam em HTML, fora do SVG esticado: dentro dele
          a letra deformaria junto com o viewBox. É também o que permite girar a
          geometria do assento 1 sem virar o rótulo de cabeça para baixo. */}
      {zones.map((zone) => (
        <span
          key={zone.id}
          className={`playmat__label playmat__label--${zone.labelAnchor} playmat__label--${
            zone.id === 'battlefield' || zone.id === 'lands' ? 'band' : 'slot'
          }`}
          style={rectStyle(zone.rect)}
        >
          <span className="playmat__labelText">{zone.label}</span>
        </span>
      ))}

      {pips.map((pip) => (
        <span
          key={pip.color}
          className="playmat__pip"
          style={{
            left: `${pip.cx * 100}%`,
            top: `${pip.cy * 100}%`,
            height: `${pip.diameter * 100}%`,
          }}
        >
          <ManaSymbol token={{ kind: 'color', color: pip.color }} size="100%" />
        </span>
      ))}
    </div>
  )
}
