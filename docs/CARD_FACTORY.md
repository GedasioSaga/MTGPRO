# Fábrica de cartas — estratégia para cobrir o catálogo inteiro

Decisão do usuário: **caminho B**. Scripts Lua próprios, licença MIT preservada,
sem derivar do `cardsfolder` do Forge (GPL-3.0). Meta: todas as cartas.
Prazo: dias, se for preciso.

## O que o Forge nos ensina

`forge-gui/res/cardsfolder/l/lightning_bolt.txt`:

```
Name:Lightning Bolt
ManaCost:R
Types:Instant
A:SP$ DealDamage | ValidTgts$ Any | NumDmg$ 3 | SpellDescription$ ...
Oracle:Lightning Bolt deals 3 damage to any target.
```

O Forge **não interpreta o texto de oracle**. A linha `Oracle:` é documentação;
quem manda é a linha `A:`. São ~27 mil arquivos escritos à mão, em 15 anos, por
uma comunidade. Não há truque de parsing — há um DSL bom e muito trabalho humano.

Nossa camada Lua é o par direto disso. A diferença é a alavanca: eles precisavam
de **um humano por carta**; nós precisamos de **um humano por lote que falha**.

## O problema real não é gerar. É verificar.

Um agente escreve 28 mil scripts Lua sem dificuldade. Escrever 28 mil scripts
**corretos** é outra coisa. Carta marcada jogável que se comporta diferente do
texto quebra a partida em silêncio — e em escala, envenena o catálogo inteiro
sem deixar rastro.

Então a fábrica é desenhada ao contrário: **primeiro a verificação, depois a
geração**.

## A vantagem que temos: um gabarito de 3.831 cartas

O compilador determinístico já resolve 3.831 cartas, e o resultado dele é
confiável por construção (fidelidade acima de cobertura, testado carta a carta).

Isso é um **conjunto de validação rotulado, de graça**:

> Antes de confiar no gerador em carta desconhecida, medir a concordância dele
> com o compilador nas 3.831 conhecidas.

Se concordar em 99%, temos evidência de que ele entende o IR. Se concordar em
80%, não temos fábrica — temos gerador de dívida. **Este número é o portão.**

## Escada de verificação, do barato ao caro

Toda carta gerada passa por todos os degraus. Falhou em qualquer um, não entra.

| # | Verificação | Pega |
|---|---|---|
| 1 | **Estrutural**: nome, custo, linha de tipo, P/T, raridade batem com o Scryfall byte a byte | alucinação de estatística, erro de digitação |
| 2 | **Contagem de habilidades**: nº de habilidades no IR = nº de linhas de habilidade no oracle | habilidade esquecida, habilidade inventada |
| 3 | **Consistência léxica**: cada verbo do oracle tem efeito correspondente no IR, e vice-versa. "destroy" sem `Destroy`, ou `DrawCards` sem "draw", reprova | efeito trocado ou omitido |
| 4 | **Números**: todo numeral do texto aparece no IR, e todo `Const` do IR aparece no texto | 3 de dano virando 2 |
| 5 | **Execução**: monta partida sintética, lança/ativa a carta, afirma que o efeito declarado aconteceu de fato (vida mudou, carta comprada, permanente destruído) e que não houve pânico | IR que compila e não faz nada |
| 6 | **Diferencial** (só para as 3.831 conhecidas): compara com a saída do compilador determinístico | mede a acurácia do gerador |

Os degraus 1–4 são estáticos e rodam em milissegundos. O 5 é o caro. O 6 é o
que autoriza a fábrica a existir.

## Três populações, três tratamentos

O catálogo travado não é homogêneo. Misturar os três é o erro que faz a fábrica
patinar.

**(A) O IR já expressa, o compilador não reconhece.**
Tratamento: parser. Zero custo de LLM. É o que a rodada em execução ataca.

**(B) O IR não expressa.**
Tratamento: **estender o IR primeiro**. Nenhum volume de Lua resolve — se não há
variante de `Effect` para "custo adicional", não há como escrever a carta.
Priorização por frequência **ponderada por formato**: destravar 200 cartas de
Un-set vale menos que destravar 80 de Pauper.

