# HANDOFF — MTGPRO (simulador automático de Magic)

**Atualizado:** 2026-08-19
**Diretório:** `C:\Users\gedasio.filho\OneDrive - Vertis Capital\Área de Trabalho\Tudo\Jogo Magic`
**Remoto:** https://github.com/GedasioSaga/MTGPRO — público, MIT, branch `main`

---

## Como retomar em 60 segundos

```bash
cd "C:\Users\gedasio.filho\OneDrive - Vertis Capital\Área de Trabalho\Tudo\Jogo Magic"
git log --oneline | head -10
cargo test --workspace                    # esperado: 504 passando, 0 falhando
cargo run -p mtg-server                   # http://127.0.0.1:8787
cd web && npm run dev                     # http://localhost:5173
```

Se algo aqui contradiz o código, **o código ganha**. Isto é retrato, não verdade.

---

## O que é

Refatoração conceitual do Forge para: **motor em Rust**, **UI React**, **cartas em
Lua**, **catálogo do Scryfall**. Escopo: só card game, só simulador automático
(bots jogam, o usuário assiste), no espírito do YGOPro.

Modo de trabalho: `ultracode` + `gauntlet-loop` + fan-out multi-agente.

---

## Estado, com números verificados por execução

| Métrica | Valor |
|---|---|
| Testes | **504 passando, 0 falhando**, 5 ignorados (27 suítes) |
| Interações de regra | **93/93** (`docs/RULES_TESTS.md`, inclui seção 9 de multijogador) |
| Catálogo | **32.452 cartas**, **4.951 jogáveis (15,2%)** |
| Pauper jogável | **3.173 (30,5%)** · Standard 574 (11,8%) · Modern 3.862 (17,3%) |
| Cartas curadas em Lua | 174, sempre jogáveis |
| Fuzzing duelo | 200 sementes, **0 pânicos** |
| Fuzzing 4 jogadores | 100 mesas, **100 vencedores, 0 empates, 0 objetos órfãos** |
| IA duelo | heurístico **46/50 (92%)** vs aleatório |
| IA mesa de 4 | heurístico **34/50 (68%)** vs 3 aleatórios (acaso = 25%) |
| Cliente | `tsc` 0 erros · build limpo · ~85 fps a 1920×1080 |

**Bugs de regra achados e corrigidos**, cada um com CR citada: 603.6d
(informação de última existência), 702.16c (proteção não prevenia dano fora de
combate), 611.2b (efeito não expirava com a fonte fora do campo), e um de
multijogador onde `expire_continuous_effects` usava módulo cru enquanto
`advance_turn` usava `next_alive` — divergiam só com 3+ jogadores, deixando
efeito `YourNextTurn` permanente.

---

## Arquitetura

```
cards/*.lua ──► mtg-script (mlua sandboxed) ──┐
                                              ├──► CardDef (IR) ──► SQLite
Scryfall bulk ──► mtg-import ──► mtg-oracle ──┘                    (data/catalog.db)
                                              │
                                    ┌─────────▼─────────┐
                                    │     mtg-core      │  camadas 613, pilha,
                                    │                   │  gatilhos, SBA 704,
                                    │                   │  combate 506-511, CR 903
                                    └───┬───────────┬───┘
                                 mtg-ai │           │ mtg-server (axum + WS)
                                        │           │        │
                                   mtg-format ──────┘        ▼
                                   (legalidade)         web/ React 19
```

### Decisões que não devem ser reabertas sem motivo

1. **Carta é dado, não código.** `CardDef` é árvore de `Effect` serializável.
   Lua roda uma vez no carregamento e *produz* o IR; em partida o motor
   interpreta IR puro — zero Lua no laço quente.
2. **Sandbox no Lua é requisito, não zelo.** `io`, `os`, `package`, `require`,
   `load` saem do ambiente antes de qualquer chunk.
3. **Arena de índices.** Todo objeto é `ObjectId`; grafo aura→criatura→controlador
   não fecha com `&mut` em Rust.
4. **Motor síncrono com callback.** `Agent::decide` responde na hora, sem estado
   suspenso — é o que evitou a classe de bug mais cara de motor de card game.
5. **UI só desenha.** Recebe `GameView` redigido por `Observer` + `MatchEvent`.
6. **`players: Vec<PlayerState>` desde o contrato inicial** — é por isso que
   Commander 2-4 foi preenchimento de comportamento, não reescrita do motor.
