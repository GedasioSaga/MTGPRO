# MTGPRO — simulador automático de Magic

Motor de regras de *Magic: The Gathering* em Rust, com cliente web em React.
Duas IAs jogam; você assiste. Não há input de jogo — é um simulador, no espírito
do YGOPro: partida completa, regras corretas, reprodução com ritmo e animação.

Cartas são escritas em **Lua**.

```lua
card {
  name = "Lightning Bolt", cost = "{R}", type = "Instant",
  rarity = "Common", set = "LEA",
  text = "Lightning Bolt deals 3 damage to any target.",
  targets = { t_any() },
  effect = deal_damage(3),
}
```

## Estado, com números aferidos

| Métrica | Valor |
|---|---|
| Interações de regra cobertas | **65/65** (`docs/RULES_TESTS.md`) |
| Testes | **175 passando**, 0 falhando |
| Fuzzing | 200 sementes, **0 pânicos**, 187 com vencedor |
| Catálogo | **172 cartas** em Lua, 4 decks de 60 |
| Cartas lançáveis | 100% do catálogo |
| IA heurística vs aleatória | **46/50 (92%)**, 0 derrotas |
| Determinismo | mesma semente, partida idêntica |
| Cliente | `tsc` 0 erros · 0 erro de console · ~85 fps a 1920×1080 |

Três bugs de regra achados e corrigidos, cada um com a regra citada:
**CR 603.6d** (informação de última existência), **CR 702.16c** (proteção não
prevenia dano fora de combate) e **CR 611.2b** (efeito não expirava com a fonte
fora do campo).

## Arquitetura

```
                    ┌──────────────────────┐
                    │   Catálogo de cartas │   cards/*.lua  →  mtg-script (mlua,
                    │  Lua → IR → SQLite   │   sandboxed)   →  CardDef  →  SQLite
                    └──────────┬───────────┘
                               ▼
              ┌────────────────────────────────┐
              │        mtg-core (Rust)         │
              │  IR de efeitos · camadas 613   │
              │  pilha · gatilhos · SBA 704    │
              │  combate 506–511 · view        │
              └───────┬────────────────┬───────┘
                      │                │
                 mtg-ai            mtg-server
              bots heurísticos    axum + WebSocket
                                        │
                                        ▼
                                  web/ (React 19)
                             MatchEvent → animação
```

Decisões que valem a explicação:

- **Carta é dado, não código.** `CardDef` é uma árvore de `Effect` serializável.
  Lua é a camada de autoria em cima disso: o script roda uma vez no carregamento
  e *produz* o IR. Em partida o motor interpreta IR puro — nenhuma chamada de Lua
  no laço quente, então facilidade de escrever não custa performance nem
  determinismo.
- **Sandbox no Lua não é zelo, é requisito.** `io`, `os`, `package`, `require`,
  `load` e companhia saem do ambiente antes de qualquer chunk rodar. Script de
  carta é conteúdo, e conteúdo vira contribuição de terceiro.
- **Arena de índices em vez de referências.** Todo objeto é `ObjectId`; aura que
  aponta para criatura que aponta para controlador é um grafo, e grafo com `&mut`
  em Rust não fecha.
- **Motor síncrono com callback.** O simulador é automático, então o bot responde
  na hora: sem máquina de estado suspensa, que é onde motor de card game acumula
  bug.
- **UI só desenha.** O cliente recebe `GameView` já redigido por observador e
  `MatchEvent` com duração sugerida. Nenhuma regra de Magic vive no TypeScript.
- **Playmat por jogador.** O tabuleiro é um tapete com zonas impressas, e cada
  lado escolhe a própria arte — carta do deck, URL colada (só `https:` e
  `data:image/`), ou gradiente por cor.

## Estrutura

| Caminho | O que é |
|---|---|
| `crates/mtg-core` | regras, estado, IR de efeitos, projeção para a UI |
| `crates/mtg-script` | interpretador Lua sandboxed + DSL de cartas |
| `crates/mtg-cards` | catálogo e decks, carregados de `cards/*.lua` |
| `crates/mtg-db` | persistência SQLite do catálogo |
| `crates/mtg-ai` | bots (`random`, `heuristic`, `greedy`) |
| `crates/mtg-server` | HTTP + WebSocket, roda a simulação e transmite |
| `cards/` | o catálogo, um arquivo por cor |
| `web` | cliente React 19 + Vite + Tailwind 4 + Motion |
| `docs/ENGINE_CONTRACT.md` | assinaturas obrigatórias e protocolo de rede |
| `docs/RULES_TESTS.md` | as 65 interações, com a regra oficial citada |

## Rodando

```bash
cargo run -p mtg-server          # http://127.0.0.1:8787
cd web && npm install && npm run dev
```

O cliente funciona sem o servidor: cai para uma partida de demonstração embutida.

## Verificação

```bash
cargo test --workspace                    # 175 testes
cargo test --workspace -- --ignored       # fuzzing de 200 sementes, catálogo inteiro
cargo clippy --workspace --all-targets

cd web
rm -f node_modules/.tmp/*.tsbuildinfo     # o cache incremental mente sobre árvore antiga
npx tsc -p tsconfig.app.json --noEmit     # `tsc --noEmit` sozinho não checa nada aqui
npm run build
```

## Escrevendo uma carta

Adicione ao arquivo da cor em `cards/`. O prelúdio (`crates/mtg-script/lua/prelude.lua`)
tem os construtores:

```lua
card {
  name = "Serra Angel", cost = "{3}{W}{W}", type = "Creature — Angel",
  pt = { 4, 4 }, rarity = "Uncommon", set = "LEA",
  keywords = { "Flying", "Vigilance" },
  text = "Flying, vigilance",
}

card {
  name = "Wall of Omens", cost = "{1}{W}", type = "Creature — Wall",
  pt = { 0, 4 }, rarity = "Uncommon", set = "ROE",
  keywords = { "Defender" },
  abilities = { etb(draw(1)) },
  text = "Defender\nWhen Wall of Omens enters the battlefield, draw a card.",
}
```

`CardScriptHost::reload()` recarrega os `.lua` sem reiniciar o servidor.

## Licença e aviso

Código sob licença MIT (`LICENSE`).

Este é um projeto de fã, sem relação com a Wizards of the Coast. *Magic: The
Gathering*, os nomes das cartas e o texto de regras são propriedade da Wizards
of the Coast LLC. Nenhuma arte de carta é redistribuída aqui: a interface busca
as imagens da API pública do Scryfall em tempo de execução.
