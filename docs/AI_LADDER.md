# Escada de dificuldade da IA

Pedido do usuário: cinco níveis, do básico ao "campeão mundial".

| Nível | Tecnologia | Comportamento | Estado |
|---|---|---|---|
| Fácil | regras | básica | **existe** (`RandomBot` com viés simples) |
| Normal | heurística | estratégica | **existe** (`HeuristicBot`, 92% vs aleatório) |
| Difícil | MCTS | analisa possibilidades | a construir |
| Expert | MCTS + aprendizado | muito forte | a construir |
| Nightmare | RL + MCTS | extremamente forte | ver "o que é honesto prometer" |

## O que o motor já oferece de graça

O modelo de execução foi desenhado para busca, e isso não é acaso:

- `Game::legal_actions_for(&Request) -> Vec<Action>` — o espaço de ações já é
  enumerável, que é o pré-requisito de qualquer busca em árvore.
- Motor **síncrono com callback**: não há estado suspenso para restaurar.
- **Determinismo por semente**: mesma semente, mesma partida — então rollout é
  reprodutível e regressão de força é detectável.
- `GameState` é `Clone` e serializável, sem I/O e sem RNG dentro.

## As três coisas que faltam, em ordem

### 1. `Game::fork()` — o habilitador

Sem isto não há busca. `Game` carrega `Vec<Box<dyn Agent>>` (não clonável) e
`Arc<CardDatabase>` (clonagem barata). O fork clona o estado, compartilha o
catálogo e recebe agentes de rollout:

```rust
pub fn fork(&self, agents: Vec<Box<dyn Agent>>) -> Game
```

O RNG do fork precisa de semente **derivada**, não copiada: rollouts que
compartilham a sequência aleatória exploram todos o mesmo futuro.

### 2. Determinização — sem isso o bot trapaceia

MCTS ingênuo em jogo de informação oculta lê a mão real do oponente e joga contra
o futuro que ele *sabe* que vem. Fica forte por trapaça, não por análise, e o
número do benchmark mente.

O correto: antes de cada rollout, **amostrar** um estado consistente com o que o
bot observa — embaralhar as cartas desconhecidas (mão do oponente + biblioteca
dele) e distribuir. Rodar N determinizações e agregar.

`Observer::Player(p)` já define exatamente o que é conhecido. A regra prática:
**o bot só pode consultar `game.view(Observer::Player(eu))`**, nunca `state` cru.

### 3. Orçamento de busca em tempo, não em nós

Partida assistida precisa de ritmo. MCTS por número fixo de iterações fica lento
justo quando o tabuleiro está cheio — que é quando o espectador está prestando
atenção. Orçamento por milissegundo, com anytime: sempre há uma jogada pronta.

## Nível a nível

**Difícil — MCTS.** UCT com rollout guiado pelo `HeuristicBot` (rollout aleatório
puro em Magic é ruído: a partida quase nunca termina de forma informativa).
Determinização de 8 a 16 amostras. Orçamento ~200ms por decisão.

**Expert — MCTS + avaliação aprendida.** Trocar a heurística escrita à mão por um
avaliador treinado: vetor de características (diferença de vida, poder em campo,
cartas na mão, mana disponível, ameaça de morte, contagem de permanentes por
tipo) e pesos ajustados por auto-jogo. Modelo linear ou árvore rasa resolve, em
Rust puro, sem dependência de framework. Os pesos viram arquivo versionado.

**Nightmare — RL + MCTS.** É onde a honestidade importa: AlphaZero exige rede
política+valor treinada por auto-jogo em escala. Em Rust dá para fazer com
`candle` (puro Rust, sem libtorch). O que **não** dá para prometer é força de
campeão mundial: MTG tem espaço de ação enorme, informação oculta e um pool de
cartas que muda. O que dá para entregar é a **infraestrutura** — self-play,
armazenamento de partidas, laço de treino — e um bot mensuravelmente mais forte
que o Expert. Chamar de "campeão mundial" seria vender o que não se mede.

## O teto que ninguém nota

**A força da IA é limitada pelo que o motor entende.** Um bot perfeito jogando
com 172 cartas simples continua sendo um bot de 172 cartas simples. Profundidade
estratégica em Magic vem da interação entre cartas — e a interação só existe se
o IR souber expressá-la.

Consequência prática: **a escada da IA e a cobertura do `ORACLE_COVERAGE.md`
crescem juntas.** Investir em Nightmare com pool raso é otimizar o jogador de um
jogo sem profundidade.

## Como provar que cada degrau é mais forte que o anterior

Sem isto, "Nightmare" é um rótulo.

`crates/mtg-ai/benches` ou um binário `mtg-arena`: torneio round-robin, todos
contra todos, N partidas por par, sementes fixas, **decks trocados** entre os
lados para anular vantagem de deck. Saída: matriz de vitórias + Elo relativo.

Critério de aceite, e ele é duro: cada degrau precisa vencer o anterior com
**margem estatisticamente significativa** no mesmo orçamento de tempo. Degrau que
não vence o de baixo não entra na escada — vira nota no relatório.

Regressão de força vira teste: se uma mudança derruba o Elo do Expert, o teste
falha.

## Ordem de execução

1. `Game::fork()` + determinização + harness de torneio (a base, e o que mede)
2. MCTS → nível Difícil
3. Avaliador aprendido por auto-jogo → nível Expert
4. `bot_by_difficulty()` e seleção na interface
5. RL só depois que 1–4 estiverem medidos e o pool de cartas for fundo o bastante

**Bloqueio atual:** os passos 1 e 2 tocam `crates/mtg-ai/**` e
`crates/mtg-core/src/engine/mod.rs`, que estão sendo reescritos agora pelo
workflow de formatos/multijogador. Começar antes disso seria sobrescrever
trabalho em voo.
