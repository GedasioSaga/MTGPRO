import { MATERIAL_LEATHER, MATERIAL_METAL } from '../../design/materials'

/*
 * A MESA — o móvel em que os dois playmats estão apoiados.
 *
 * Nesta rodada o protagonista passou a ser o mat, então a arena parou de ser
 * arquitetura e voltou a ser o que é numa partida de verdade: um tampo de couro
 * escuro com um friso de metal na borda. Piso esculpido, patamares e nichos
 * saíram — eles disputavam a leitura com a serigrafia do mat, e dois objetos
 * com relevo próprio na mesma tela viram ruído.
 *
 * O que sobrou tem uma função só: dar CHÃO ao mat. Sem um tampo mais escuro por
 * baixo, o mat não é um objeto pousado — é o fundo da tela, e volta o vazio.
 *
 * Luz: uma só, vinda do topo. A aresta virada para cima acende, a virada para
 * baixo escurece, nos dois lados da mesa.
 */

const TABLE_RADIUS = 18

/** Textura procedural como VÉU sobre a base pintada: se o filtro falhar, some
 *  só a textura e o tampo continua lá. */
function MaterialVeil({ filter, opacity }: { filter: string; opacity: number }) {
  return (
    <rect
      x="0"
      y="0"
      width="1000"
      height="1000"
      fill="#8a8177"
      opacity={opacity}
      filter={`url(#${filter})`}
      style={{ mixBlendMode: 'soft-light' }}
    />
  )
}

export function ArenaFrame() {
  return (
    <div className="arena-layer arena-layer--table" aria-hidden="true">
      <svg
        className="arena-layer__svg"
        viewBox="0 0 1000 1000"
        preserveAspectRatio="none"
        focusable="false"
      >
        <defs>
          <linearGradient id="arena-table-face" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="#221a12" />
            <stop offset="0.42" stopColor="#150f0a" />
            <stop offset="1" stopColor="#0a0705" />
          </linearGradient>

          {/* Friso: metal claro na aresta de cima, apagado na de baixo. É o
              único traço da moldura que continua desenhado. */}
          <linearGradient id="arena-table-rim" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="rgba(238,226,198,0.5)" />
            <stop offset="0.4" stopColor="rgba(150,136,112,0.2)" />
            <stop offset="1" stopColor="rgba(0,0,0,0.6)" />
          </linearGradient>
        </defs>

        <g style={{ isolation: 'isolate' }}>
          <rect
            x="0"
            y="0"
            width="1000"
            height="1000"
            rx={TABLE_RADIUS}
            ry={TABLE_RADIUS}
            fill="url(#arena-table-face)"
          />
          <MaterialVeil filter={MATERIAL_LEATHER} opacity={0.22} />
          <MaterialVeil filter={MATERIAL_METAL} opacity={0.06} />
        </g>

        <rect
          x="1"
          y="1"
          width="998"
          height="998"
          rx={TABLE_RADIUS}
          ry={TABLE_RADIUS}
          fill="none"
          stroke="url(#arena-table-rim)"
          strokeWidth="2"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
    </div>
  )
}