**(C) O IR expressa, mas o padrão é único ou raro.**
Tratamento: a fábrica. É aqui que o LLM ganha do parser.

## Clusterização por template — o multiplicador

Muitas cartas diferem só em número, cor ou subtipo. "Target creature gets +2/+2
until end of turn" e "+3/+3" são a **mesma** carta com parâmetro diferente.

Então a fábrica não decide carta a carta:

1. Normaliza o oracle (nome → `~`, números → `N`, cores e subtipos → parâmetro)
2. Agrupa por texto normalizado
3. Para cada cluster, o agente escreve **um gerador de template** uma vez
4. O template é aplicado mecanicamente aos membros do cluster
5. Cada membro passa pela escada de verificação individualmente

Efeito prático: as ~28.893 cartas viram alguns milhares de decisões de template,
não 28.893 decisões de carta. E o template errado falha em bloco, o que é fácil
de ver — bem melhor que 50 cartas erradas espalhadas.

## Ordem de ataque: por valor, não alfabética

| Prioridade | Pool | Por quê |
|---|---|---|
| 1 | **Pauper** | pool pequeno e coerente; é onde dá para chegar perto de 100% e ter decks reais jogáveis primeiro |
| 2 | **Standard** | pool pequeno, cartas modernas, mecânicas atuais |
| 3 | **Modern** | grande, mas é onde mora a maioria do que se joga |
| 4 | Comuns e incomuns de qualquer era | a espinha de qualquer deck |
| 5 | O resto | inclusive as que só existem por completude |

Um catálogo com Pauper 100% jogável vale mais que um catálogo com 40% espalhado
por todo lado, porque o primeiro **permite jogar**.

## Estado durável — a fábrica atravessa sessões

Dias de trabalho e limites de sessão já nos derrubaram várias vezes. A fábrica
precisa retomar de onde parou, sem refazer nem pular.

Tabela `card_factory`:

| coluna | |
|---|---|
| `oracle_id` | chave |
| `status` | `pending` · `clustered` · `generated` · `verified` · `rejected` · `needs_ir` |
| `cluster_id` | o template a que pertence |
| `attempts` | quantas vezes já tentou |
| `reject_reason` | qual degrau da escada reprovou, e por quê |
| `ir_request` | qual capacidade falta, quando `needs_ir` |
| `updated_at` | |

Um comando mostra o painel: quantas em cada estado, quantas por pool de formato,
quais capacidades de IR estão bloqueando mais cartas.

## Revisão humana, onde ela paga

Não revisar carta a carta — revisar **amostra e divergência**:

- Amostra aleatória de cada lote aprovado (ex.: 2%), para pegar erro sistemático
  que passou por todos os degraus
- Toda divergência com o compilador determinístico
- Todo cluster novo antes de aplicar em massa (aprovar o template, não os 300 membros)

## A meta honesta

"Todas as cartas" não é 32.724, e prometer isso seria mentir. Algumas nunca serão
automatizáveis: cartas de Un-set com regras de destreza física, ante, cartas que
pedem interação fora do jogo, layouts que o motor não representa.

A meta que dá para defender:

> **100% das cartas legais em pelo menos um formato suportado e mecanicamente
> expressáveis**, com contagem pública e nominal do resto.

O que sobrar fica listado, com o motivo, no `ORACLE_COVERAGE.md`. Número que não
se esconde é número que se persegue.

## Fases

| Fase | O que | Bloqueio |
|---|---|---|
| **0** | compilador determinístico usa todo o IR existente + layouts multiface | em execução |
| **1** | infraestrutura: manifesto durável, escada de verificação 1–5, painel | livre |
| **2** | medir concordância do gerador nas 3.831 conhecidas (degrau 6) — **o portão** | depende da 1 |
| **3** | clusterização por template + extensão do IR guiada por formato | IR depende do workflow de formatos soltar `mtg-core` |
| **4** | fábrica em lotes, na ordem de prioridade | depende da 2 passar no portão |
| **5** | revisão amostrada e fechamento por pool | contínuo |

A fase 2 é um portão de verdade: se a concordância for baixa, a resposta não é
"gerar assim mesmo", é consertar o gerador — ou aceitar que a fábrica cobre menos
do que se queria e dizer isso em voz alta.
