# HANDOFF — Arena Automata (simulador automático de Magic)

**Atualizado:** 2026-08-17
**Diretório:** `C:\Users\gedasio.filho\OneDrive - Vertis Capital\Área de Trabalho\Tudo\Jogo Magic`
**Repo git:** sim (inicializado nesta sessão). Branch `master`.

---

## O que é

Refatoração conceitual do Forge (`github.com/Card-Forge/forge`) para a arquitetura
pedida pelo usuário: **motor de regras em Rust**, **UI em TypeScript/React**,
**catálogo em banco**, **arte via CDN**.

Escopo travado pelo usuário: **só a parte de card game**, e **só um simulador
automático** (bot vs bot, estilo YGOPro). Não há input de jogo — o usuário assiste.

Modo de trabalho combinado: `ultracode` + `gauntlet-loop` no nível máximo +
`/loop` até estar funcional, perfeito, bonito, AAA. Um subagente por peça.

---

## Estado atual

### Feito e commitado

| Commit | Conteúdo |
|---|---|
| `6dc17d9` | contrato do motor (tipos, IR, view, esqueleto) |
| `a97d121` | docs: contrato, suíte de interações, README, bar visual |

Escrito no **main thread** (é o spine que todos os builders compartilham — não
delegado de propósito):

- `crates/mtg-core/src/ids.rs` — `ObjectId`/`PlayerId`/`CardDefId`, `IdGen` monotônico
- `crates/mtg-core/src/mana.rs` — `Color`, `ColorSet`, `ManaSymbol`, `ManaCost`, `ManaPool`
- `crates/mtg-core/src/types.rs` — `CardType`, `Supertype`, `TypeLine`, `CounterKind`
- `crates/mtg-core/src/zone.rs` — `ZoneKind`, `ZoneId`, `Zone`
- `crates/mtg-core/src/ir.rs` — **IR de efeitos** (~50 variantes de `Effect`),
  `Filter`, `Selector`, `Value`, `Condition`, `Keyword`, `Cost`, `StaticModRuntime`
- `crates/mtg-core/src/card.rs` — `CardDef`, `Ability`, `TriggerCondition`, `CardDatabase`
- `crates/mtg-core/src/event.rs` — `GameEvent`, `Step`, `Phase`, `Defender`
- `crates/mtg-core/src/action.rs` — `Request`, `Action`, `TargetChoice`, `ActionError`
- `crates/mtg-core/src/state.rs` — `GameState`, `ObjectState`, `StackItem`, `ContinuousEffect`
- `crates/mtg-core/src/view.rs` — `GameView`, `CardView`, **`MatchEvent`** (contrato da UI)
- `crates/mtg-core/src/engine/mod.rs` — `Game`, trait `Agent`, `Characteristics`, `Game::ask`
- `docs/ENGINE_CONTRACT.md` — **assinaturas obrigatórias** de cada submódulo + protocolo WS
- `docs/RULES_TESTS.md` — 65 interações nomeadas com a regra oficial citada
- `docs/bar/` — 4 screenshots reais do MTG Arena + `SOURCES.md`
- `README.md`, `.gitignore`
- `web/` — Vite scaffold com React 19.2, TS 6, Vite 8, Tailwind 4.3, Motion 13, Zustand 5

### Em execução quando este handoff foi escrito

| Frente | Run ID | Agentes | Entrega |
|---|---|---|---|
| Workflow 1 — motor | `wf_b256501a-a2e` | 10 builders + 1 integrador | `engine/{query,layers,turn,viewgen,stack,triggers,sba,cast,combat,resolve}.rs`, crates `mtg-cards`, `mtg-db`, `mtg-ai`, `mtg-server` |
| Workflow 2 — cliente | `wf_0e9383c5-e1c` | 5 builders + 1 integrador | `web/src/{design,components,state,net,fx,mock,types}` |

Scripts persistidos (para retomar sem reenviar):
- `...\workflows\scripts\mtg-engine-foundation-wf_b256501a-a2e.js`
- `...\workflows\scripts\mtg-client-aaa-wf_0e9383c5-e1c.js`

Retomar: `Workflow({scriptPath: "<caminho>", resumeFromRunId: "<run id>"})`.
Antes de diagnosticar resultado vazio, ler `journal.jsonl` do transcript dir.

---

## Decisões de arquitetura (não reabrir sem motivo)

1. **Carta é dado, não código.** `CardDef` é árvore de `Effect` serializável.
   Carta nova = linha no banco. Preço: o interpretador (`engine/resolve.rs`)
   precisa cobrir o vocabulário inteiro.
2. **Arena de índices.** Todo objeto é `ObjectId`; grafo de objetos (aura →
   criatura → controlador) não fecha com `&mut` em Rust.
3. **Motor síncrono com callback.** `Agent::decide` responde na hora; não há
   máquina de estado suspensa. Justificativa: o simulador é automático, então a
   restrição que obriga continuação (esperar humano) não existe. Se um dia
   houver jogo interativo, o `Agent` vira um que bloqueia num canal.
