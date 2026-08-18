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

/**
 * Silhueta do piso: hexágono alongado de arena. Arestas retas em cima e
 * embaixo, laterais que ABREM até o ponto mais largo em y=500 — que é a linha
 * de combate. Retângulo arredondado aqui vira painel; esta forma não vira.
 */
const FLOOR =
  'M262 42 L738 42 C812 42 862 90 884 170 C932 302 960 402 962 500 ' +
  'C960 598 932 698 884 830 C862 910 812 958 738 958 L262 958 ' +
  'C188 958 138 910 116 830 C68 698 40 598 38 500 ' +
  'C40 402 68 302 116 170 C138 90 188 42 262 42 Z'

/** Patamar do oponente: estreito na aresta externa, cheio na linha de combate. */
/*
 * Patamares: BANDAS de largura cheia, recortadas pela silhueta do piso. Dar
 * contorno próprio a cada patamar desenhava um trapézio dentro da arena — e
 * trapézio com aresta visível é exatamente a caixa que esta rodada veio tirar.
 * Cada banda acende na aresta externa e apaga na costura: as duas caem para o
 * centro, e o degrau nasce do encontro delas.
 */
const TERRACE_TOP = 'M0 0 H1000 V462 H0 Z'
const TERRACE_BOTTOM = 'M0 538 H1000 V1000 H0 Z'
/** Lábio do patamar de baixo, rente à costura: é o degrau que o olho lê. */
const TERRACE_BOTTOM_LIP = 'M42 538 L958 538'
/** Queda do patamar de cima para a vala. Escura, não desenhada. */
const TERRACE_TOP_FALL = 'M42 462 L958 462'

/** Cartucho onde o retrato encaixa. Aponta para FORA da mesa dos dois lados. */
const KEYSTONE_BOTTOM =
  'M278 -6 L722 -6 L722 58 C722 72 714 86 700 94 L656 120 L344 120 L300 94 C286 86 278 72 278 58 Z'
const KEYSTONE_TOP =
  'M278 106 L722 106 L722 42 C722 28 714 14 700 6 L656 -20 L344 -20 L300 6 C286 14 278 28 278 42 Z'

const RIM_SCALE = 'translate(500 500) scale(1.055 1.078) translate(-500 -500)'

/** Juntas do anel. 0° e 180° caem na linha de combate: o eixo vira desenho. */
const RIM_JOINTS = [0, 30, 60, 90, 120, 150, 180, 210, 240, 270, 300, 330]

