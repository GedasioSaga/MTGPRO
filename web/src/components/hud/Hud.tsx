import { GameLog } from './GameLog'
import { PlaybackBar } from './PlaybackBar'
import { Toast } from './Toast'

/**
 * Moldura da interface durante a partida.
 *
 * Fica fora do `BoardLayout` de propósito: a mesa é o conteúdo que o usuário
 * assiste, e o HUD flutua por cima sem disputar espaço no grid dela. Por isso
 * cada peça é `fixed` e o container não intercepta ponteiro — só os filhos.
 */
export function Hud() {
  return (
    <div className="pointer-events-none fixed inset-0 z-[80]">
      <div className="pointer-events-auto absolute bottom-0 left-1/2 -translate-x-1/2">
        <PlaybackBar />
      </div>
      <div className="pointer-events-auto absolute bottom-24 left-4 w-[320px] max-h-[42vh]">
        <GameLog />
      </div>
      <Toast />
    </div>
  )
}