7. **Impressão ≠ carta.** `oracle_id` é regra (motor); `printing_id` é arte
   (UI/deck). Ver `docs/DECK_EDITOR.md`.

---

## Onde está cada coisa

| Caminho | O que é |
|---|---|
| `crates/mtg-core` | regras, IR, estado, view |
| `crates/mtg-script` | Lua sandboxed + DSL (`lua/prelude.lua`) |
| `crates/mtg-cards` | catálogo curado + decks |
| `crates/mtg-db` | SQLite (`CardStore`, `ImportedCard`, `search_page`, `stats`) |
| `crates/mtg-import` | bulk do Scryfall → SQLite; **tem compilador próprio** |
| `crates/mtg-oracle` | segundo compilador + `layouts` + `coverage` |
| `crates/mtg-format` | validação de deck por formato |
| `crates/mtg-ai` | `random`, `heuristic`, `greedy` |
| `crates/mtg-server` | axum + WS; `sim.rs` roda 2-4 assentos |
| `cards/*.lua` | catálogo curado, um arquivo por cor |
| `web/src` | React 19 + Vite + Tailwind 4 + Motion |

**Docs que valem ler antes de mexer:** `ENGINE_CONTRACT.md` (assinaturas +
protocolo) · `RULES_TESTS.md` (93 itens) · `ORACLE_COVERAGE.md` (o que falta, por
frequência) · `CARD_FACTORY.md` (estratégia para cobrir tudo) · `AI_LADDER.md`
(MCTS→RL) · `DECK_EDITOR.md` · `UI_BAR_SPEC.md`

---

## PRÓXIMOS PASSOS, em ordem de valor

### 1. Capacidades de IR faltando — fila priorizada por frequência real

De `docs/ORACLE_COVERAGE.md` (27 capacidades):

| Cartas | Capacidade | Nota |
|---|---|---|
| **882** | layouts multiface | split, adventure, DFC |
| **753** | `Filter::Attached` | aura e equipamento |
| **589** | variantes de `Keyword` | devoid, changeling, infect |
| **563** | **motor ler `Ability::Replacement`** | **existe no IR, ZERO uso em `engine/`** |
| 376 | olhar-e-escolher no topo | |
| 366 | `SearchLibrary` com destino ≠ mão | |
| 355 | `TokenSpec` com habilidades | |
| 304 | custo adicional de lançamento | |
| 269 | `Effect::Reveal` | não existe no IR |
| 191 | regeneração | |

`Ability::Replacement` é o mais revelador: o **tipo existe desde o contrato
inicial e nenhum módulo do motor o consome**. Tipo declarado não é capacidade
implementada. Vale varrer o IR atrás de outras variantes órfãs.

### 2. Unificar os dois compiladores — dívida nomeada

`mtg-import/src/compile.rs` e `mtg-oracle` resolvem o mesmo problema em ~5.500
linhas. Concordam em 3.443 cartas, **divergem em 388**. A segunda passada
(`oracle_second_pass`) é ponte, não fusão. Manutenção dupla com risco de
divergirem em silêncio.

### 3. Fábrica de cartas — Fases 1 e 2 de `CARD_FACTORY.md`

Fase 1: manifesto durável + escada de verificação de 6 degraus.
Fase 2: **o portão** — medir a concordância do gerador LLM contra as 4.951 cartas
onde já sabemos a resposta certa. Concordância baixa **não** autoriza a fábrica.

### 4. Escada de IA — `AI_LADDER.md`

`Game::fork()` com **semente derivada** (não copiada; senão todos os rollouts
exploram o mesmo futuro) + **determinização** (senão o bot lê a mão oculta e fica
forte por trapaça) + orçamento em ms. Depois MCTS.

### 5. Editor de decks — `DECK_EDITOR.md`

Tabela `printings`, busca estilo Scryfall com `is:playable`, import/export `.dck`.

---

## Dívidas e defeitos conhecidos — escolha, não surpresa

- **Mesa de 3-4 jogadores é visualmente crua**: quadrantes simples, sem tapete
  nem perspectiva. `StatsPanel` está fixo em 2 assentos — **mente numa mesa de 4**.
- **Só 2 decks de Commander.** Com 2 decks numa mesa de 4, assentos repetem, e a
  métrica "vitórias por assento" (`[47, 7, 42, 4]`) mede força de deck, não
  posição na mesa.