const STONE_LIT = '#5f6d63'
const STONE_MID = '#333e37'
const STONE_DEEP = '#0d1310'

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
      {/* Biblioteca de materiais: monta solta, fora do `<defs>` da arena, para
          os ids valerem no documento sem depender de aninhamento de `<svg>`. */}
      <MaterialDefs />

      <svg className="arena-defs" aria-hidden="true" focusable="false">
        <defs>
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
            y1="0"
            x2="0"
            y2="462"
          >
            <stop offset="0" stopColor="rgba(216,255,236,0.1)" />
            <stop offset="0.5" stopColor="rgba(150,214,182,0.05)" />
            <stop offset="1" stopColor="rgba(0,0,0,0.34)" />
          </linearGradient>

          <linearGradient
            id="arena-terrace-bottom"
            gradientUnits="userSpaceOnUse"
            x1="0"
            y1="538"
            x2="0"
            y2="1000"
          >
            <stop offset="0" stopColor="rgba(226,255,242,0.15)" />
            <stop offset="0.5" stopColor="rgba(150,214,182,0.05)" />
            <stop offset="1" stopColor="rgba(0,0,0,0.32)" />
          </linearGradient>

          {/* Sombra do degrau: densa rente à costura, dissolvida acima dela.
              Barra chapada aqui vira listra; gradiente vira profundidade. */}
          <linearGradient
            id="arena-step-shade"
            gradientUnits="userSpaceOnUse"
            x1="0"
            y1="430"
            x2="0"
            y2="502"
          >
            <stop offset="0" stopColor="rgba(0,0,0,0)" />
            <stop offset="1" stopColor="rgba(0,0,0,0.5)" />
          </linearGradient>

          <radialGradient id="arena-medallion" cx="0.5" cy="0.5" r="0.5">
            <stop offset="0" stopColor="rgba(198,255,226,0.14)" />
            <stop offset="0.62" stopColor="rgba(120,180,152,0.05)" />
            <stop offset="1" stopColor="rgba(0,0,0,0)" />
          </radialGradient>

          <linearGradient id="arena-apron-face" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="#1d2420" />
            <stop offset="0.4" stopColor="#0f1512" />
            <stop offset="1" stopColor="#070a09" />
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

          {/* Só o anel de pedra: moldura menos piso. É onde as juntas moram. */}
          <mask id="arena-rim-mask" maskUnits="userSpaceOnUse" x="-100" y="-160" width="1200" height="1320">
            <path d={FLOOR} transform={RIM_SCALE} fill="#fff" />
            <path d={FLOOR} fill="#000" />
          </mask>
        </defs>
      </svg>

      <div className="arena-layer arena-layer--apron" aria-hidden="true">
        <svg viewBox="0 0 1000 1000" preserveAspectRatio="none" focusable="false">
          <g opacity="0.88" style={{ isolation: 'isolate' }}>
            <rect
              x="0"
              y="0"
              width="1000"
              height="1000"
              rx="14"
              ry="22"
              fill="url(#arena-apron-face)"
            />
            <MaterialVeil path="M0 0 H1000 V1000 H0 Z" filter={MATERIAL_LEATHER} opacity={0.16} />
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
          <g transform="translate(500 512) scale(1.075 1.11) translate(-500 -500)">
            <path d={FLOOR} fill="rgba(0,0,0,0.62)" filter="url(#arena-cast)" />
          </g>

          {/* Moldura esculpida: a mesma silhueta, um pouco maior. O anel que
              sobra entre ela e o piso É a pedra. */}
          <g
            transform={RIM_SCALE}
            style={{ isolation: 'isolate' }}
          >
            <path d={FLOOR} fill="url(#arena-rim-face)" />
            <MaterialVeil path={FLOOR} filter={MATERIAL_STONE} opacity={0.26} />
          </g>

          {/* Juntas radiais: é o que separa pedra ESCULPIDA de borda arredondada.
              A máscara recorta no anel, então nada invade o piso. */}
          <g mask="url(#arena-rim-mask)">
            {RIM_JOINTS.map((deg) => {
              const rad = (deg * Math.PI) / 180
              const x = 500 + Math.cos(rad) * 700
              const y = 500 + Math.sin(rad) * 700
              return (
                <g key={deg}>
                  <path
                    d={`M500 500 L${x.toFixed(1)} ${y.toFixed(1)}`}
                    stroke="rgba(0,0,0,0.55)"
                    strokeWidth="4"
                    vectorEffect="non-scaling-stroke"
                  />
                  <path
                    d={`M501.5 502 L${(x + 1.5).toFixed(1)} ${(y + 2).toFixed(1)}`}
                    stroke="rgba(226,255,242,0.16)"
                    strokeWidth="1"
                    vectorEffect="non-scaling-stroke"
                  />
                </g>
              )
            })}
          </g>

          <g style={{ isolation: 'isolate' }}>
            <path d={FLOOR} fill="url(#arena-floor-face)" />
            <MaterialVeil path={FLOOR} filter={MATERIAL_FELT} opacity={0.2} />
          </g>

          <g clipPath="url(#arena-floor-clip)">
            <path d={TERRACE_TOP} fill="url(#arena-terrace-top)" />
            <path
              d={TERRACE_TOP_FALL}
              fill="none"
              stroke="rgba(0,0,0,0.5)"
              strokeWidth="2"
              vectorEffect="non-scaling-stroke"
            />
            {/* Vala entre o degrau de cima e a costura: sombra, não linha. */}
            <rect x="0" y="430" width="1000" height="72" fill="url(#arena-step-shade)" />

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
            <ellipse cx="500" cy="500" rx="262" ry="52" fill="url(#arena-medallion)" />
            <ellipse
              cx="500"
              cy="500"
              rx="262"
              ry="52"
              fill="none"
              stroke="rgba(210,255,232,0.16)"
              strokeWidth="1"
              vectorEffect="non-scaling-stroke"
            />
            <ellipse
              cx="500"
              cy="500"
              rx="182"
              ry="34"
              fill="none"
              stroke="rgba(210,255,232,0.09)"
              strokeWidth="1"
              vectorEffect="non-scaling-stroke"
            />
            <path
              d="M46 500 H222 M778 500 H954"
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
          <MaterialVeil path={keystone} filter={MATERIAL_METAL} opacity={0.3} />
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
