import { PlaybackBar } from './PlaybackBar'
import { Toast } from './Toast'

/**
 * Moldura flutuante da partida.
 *
 * Só o que é transitório vive aqui. O registro e a leitura da partida moram em
 * colunas do grid (`App.tsx`), justamente para não cobrir a mesa; a barra de
 * reprodução continua flutuando na base, e o grid da mesa reserva a faixa
 * abaixo da mão para ela (`--playback-reserve` em `App.css`).
 */
export function Hud() {
  return (
    <div className="pointer-events-none fixed inset-0 z-[80]">
      <div className="pointer-events-auto absolute bottom-3 left-1/2 -translate-x-1/2">
        <PlaybackBar />
      </div>
      <Toast />
    </div>
  )
}
