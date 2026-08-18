# HANDOFF — MTGPRO (simulador automático de Magic)

**Atualizado:** 2026-08-18
**Diretório:** `C:\Users\gedasio.filho\OneDrive - Vertis Capital\Área de Trabalho\Tudo\Jogo Magic`
**Remoto:** https://github.com/GedasioSaga/MTGPRO — público, MIT, branch `main`

---

## O que é

Refatoração conceitual do Forge (`github.com/Card-Forge/forge`) para a arquitetura
pedida: **motor em Rust**, **UI em TypeScript/React**, **catálogo em banco**,
**arte via CDN**, e — pedido em 18/08 — **cartas escritas em Lua**.

Escopo travado: **só card game**, **só simulador automático** (bot vs bot, estilo
YGOPro). Não há input de jogo; o usuário assiste.

Modo de trabalho: `ultracode` + `gauntlet-loop` nível máximo + loop até AAA.

---

## Estado real (verificado no disco, não em relatório de agente)

> **Lição da rodada anterior:** o `ok: true` de um workflow significa que o agente
> retornou, **não** que ele escreveu o código. Um builder criou *stubs vazios nos
> arquivos dos outros* para conseguir compilar o próprio módulo. Sempre confira
> `wc -l` e o conteúdo antes de acreditar.

### Funciona, com prova

| Peça | Prova |
|---|---|
| `crates/mtg-script` — Lua sandboxed, DSL de cartas | `cargo test -p mtg-script` → **5/5 passam**, incluindo sandbox e round-trip Lua→`CardDef` |
| `crates/mtg-core` compila | `cargo build -p mtg-core` limpo (1 warning de import) |
| Lua 5.4 vendored compila nesta máquina | build de `mlua` com `lua54,vendored,serialize` OK |

### Implementado de verdade, ainda não testado ponta a ponta

- `engine/turn.rs` (~1050 linhas) — laço de turno, prioridade, `move_object`, `draw_card`
- `engine/cast.rs` (~1830 linhas) — ações legais, custos, mana
- `engine/viewgen.rs` (~550 linhas) — projeção para a UI
- `crates/mtg-db` — SQLite via rusqlite bundled
- `crates/mtg-ai`, `crates/mtg-server` — parciais
- `web/src/**` — design system, board, fx, ui parciais (workflow de UI morreu no limite)

### Stub vazio — em reconstrução pelo workflow `wf_401ec653-103`

`query.rs` · `layers.rs` · `stack.rs` · `triggers.rs` · `sba.rs` · `combat.rs` · `resolve.rs`

Retornam `None` / `false` / `vec![]`. **Sem eles o jogo não roda** — são as regras.

---

## Camada Lua (adicionada 18/08, a pedido)

`crates/mtg-script`:

- **Papel: autoria, não runtime.** O script roda uma vez no carregamento e
  *produz* a árvore de `Effect` (o IR). Em partida, o motor interpreta IR puro —
  zero chamada de Lua no laço quente. Facilidade de escrever sem custar
  performance nem determinismo.
- **Ponte via serde:** a tabela Lua é desserializada direto em `CardDef` pelo
  mesmo caminho do banco. Não há código de conversão para manter em dia.
- **Sandbox obrigatório:** `io`, `os`, `package`, `require`, `dofile`, `loadfile`,
  `load`, `loadstring`, `debug`, `setmetatable` são removidos do ambiente antes
  de qualquer chunk. Script de carta é conteúdo, e conteúdo vira contribuição de
  terceiro. Coberto por teste.
- **DSL em `crates/mtg-script/lua/prelude.lua`** — `card{}`, `deal_damage`,
  `etb`, `dies`, `activated`, `mana_ability`, `static_pt`, `token`, `modal`, etc.
  Alvos são 1-based em Lua e convertidos para 0-based na fronteira.
- **Hot reload:** `CardScriptHost::reload()` recarrega os `.lua` sem reiniciar.

Carta hoje se escreve assim:

```lua
card {
  name = "Lightning Bolt", cost = "{R}", type = "Instant",
  rarity = "Common", set = "LEA",
  text = "Lightning Bolt deals 3 damage to any target.",
  targets = { t_any() },
  effect = deal_damage(3),
}
```

Catálogo em `cards/*.lua` (por cor), carregado por `crates/mtg-cards`.

---

## Decisões de arquitetura (não reabrir sem motivo)

1. **Carta é dado, não código.** `CardDef` é árvore de `Effect` serializável; Lua
   é a camada de autoria em cima disso.
2. **Arena de índices.** Todo objeto é `ObjectId`; grafo aura → criatura →
   controlador não fecha com `&mut` em Rust.
3. **Motor síncrono com callback.** `Agent::decide` responde na hora; sem máquina
   de estado suspensa. Justificativa: simulador automático não espera humano.
4. **UI só desenha.** Recebe `GameView` redigido por `Observer` e `MatchEvent`
   com `suggestedDurationMs`. Nenhuma regra vive no TypeScript.
5. **SQLite bundled, schema portável para PostgreSQL.** Não há `psql` nem
   `sqlite3` nesta máquina.
6. **Arte via Scryfall**, `art_key` = nome exato da carta, com fallback
   procedural obrigatório (gradiente por hash) — nunca caixa quebrada.
7. **Sem `wasm-pack`.** Caminho atual é servidor nativo + WebSocket.

### Bugs do contrato já corrigidos

- `Filter::HasKeyword(Keyword)` ↔ `Keyword::Enchant(Filter)` fazia ciclo de
  tamanho infinito (E0072). Corrigido com `Enchant(Box<Filter>)`.