- **Multijogador só em Commander** (decisão em `AppState::seat_range`). O motor
  roda 4 em qualquer formato se a regra mudar.
- `GameState::player/zone` indexam direto e entram em pânico com `PlayerId` fora
  de faixa. Inalcançável hoje, mas viola a regra do projeto num ponto quente.
- `web/tsconfig` **não tem `"strict": true`** em lugar nenhum.
- 27 warnings de clippy pré-existentes em `mtg-core` (só estilo).
- Importação foi de 2,7s para 5,6s (a 2ª passada recompila as recusadas).

---

## Gauntlet — placar da UI

Bar: `docs/bar/arena-ixalan-board.png` (fora do git: captura da WotC).

| Rodada | Gap nomeado | Estado |
|---|---|---|
| 1 | painéis de telemetria dominando a largura | fechado |
| 2 | cartas flutuando sem superfície | fechado |
| 3 | cartas em jogo ilegíveis | fechado |
| 4 | setas de combate cruzando a tela | fechado |
| 5-6 | **pêndulo**: caixa demais ↔ zona nenhuma | seca declarada |
| — | usuário trouxe bar melhor + ideia do **playmat** | pêndulo quebrado |
| 7 | falta perspectiva; outline lê como debug | atacado na rodada 8 |
| 8 | perspectiva aplicada (`--mat-tilt: 9deg`) | **não julgada** |

**Pendência:** a rodada 8 aplicou perspectiva mas o agente morreu no limite antes
de medir o alinhamento de combate. Antes era **758,3/758,3 com desvio 0px**.
Perspectiva é exatamente o que pode quebrar isso (`getBoundingClientRect` passa a
devolver caixa projetada). **Medir antes de julgar de novo.**

---

## Armadilhas desta máquina — leia antes de perder uma hora

- **`npx tsc --noEmit` NÃO checa nada aqui.** `tsconfig.json` só tem
  `references`, sem `files`. Use `npx tsc -p tsconfig.app.json --noEmit`.
- **Apague `web/node_modules/.tmp/*.tsbuildinfo` antes de checar tipos.** O cache
  incremental já reportou "sem erros" sobre árvore antiga.
- **Heredoc longo no Bash trunca** (~200+ linhas) com `unexpected EOF`. Arquivo
  grande → tool `Write`.
- **Medição de FPS no browser mede o Chrome**, não o app, se a janela estiver
  ocluída (throttle a 1 Hz). Reafirme `Page.setWebLifecycleState: active`.
- **Confirme o viewport antes de capturar.** Já capturei em 3840×2160 achando que
  era 1920×1080 e quase reportei bug inexistente.
- **`cargo build` falha com "Acesso negado"** se o `.exe` estiver rodando:
  `taskkill //F //IM mtg-server.exe`.
- **Caminho do projeto tem espaço e acento** — sempre entre aspas. Caminho estilo
  Git Bash (`/c/Users/...`) **não funciona** em variável lida pelo Rust.
- **Limite de sessão mata agente no meio.** O trabalho parcial fica no disco mas o
  workflow reporta falha. **Inspecionar o disco antes de refazer**, e retomar com
  `Workflow({scriptPath, resumeFromRunId})`.
- **`ok: true` de workflow não significa código no disco.** Confira com `wc -l`.

---

## A lição que se repetiu duas vezes

**A costura entre crates é onde o trabalho paralelo falha.** Aconteceu com
import↔db (a importação gravava 32 mil cartas num arquivo que o servidor nunca
abria, e `/api/stats` respondia 174) e com import↔oracle (1.786 linhas de
compilador viraram código morto). Nos dois casos cada agente entregou um crate
correto e testado, e ninguém era dono da fronteira.

**A defesa que funcionou nas duas vezes foi a mesma:** uma rota que reporta o
estado real de ponta a ponta. `/api/stats` denunciou o "174" e depois o "3.831
travado". Endpoint de estatística é barato e paga sozinho.

---

## Grafo (graphify)

`graphify-out/graph.json`, `--code-only`, fora do git. Hooks `post-commit` e
`post-checkout` reconstroem em background a cada commit. Staleness: comparar
`built_at_commit` com `git rev-parse HEAD`.

```bash
G='.../graphify-out/graph.json'
graphify query "quem chama layers::characteristics" --graph "$G" --budget 1200
graphify god-nodes --graph "$G"
```
