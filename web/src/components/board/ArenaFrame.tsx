import {
  MATERIAL_FELT,
  MATERIAL_LEATHER,
  MATERIAL_METAL,
  MATERIAL_STONE,
  MaterialDefs,
} from '../../design/materials'

/*
 * A ARENA como objeto físico, desenhada ATRÁS das cartas.
 *
 * A oscilação que travou as últimas rodadas ("faltam zonas demarcadas" x "são
 * caixas demais") vinha de sinalizar zona com contorno ou com nada. Aqui a zona
 * não é sinalizada: ela é a FORMA da arena. O piso tem silhueta própria —
 * hexágono alongado, mais largo exatamente na linha de combate — e cada lado
 * tem seu patamar, que existe por relevo e sombra, não por linha desenhada.
 *
 * Cada camada é um filho da própria `.board-grid` colocado por `grid-area`, e
 * não um SVG de tela cheia. É isso que faz a geometria bater com as fileiras
 * sem medir nada em JS: o piso ocupa exatamente o intervalo `foe-field →
 * own-field`, simétrico em torno da costura, então o eixo de simetria do
 * desenho É a linha de combate, por construção.
 *
 * Luz: uma só, vinda do topo da tela. Nada aqui é espelhado — aresta virada
 * para cima acende, aresta virada para baixo escurece, dos dois lados da mesa.
 */

/** Silhueta do piso. Ponto mais largo em y=500, que é a linha de combate. */
const FLOOR =
  'M200 26 L800 26 C866 26 906 54 918 110 C952 260 976 380 984 500 ' +
  'C976 620 952 740 918 890 C906 946 866 974 800 974 L200 974 ' +
  'C134 974 94 946 82 890 C48 740 24 620 16 500 ' +
  'C24 380 48 260 82 110 C94 54 134 26 200 26 Z'

/** Patamar do oponente: estreito na borda externa, cheio na linha de combate. */
const TERRACE_TOP =
  'M158 44 L842 44 C894 44 930 72 940 124 L996 458 L4 458 L60 124 C70 72 106 44 158 44 Z'
/** Aresta virada para cima do patamar de cima — é a que pega luz. */
const TERRACE_TOP_LIP = 'M60 124 C70 72 106 44 158 44 L842 44 C894 44 930 72 940 124'

const TERRACE_BOTTOM =
  'M4 542 L996 542 L940 876 C930 928 894 956 842 956 L158 956 C106 956 70 928 60 876 Z'
/** Lábio do patamar de baixo, rente à costura: é o degrau que o olho lê. */
const TERRACE_BOTTOM_LIP = 'M4 542 L996 542'

/** Cartucho onde o retrato encaixa. Aponta para FORA da mesa dos dois lados. */
const KEYSTONE_BOTTOM =
  'M278 -6 L722 -6 L722 58 C722 72 714 86 700 94 L656 120 L344 120 L300 94 C286 86 278 72 278 58 Z'
const KEYSTONE_TOP =
  'M278 106 L722 106 L722 42 C722 28 714 14 700 6 L656 -20 L344 -20 L300 6 C286 14 278 28 278 42 Z'

const STONE_LIT = '#7d8c81'
const STONE_MID = '#46534b'
const STONE_DEEP = '#131b18'

/** A textura procedural entra como VÉU sobre a base pintada, nunca como fundo:
 *  se o filtro de material não resolver, some só a textura e a base continua. */
function MaterialVeil({
  path,
  filter,
  opacity,
}: {
  path: string
  filter: string
  opacity: number
}) {
  return (
    <path
      d={path}
      fill="#8f9a90"
      opacity={opacity}
      filter={`url(#${filter})`}
      style={{ mixBlendMode: 'soft-light' }}
    />
  )
}