- `Keyword` perdeu o derive `Hash` (consequência do `Box<Filter>`, que exigiria
  `Filter: Hash`). Nada dependia disso.

---

## Protocolo motor ↔ UI (quebrar isso quebra os dois lados)

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
JSON em camelCase. `MatchEvent` usa tag interna `"type"`.

---

## Workflows

| Run ID | O que era | Resultado |
|---|---|---|
| `wf_b256501a-a2e` | motor, 10 builders + integração | 5 retornaram, 6 morreram no limite de sessão; integração nunca rodou; **stubs vazios plantados** |
| `wf_0e9383c5-e1c` | cliente, 5 builders + integração | todos morreram no limite; arquivos parciais no disco |
| `wf_401ec653-103` | **em execução** — 7 módulos stub + catálogo Lua + integração | — |

Retomar qualquer um: `Workflow({scriptPath: "<script>", resumeFromRunId: "<run id>"})`.
Scripts em `...\workflows\scripts\`. Antes de diagnosticar resultado vazio, ler
`journal.jsonl` do transcript dir.

---

## Gauntlet — estado do protocolo

- **Tier:** T3.
- **Bar do motor:** `Card-Forge/forge` + Comprehensive Rules. Metade mensurável =
  pass rate de `docs/RULES_TESTS.md` (65 itens) + tempo de partida.
- **Bar da UI:** cliente de partida do MTG Arena. `docs/bar/arena-board-01.jpg` é
  a referência limpa (as outras 3 têm overlay promocional). **Fora do git** —
  screenshot é da WotC, não se redistribui em repo público.
- **Rodadas de julgamento:** ainda **nenhuma**. Critics entram quando
  `cargo test --workspace` passar e a UI construir.
- **Parada:** vence a cega, seca (2 rodadas sem fechar gap), ou o usuário manda parar.

---

## Próximos passos, em ordem

1. Aguardar `wf_401ec653-103`. **Conferir no disco** (`wc -l`, `head`) se os 7
   módulos deixaram de ser stub — não confiar no `ok`.
2. Teste de fumaça `crates/mtg-core/tests/smoke.rs`: partida completa termina sem
   pânico, sementes 0..20. É o primeiro sinal de que o jogo existe.
3. Implementar `tests/interactions.rs` a partir de `docs/RULES_TESTS.md` (65 itens).
   Precisa de um `FixedAgent` com fila de `Action`.
4. Retomar o workflow de UI (`wf_0e9383c5-e1c`) e fazer `tsc` + `npm run build` passar.
5. Rodar ponta a ponta: `cargo run -p mtg-server` + `npm run dev`, confirmar no
   browser que a partida anima até o fim.
6. **Gauntlet rodada 1:** critic cego de UI (nossa mesa vs `arena-board-01.jpg`,
   labels removidos, veredito binário, um único maior gap) + `verificador-realidade`
   sobre o pass rate + `revisor` por dimensão.
7. Diagrama de arquitetura em `docs/diagrams/` a partir do grafo.

---

## Armadilhas conhecidas nesta máquina

- **Heredoc longo no Bash trunca** (~200+ linhas) com
  `unexpected EOF while looking for matching '`. Arquivo grande → tool `Write`.
- **`npm create vite` é interativo** e aborta sem stdin. Use
  `CI=1 npx --yes create-vite@latest web --template react-ts` com o diretório
  **inexistente**.
- **Sem `wasm-pack`, `psql`, `sqlite3` no PATH.** Único target Rust:
  `x86_64-pc-windows-msvc`.
- **Caminho tem espaço e acento** (`Área de Trabalho`) — sempre entre aspas.
- **`target-probe/`** foi commitado por engano (78 MB, 268 arquivos) e depois
  destrancado; o histórico ainda carrega, `.git` está em ~51 MB. Já no `.gitignore`.
- **Limite de sessão mata agente no meio.** Trabalho parcial fica no disco, mas o
  workflow reporta falha. Sempre inspecionar antes de refazer.

---

## Grafo (graphify)

- `graphify-out/graph.json`, construído com `--code-only` (obrigatório: não manda
  doc/PDF/imagem para LLM). **Fora do git** — é derivado.
- Hooks `post-commit` e `post-checkout` instalados em `.git/hooks/`: rodam
  `graphify update` em background a cada commit, log em
  `graphify-out/.last-update.log`. Não bloqueiam o commit.
- Staleness: comparar `built_at_commit` do `graph.json` com `git rev-parse HEAD`.

```bash
G='.../graphify-out/graph.json'
graphify query "quem chama layers::characteristics" --graph "$G" --budget 1200
graphify god-nodes --graph "$G"
graphify path "Game::run" "sba::check" --graph "$G"
```

Último build: 841 nodes, 2120 edges, 31 communities, commit `a97d121`.

---

## Como retomar do zero

1. `cd` para o diretório (entre aspas).
2. Ler este arquivo, depois `docs/ENGINE_CONTRACT.md`.
3. `git log --oneline | head` e `git status`.
4. `cargo check --workspace --all-targets 2>&1 | head -40` — a verdade sobre o motor.
5. `for f in query layers stack triggers sba combat resolve; do wc -l crates/mtg-core/src/engine/$f.rs; done`
   — se algum tiver < 50 linhas, ainda é stub.
6. `cd web && npx tsc --noEmit` — a verdade sobre o cliente.
7. `graphify update "<repo>"` se o grafo estiver velho.
8. Retomar em "Próximos passos".

Se algo aqui contradiz o código, **o código ganha**.