4. **UI só desenha.** Recebe `GameView` já redigido por `Observer` e `MatchEvent`
   com `suggestedDurationMs`. Nenhuma regra vive no TypeScript.
5. **SQLite bundled (`rusqlite`), schema portável para PostgreSQL.** Não há
   binário `sqlite3` nem `psql` nesta máquina; bundled compila o SQLite junto.
6. **Arte via Scryfall.** `art_key` = nome exato da carta; a UI monta
   `https://api.scryfall.com/cards/named?exact=...&format=image&version=art_crop`,
   com fallback procedural obrigatório (gradiente por hash do nome).
7. **Sem `wasm-pack` nesta máquina.** O caminho é servidor nativo + WebSocket.
   WASM fica para depois, se pedido.

---

## Protocolo entre motor e UI (contrato — quebrar isso quebra os dois lados)

WebSocket `/ws/match` em `127.0.0.1:8787`:

```jsonc
// servidor -> cliente
{ "type": "init",   "view": GameView, "players": ["Bot A","Bot B"], "seed": 123 }
{ "type": "events", "events": [MatchEvent, ...], "view": GameView }
{ "type": "done",   "outcome": GameOutcome, "turns": 14, "durationMs": 812 }
// cliente -> servidor
{ "type": "start", "deckA": "burn", "deckB": "elves", "seed": 123, "speed": 1.0 }
{ "type": "pause" } | { "type": "resume" } | { "type": "step" }
```

REST: `GET /api/health`, `GET /api/cards`, `GET /api/decks`.
JSON em camelCase (`serde rename_all`). `MatchEvent` usa tag interna `"type"`.

---

## Gauntlet — estado do protocolo

- **Tier:** T3 (feature multi-arquivo com UI).
- **Bar do motor:** `Card-Forge/forge` + Comprehensive Rules; metade mensurável =
  pass rate de `docs/RULES_TESTS.md` (65 itens) + tempo de partida.
- **Bar da UI:** cliente de partida do MTG Arena — `docs/bar/arena-board-01.jpg`
  é a referência limpa (as outras 3 têm overlay promocional).
- **Rodadas de julgamento:** ainda **nenhuma**. Os critics cegos entram depois
  que Workflow 1 e 2 fecharem.
- **Regra de parada:** vence a cega, ou seca (2 rodadas sem fechar gap), ou o
  usuário manda parar. Nunca por contagem fixa de rodadas.

---

## Próximos passos, em ordem

1. **Aguardar** Workflow 1 e 2. Ler os relatórios de integração (`cargo check`,
   `tsc --noEmit`, `npm run build`) — números reais, não auto-relato.
2. **Implementar `crates/mtg-core/tests/interactions.rs`** a partir de
   `docs/RULES_TESTS.md`. Precisa de um `FixedAgent` de teste (fila de `Action`
   pré-programada). Delegar a `testador`.
3. **Rodar a partida ponta a ponta**: `cargo run -p mtg-server` + `npm run dev`,
   confirmar no browser (Playwright) que a partida anima até o fim.
4. **Gauntlet rodada 1:**
   - critic cego de UI: screenshot da nossa mesa vs `arena-board-01.jpg`,
     labels removidos, veredito binário, um único maior gap
   - critic de regras: `verificador-realidade` sobre o pass rate da suíte
   - critic de código: `revisor` em paralelo por dimensão
5. **Loop** no maior gap até vencer ou secar.
6. **Diagrama de arquitetura** em `docs/diagrams/` a partir do grafo (§2g).

---

## Armadilhas conhecidas nesta máquina

- **Heredoc longo no Bash trunca.** `cat > f <<'EOF'` com ~200+ linhas quebra com
  `unexpected EOF while looking for matching '`. Usar a tool `Write` para arquivo
  grande; heredoc só para trecho curto.
- **`npm create vite` é interativo** e aborta sem stdin. Usar
  `CI=1 npx --yes create-vite@latest web --template react-ts` com o diretório
  **inexistente** (ele recusa diretório não vazio).
- **Sem `wasm-pack`, sem `psql`, sem `sqlite3` no PATH.** Target Rust instalado:
  só `x86_64-pc-windows-msvc`.
- **Caminho do projeto tem espaço e acento** (`Área de Trabalho`). Sempre entre
  aspas em qualquer comando.

---

## Grafo (graphify)

- Grafo em `graphify-out/graph.json`, construído com `--code-only` (obrigatório
  em repo Vertis: não manda doc/PDF/imagem para LLM).
- Reconstruir/atualizar: `graphify update "<repo>"`.
- Checar staleness: comparar `built_at_commit` do `graph.json` com
  `git rev-parse HEAD`. Diferente = atualizar antes de confiar.
- Hooks `post-commit` e `post-checkout` de auto-rebuild: **ver seção abaixo**.