export function ArenaFrame() {
  return (
    <>
      <svg className="arena-defs" aria-hidden="true" focusable="false">
        <defs>
          <MaterialDefs />

          {/* Pedra iluminada de cima: face clara na aresta superior, escura na
              inferior. É esse par que faz a moldura ler como volume. */}
          <linearGradient id="arena-rim-face" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor={STONE_LIT} />
            <stop offset="0.34" stopColor={STONE_MID} />
            <stop offset="1" stopColor={STONE_DEEP} />
          </linearGradient>

          {/* Bisel do encaixe: parede interna de cima na sombra, de baixo na luz
              — o inverso da moldura. É o que diz que o piso AFUNDA. */}
          <linearGradient id="arena-socket-bevel" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="rgba(0,0,0,0.85)" />
            <stop offset="0.5" stopColor="rgba(0,0,0,0.25)" />
            <stop offset="1" stopColor="rgba(214,255,232,0.4)" />
          </linearGradient>

          <linearGradient id="arena-floor-face" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="#0c1c18" />
            <stop offset="0.5" stopColor="#173229" />
            <stop offset="1" stopColor="#0a1714" />
          </linearGradient>

          <linearGradient
            id="arena-terrace-top"
            gradientUnits="userSpaceOnUse"
            x1="0"
            y1="44"
            x2="0"
            y2="458"
          >
            <stop offset="0" stopColor="rgba(216,255,236,0.15)" />
            <stop offset="0.5" stopColor="rgba(150,214,182,0.05)" />
            <stop offset="1" stopColor="rgba(0,0,0,0.34)" />
          </linearGradient>

          <linearGradient
            id="arena-terrace-bottom"
            gradientUnits="userSpaceOnUse"
            x1="0"
            y1="542"
            x2="0"
            y2="956"
          >
            <stop offset="0" stopColor="rgba(226,255,242,0.17)" />
            <stop offset="0.5" stopColor="rgba(150,214,182,0.05)" />
            <stop offset="1" stopColor="rgba(0,0,0,0.36)" />
          </linearGradient>

          <radialGradient id="arena-medallion" cx="0.5" cy="0.5" r="0.5">
            <stop offset="0" stopColor="rgba(198,255,226,0.14)" />
            <stop offset="0.62" stopColor="rgba(120,180,152,0.05)" />
            <stop offset="1" stopColor="rgba(0,0,0,0)" />
          </radialGradient>

          <linearGradient id="arena-apron-face" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="#2b332c" />
            <stop offset="0.4" stopColor="#161d19" />
            <stop offset="1" stopColor="#080c0a" />
          </linearGradient>

          <linearGradient id="arena-keystone-face" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="#66756a" />
            <stop offset="0.46" stopColor="#38443c" />
            <stop offset="1" stopColor="#111815" />
          </linearGradient>

          {/* Sombra projetada da moldura no chão. Estática de propósito: filtro
              que anima em laço custa GPU a cada frame. */}
          <filter id="arena-cast" x="-12%" y="-14%" width="124%" height="128%">
            <feGaussianBlur stdDeviation="14" />
          </filter>

          <clipPath id="arena-floor-clip">
            <path d={FLOOR} />
          </clipPath>
        </defs>
      </svg>

      <div className="arena-layer arena-layer--apron" aria-hidden="true">
        <svg viewBox="0 0 1000 1000" preserveAspectRatio="none" focusable="false">
          <g style={{ isolation: 'isolate' }}>
            <rect
              x="0"
              y="0"
              width="1000"
              height="1000"
              rx="14"
              ry="22"
              fill="url(#arena-apron-face)"
            />
            <MaterialVeil path="M0 0 H1000 V1000 H0 Z" filter={MATERIAL_LEATHER} opacity={0.5} />
          </g>
          {/* Aresta externa do tampo: fio de luz em cima, queda embaixo. */}
          <rect
            x="0.5"
            y="0.5"
            width="999"
            height="999"
            rx="14"
            ry="22"
            fill="none"
            stroke="url(#arena-rim-face)"
            strokeWidth="2"
            vectorEffect="non-scaling-stroke"
            opacity="0.7"
          />
        </svg>
      </div>

      <div className="arena-layer arena-layer--floor" aria-hidden="true">
        <svg viewBox="0 0 1000 1000" preserveAspectRatio="none" focusable="false">
          {/* Sombra da moldura caindo no chão, deslocada para baixo. */}
          <g transform="translate(500 512) scale(1.075 1.13) translate(-500 -500)">
            <path d={FLOOR} fill="rgba(0,0,0,0.62)" filter="url(#arena-cast)" />
          </g>

          {/* Moldura esculpida: a mesma silhueta, um pouco maior. O anel que
              sobra entre ela e o piso É a pedra. */}
          <g
            transform="translate(500 500) scale(1.058 1.105) translate(-500 -500)"
            style={{ isolation: 'isolate' }}
          >
            <path d={FLOOR} fill="url(#arena-rim-face)" />
            <MaterialVeil path={FLOOR} filter={MATERIAL_STONE} opacity={0.62} />
          </g>

          <g style={{ isolation: 'isolate' }}>
            <path d={FLOOR} fill="url(#arena-floor-face)" />
            <MaterialVeil path={FLOOR} filter={MATERIAL_FELT} opacity={0.34} />
          </g>

          <g clipPath="url(#arena-floor-clip)">
            <path d={TERRACE_TOP} fill="url(#arena-terrace-top)" />
            <path
              d={TERRACE_TOP_LIP}
              fill="none"
              stroke="rgba(226,255,242,0.32)"
              strokeWidth="1.5"
              vectorEffect="non-scaling-stroke"
            />
            {/* Vala entre o degrau de cima e a costura: sombra, não linha. */}
            <rect x="0" y="452" width="1000" height="46" fill="rgba(0,0,0,0.42)" />

            <path d={TERRACE_BOTTOM} fill="url(#arena-terrace-bottom)" />
            <path
              d={TERRACE_BOTTOM_LIP}
              fill="none"
              stroke="rgba(232,255,244,0.34)"
              strokeWidth="1.5"
              vectorEffect="non-scaling-stroke"
            />

            {/* Eixo da arena: o medalhão é a marca de centro, e as duas sulcas
                correm até a borda amarrando os dois patamares. */}
            <ellipse cx="500" cy="500" rx="212" ry="48" fill="url(#arena-medallion)" />
            <ellipse
              cx="500"
              cy="500"
              rx="212"
              ry="48"
              fill="none"
              stroke="rgba(210,255,232,0.16)"
              strokeWidth="1"
              vectorEffect="non-scaling-stroke"
            />
            <ellipse
              cx="500"
              cy="500"
              rx="148"
              ry="32"
              fill="none"
              stroke="rgba(210,255,232,0.09)"
              strokeWidth="1"
              vectorEffect="non-scaling-stroke"
            />
            <path
              d="M28 500 H272 M728 500 H972"
              stroke="rgba(206,250,228,0.14)"
              strokeWidth="1"
              vectorEffect="non-scaling-stroke"
            />
          </g>

          {/* Bisel do encaixe, por último: é a aresta viva entre pedra e piso. */}
          <path
            d={FLOOR}
            fill="none"
            stroke="url(#arena-socket-bevel)"
            strokeWidth="3"
            vectorEffect="non-scaling-stroke"
          />
        </svg>
      </div>

      <ArenaNiche side="top" />
      <ArenaNiche side="bottom" />
    </>
  )
}

