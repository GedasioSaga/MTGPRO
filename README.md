# Arena Automata — simulador automático de Magic

Motor de regras de *Magic: The Gathering* em Rust, com cliente web em React.
Duas IAs jogam; você assiste. Nada de input de jogo — é um simulador, no espírito
do YGOPro: partida completa, regras corretas, reprodução com ritmo e animação.

## Arquitetura

```
                    ┌──────────────────────┐
                    │   Catálogo de cartas │   SQLite (rusqlite bundled),
                    │  JSON do CardDef +   │   schema portável p/ PostgreSQL
                    │  colunas indexadas   │
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

- **Carta é dado, não código.** `CardDef` é uma árvore de `Effect` serializável
  (`crates/mtg-core/src/ir.rs`). Carta nova é linha no banco, não recompilação.
- **Arena de índices em vez de referências.** Todo objeto é `ObjectId`; aura que
  aponta para criatura que aponta para controlador é um grafo, e grafo com `&mut`
  em Rust não fecha.
- **Motor síncrono com callback.** O simulador é automático, então o bot responde
  na hora: sem máquina de estado suspensa, que é onde motor de card game acumula bug.
- **UI só desenha.** O cliente recebe `GameView` já redigido e `MatchEvent` com
  duração sugerida. Nenhuma regra de Magic vive no TypeScript.

## Estrutura

| Caminho | O que é |
|---|---|
| `crates/mtg-core` | regras, estado, IR de efeitos, projeção para a UI |
| `crates/mtg-cards` | catálogo de cartas e decks, escritos no IR |
| `crates/mtg-db` | persistência SQLite do catálogo |
| `crates/mtg-ai` | bots (`random`, `heuristic`, `greedy`) |
| `crates/mtg-server` | HTTP + WebSocket, roda a simulação e transmite |
| `web` | cliente React 19 + Vite + Tailwind 4 + Motion |
| `docs/ENGINE_CONTRACT.md` | assinaturas obrigatórias e protocolo de rede |

## Rodando

```bash
cargo run -p mtg-server          # http://127.0.0.1:8787
cd web && npm run dev            # http://localhost:5173
```

O cliente funciona sem o servidor: cai para uma partida de demonstração embutida.

## Verificação

```bash
cargo test --workspace           # regras + interações
cargo clippy --workspace --all-targets
cd web && npx tsc --noEmit && npm run build
```
