# Fontes das imagens de referência — MTG Arena (tela de partida)

Coletadas em 2026-08-17 para servir de barra de comparação visual num review de UI.

## arena-board-01.jpg
- **Fonte:** Steam Store (loja oficial da Wizards no Steam), app 2141910 "Magic: The Gathering Arena"
- **URL:** https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/2141910/a5256bda348ed5984bedc8be64193d21afa4422b/ss_a5256bda348ed5984bedc8be64193d21afa4422b.1920x1080.jpg
- **O que mostra:** Tela de partida sem sobreposição promocional pesada — tabuleiro visto de cima, vida total do jogador (4), mãos de terrenos (Plains x4), criaturas na mesa (Rumbling Baloth 8/8, Shrine Keeper 2/2, Loxodon Line Breaker 3/2) em posição de combate/bloqueio, e caixa de diálogo do tutorial. É a imagem mais limpa do lote — corresponde bem ao pedido (tabuleiro + vida + combate).

## arena-board-02-cosmetics-promo.jpg
- **Fonte:** Steam Store, mesma página do app 2141910
- **URL:** https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/2141910/1ad290730ef170a5798a9d67c4b1dadf490c7558/ss_1ad290730ef170a5798a9d67c4b1dadf490c7558.1920x1080.jpg
- **O que mostra:** Tela de partida ao fundo (vida total 20, mão, pilha de cartas na mesa, criaturas) mas com **arte promocional de cosméticos sobreposta** (texto "STUNNING COSMETICS" e mascote grande em primeiro plano). Útil para ver HUD de vida/mão, mas parcialmente obstruída — usar com ressalva.

## arena-board-03-vs-promo.jpg
- **Fonte:** Steam Store, mesma página do app 2141910
- **URL:** https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/2141910/952fffb8e828e35fed045e2827a687fd71291669/ss_952fffb8e828e35fed045e2827a687fd71291669.1920x1080.jpg
- **O que mostra:** Tela de partida ao fundo (vida total 18, cartas na mesa) com **overlay promocional "VS"** e ilustrações de personagem em primeiro plano cobrindo boa parte do tabuleiro. Mesma ressalva do arquivo anterior — HUD real visível só nas bordas.

## arena-board-04-wizards-official.jpg
- **Fonte:** Site oficial magic.wizards.com/en/mtgarena (imagem de compartilhamento/meta da página)
- **URL:** https://images.ctfassets.net/s5n2t79q9icq/cAQJBdFMcKDZY4BhUUsDx/616212775582792942f3927c843aea80/arena_Meta-ShareImage.jpg
- **O que mostra:** Tela de partida ao fundo (vida total 20, mão de terrenos, criaturas na mesa, carta de habilidade/pilha em destaque no canto superior direito) com o **logo "Magic The Gathering Arena" sobreposto** no centro. Fonte oficial da Wizards, mas também parcialmente obstruída pelo logo.

## Observação sobre cobertura
Não foi possível obter screenshots 100% limpas (sem overlay promocional/logo) do cliente de partida nas fontes tentadas — a Steam Store só disponibiliza 6 screenshots oficiais para este app, das quais 2 são menu/coleção/draft (descartadas) e as demais 3 usadas aqui têm elementos promocionais sobrepostos ao HUD real. `arena-board-01.jpg` é a mais limpa e deve ser a referência principal. `mtg.fandom.com` retornou erro HTTP 402 (paywall) e não pôde ser usada. Google Play Store não retornou screenshots utilizáveis via fetch estático (página requer JS).