/**
 * Nicho da moldura: a cavidade em que retrato e vida se ENCAIXAM. Ocupa a mesma
 * linha da grade que a `PlayerBar`, então altura e raio batem sem ninguém medir
 * nada — e a vida deixa de morar numa caixa flutuante solta.
 */
function ArenaNiche({ side }: { side: 'top' | 'bottom' }) {
  const keystone = side === 'top' ? KEYSTONE_TOP : KEYSTONE_BOTTOM

  return (
    <div className={`arena-layer arena-layer--niche arena-layer--niche-${side}`} aria-hidden="true">
      <svg viewBox="0 0 1000 100" preserveAspectRatio="none" focusable="false">
        {/* Cavidade: escura, com bisel invertido — parede de cima na sombra. */}
        <rect x="10" y="8" width="980" height="84" rx="24" ry="30" fill="rgba(6,11,9,0.55)" />
        <rect
          x="10"
          y="8"
          width="980"
          height="84"
          rx="24"
          ry="30"
          fill="none"
          stroke="url(#arena-socket-bevel)"
          strokeWidth="2"
          vectorEffect="non-scaling-stroke"
        />

        <g style={{ isolation: 'isolate' }}>
          <path d={keystone} fill="url(#arena-keystone-face)" />
          <MaterialVeil path={keystone} filter={MATERIAL_METAL} opacity={0.55} />
        </g>
        <path
          d={keystone}
          fill="none"
          stroke="rgba(220,245,232,0.22)"
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
    </div>
  )
}
