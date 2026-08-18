import { PlaybackBar } from './PlaybackBar'
import { Toast } from './Toast'

/**
 * Moldura flutuante da partida.
 *
 * Só o que é transitório vive aqui. O registro e a leitura da partida são
 * overlays sob demanda (`App.tsx`), fechados por padrão para a mesa ficar com a
 * tela inteira; o transporte virou uma pastilha no canto inferior direito, que
 * só abre no ponteiro ou no foco, e o grid reserva apenas a altura dela
 * (`--playback-reserve` em `App.css`).
 */
export function Hud() {
  return (
    <div className="pointer-events-none fixed inset-0 z-[80]">
      <div className="pointer-events-auto absolute right-3 bottom-2">
        <PlaybackBar />
      </div>
      <Toast />
    </div>
  )
}
