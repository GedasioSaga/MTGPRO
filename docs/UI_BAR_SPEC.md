# Barra visual — o que a referência faz e nós não

Bar principal: `docs/bar/arena-ixalan-board.png` (cliente de partida do MTG Arena,
era Ixalan, tabuleiro cheio). Substitui `arena-board-01.jpg`, que era tutorial
com pouca coisa em jogo e overlay promocional.

> `docs/bar/` está fora do git: são capturas da Wizards, referência interna, não
> se redistribui em repositório público. `docs/reference-mana-symbols.png` fica
> versionado porque é especificação de forma, não captura de produto.

## Observações acionáveis da bar

Cada item abaixo é algo que a referência resolve e que o nosso cliente ainda não.

### 1. Terreno agrupado em pilha com contador
Quatro Florestas viram **uma pilha** com o rótulo `x4` e a carta do topo visível.
Montanhas idem. Isso devolve metade da largura da faixa de terrenos e elimina a
fileira de miniaturas repetidas que hoje ocupa espaço sem informar nada.
Terreno virado mostra a **seta de rotação** sobreposta, não a carta girada — em
pilha, girar cada carta viraria mingau.

### 2. Ícone de habilidade em badge circular na borda da carta
Voar, pressa, vigilância, atropelar aparecem como **círculo escuro com glifo
branco** encostado na borda esquerda da miniatura, empilhados verticalmente.
Hoje nós só temos a palavra-chave no texto do hover — o estado de combate não se
lê sem abrir a carta.

### 3. P/T colorido quando modificado
`5/5` aparece em **azul** quando o valor difere do impresso, e em cinza quando é
o valor natural. É a leitura instantânea de "essa criatura está buffada" sem
precisar comparar com nada.

### 4. Cartas com moldura por identidade de cor
A borda da miniatura carrega a cor da carta (verde, azul, vermelho, multicolor).
Nosso `cardVisuals` já deriva cor — falta a moldura ler à distância.

### 5. Arte de ambiente nas bordas do tabuleiro
Os cantos têm arte pintada (flores, arquitetura de templo) que fecha a
composição. É o que impede a tela de ler como retângulo de aplicação. Não temos
asset pintado, então o equivalente é **arquitetura procedural**: moldura
esculpida com bisel e material, que é o que o passe de material está fazendo.

### 6. Retrato e vida integrados à moldura
O oponente tem retrato circular com moldura ornamentada no canto superior
esquerdo; o jogador tem a vida num **escudo** no centro inferior. Nenhum dos dois
é uma caixa retangular flutuante.

### 7. Pips de mana disponível na base
Uma fileira de símbolos mostra a mana flutuante. Nós escondemos isso quando
removemos "sem mana flutuante" — voltar como pip, não como frase.

### 8. VFX pertence à cena, não à interface
O efeito da mágica (vinhas verdes) passa **por cima das cartas e do tabuleiro**,
com a criatura invocada em escala maior que a carta. Não é um badge nem um chip:
é um evento que acontece na mesa.

## Símbolos de mana — especificação de forma

`docs/reference-mana-symbols.png` fixa as cinco cores oficiais e seus glifos:

| Cor | Glifo | Fundo | Símbolo |
|---|---|---|---|
| Branco `{W}` | sol de raios curvos com anel central | creme/amarelo dourado | branco |
| Azul `{U}` | gota d'água | azul médio | branco |
| Preto `{B}` | caveira estilizada | roxo escuro | branco |
| Vermelho `{R}` | chama em espiral | vermelho carmesim | branco |
| Verde `{G}` | árvore com copa e raízes | verde floresta | branco |

Todos: círculo pleno, glifo branco vazado, sem contorno externo. Genérico é
círculo cinza com o número. Híbrido é o círculo dividido na diagonal com as duas
cores e os dois glifos reduzidos.

## Segunda referência: `spelltable-commander.png`

Mesa física por webcam, quatro jogadores, com painel de histórico à direita
mostrando as últimas cartas jogadas em miniatura. Não é a bar do nosso layout
(produto diferente), mas ensina uma coisa: **o histórico com miniatura da carta**
lê muito melhor que o nosso log de texto. Candidato para o painel `REGISTRO`.
