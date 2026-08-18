# Editor de decks

Referências pedidas: [Moxfield](https://moxfield.com), [Archidekt](https://archidekt.com),
busca no estilo [Scryfall](https://scryfall.com), recomendação no estilo
[EDHREC](https://edhrec.com), importação e exportação `.dck`.

## A decisão que precisa vir primeiro: impressão ≠ carta

O pedido "escolher qual arte" parece cosmético e não é — muda o modelo de dados.
Se ficar errado agora, custa reescrever tudo depois.

Uma carta de Magic tem **duas identidades**:

| Identidade | Chave do Scryfall | O que é | Quem se importa |
|---|---|---|---|
| **Oracle** | `oracle_id` | as regras: nome, custo, tipo, texto, P/T | o **motor** |
| **Impressão** | `id` | a versão física: arte, set, número, moldura, artista | a **interface** |

Lightning Bolt tem uma identidade oracle e dezenas de impressões (LEA, M10,
Secret Lair, promo...). Todas jogam idêntico.

Consequência de arquitetura, e é a regra dura desta feature:

> **O motor nunca vê impressão.** `CardDefId` continua sendo identidade de regra.
> A escolha de arte vive no deck e na UI, jamais em `mtg-core`.

Uma entrada de deck passa a ser:

```rust
pub struct DeckEntry {
    pub oracle_id: String,        // o que o motor joga
    pub quantity: u8,
    pub printing_id: Option<String>, // qual arte mostrar; None = a padrão
}
```

Sem essa separação, trocar a arte de uma carta viraria trocar a carta — e duas
impressões da mesma carta contariam como cartas diferentes na regra de 4 cópias
(ou na de singleton do Commander, onde o estrago seria pior).

### O custo de dados

Hoje importamos `oracle_cards` (uma entrada por carta única, 23 MB). Impressões
vivem em `default_cards` (74 MB, ~110 mil linhas). Não precisamos do objeto
inteiro de cada impressão — só do que a UI mostra:

```sql
CREATE TABLE printings (
  printing_id   TEXT PRIMARY KEY,   -- scryfall id
  oracle_id     TEXT NOT NULL,      -- liga na carta
  set_code      TEXT NOT NULL,
  set_name      TEXT NOT NULL,
  collector_num TEXT NOT NULL,
  released_at   TEXT,
  frame_effects TEXT,               -- showcase, extendedart, etc.
  promo         INTEGER NOT NULL,
  image_art_crop TEXT,
  image_normal   TEXT,
  artist        TEXT
);
CREATE INDEX idx_printings_oracle ON printings(oracle_id);
```

Isso cabe em dezenas de MB, não centenas.

## Busca no estilo Scryfall

Uma mini-linguagem, não um campo de texto. Subconjunto que cobre o uso real:

| Sintaxe | Significado |
|---|---|
| `c:rg` / `c>=wu` | cores |
| `id:bant` | identidade de cor (Commander) |
| `t:creature` `t:legendary` | linha de tipo |
| `o:"draw a card"` | texto de oracle |
| `mv<=3` `pow>=4` `tou<2` | numéricos |
| `r:rare` | raridade |
| `f:modern` `f:pauper` | legalidade de formato |
| `s:dom` | set |
| `is:playable` | **nosso** — o motor sabe jogar |
| `-t:land` | negação |
| `t:elf or t:goblin` | alternativa |

Implementação: tokenizador → parser recursivo → árvore de predicados → SQL
parametrizado. Cada operador vira um teste. **Nunca** montar SQL por
concatenação de string: o campo de busca é entrada de usuário por definição.

`is:playable` é a nossa adição e é a mais importante da lista: 88% do catálogo o
motor ainda não sabe jogar, e deixar o usuário montar um deck que não roda seria
a pior experiência possível.

## Importação e exportação

| Formato | Direção | Nota |
|---|---|---|
| Texto simples (`4 Lightning Bolt`) | ambas | o mais universal; Moxfield, Archidekt e Arena exportam assim |
| `.dck` (Forge) | ambas | seções `[metadata]`, `[Main]`, `[Sideboard]` |
| `.dek` (MTGO) | importar | XML |
| JSON nosso | ambas | preserva `printing_id`, que os outros formatos perdem |

Regras de robustez, porque arquivo colado é entrada hostil:
- Nome que não existe no catálogo **não** é ignorado em silêncio — vira lista de
  erros mostrada ao usuário, com sugestão por distância de edição.
- Quantidade ausente vira 1; quantidade absurda é rejeitada, não truncada.
- Round-trip testado: exportar e reimportar devolve o mesmo deck.

## Recomendação — o que dá para prometer honestamente

EDHREC funciona sobre **um corpus de milhões de decklists públicas**: ele diz
"87% dos decks com este comandante jogam Sol Ring". Nós não temos esse corpus, e
inventar número de popularidade seria mentir com cara de dado.

O que dá para entregar, sem fingir:

1. **Tags funcionais do Scryfall.** O bulk `oracle_tags` (projeto Tagger da
   comunidade) classifica cartas por função: `ramp`, `card-draw`, `removal`,
   `tutor`, `board-wipe`. Já está disponível no mesmo endpoint que usamos.
2. **Análise do próprio deck**: curva de mana, contagem de terrenos, proporção de
   remoção e compra, buracos de curva. Isso é medível e específico daquele deck.
3. **Sinergia declarada**: carta que cita um subtipo que o deck tem em massa
   (`Elf`, `Goblin`), ou palavra-chave que o deck usa.
4. **Ajuste ao formato**: identidade de cor no Commander, legalidade, e `playable`.

O texto na tela precisa dizer de onde vem a sugestão — "seu deck tem 4 fontes de
compra, a média para 100 cartas é 10" é útil e verificável. "87% dos jogadores
usam" seria invenção.

Se um dia houver corpus (decks salvos pelos usuários), recomendação por
co-ocorrência entra sem quebrar nada — o gancho já é o mesmo.

## Validação viva

O editor mostra as violações **enquanto** se monta, não só no fim. O crate
`mtg-format` (em construção agora) já devolve **todas** as violações de uma vez,
que é exatamente o que um editor precisa:

- tamanho do deck por formato
- limite de cópias (terreno básico isento)
- singleton e identidade de cor no Commander
- legalidade por formato
- **quantas cartas do deck o motor sabe jogar** — nosso indicador, e o que
  separa "deck legal" de "deck que roda aqui"

## Ordem de execução

1. Tabela `printings` + importação de `default_cards` (a base do seletor de arte)
2. Parser da busca estilo Scryfall + API paginada
3. UI do editor: busca, grade de resultados, lista do deck, seletor de arte,
   validação viva
4. Importação e exportação de formatos
5. Recomendação por tags e análise de curva

## Bloqueio atual

Os passos 1 e 2 tocam `crates/mtg-db/**` e `crates/mtg-server/src/routes.rs`, e o
passo 3 toca `web/src/net/api.ts` — todos sendo reescritos agora pelo workflow
que liga o catálogo de 32 mil cartas ao servidor. O passo 5 depende de
`mtg-format`, que está sendo criado pelo workflow de formatos.
