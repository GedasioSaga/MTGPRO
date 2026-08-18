# Suíte de interações — a metade mensurável da barra

Cada item vira **um teste** com o nome do item. O teste monta um estado mínimo,
executa, e afirma o resultado exato. Pass rate desta suíte é o número que
acompanha o julgamento visual: gosto sozinho infla, gosto com número não.

Convenção dos testes: `Game` com `FixedAgent`/`ScriptedAgent` (agentes de teste
determinísticos), semente fixa, ferramental único em
`crates/mtg-core/tests/common/mod.rs`.

**Onde cada teste vive** — itens 1–41 em `tests/interactions_core.rs`, 42–60 em
`tests/interactions_combat.rs`, 61–65 em `tests/fuzz.rs`, 66–73 nos testes de
unidade de `engine/turn.rs` e `engine/stack.rs` (precisam de função privada e da
fábrica de partida com 3 e 4 jogadores), 74–79 em `tests/multiplayer.rs` e
`tests/commander.rs`, 80–86 nos testes de unidade de `mtg-server/src/sim.rs`,
87–91 nos de `mtg-format/src/scryfall_legality.rs` e `mtg-core/src/types.rs`.

## Legenda

`[x]` coberto por teste que afirma a regra · `[~]` parcial · `[ ]` ausente ·
`[!]` motor errado.

## Estado real em 18/08/2026

**91/91 `[x]`.** Nenhum `[~]`, nenhum `[ ]`, nenhum `[!]` em aberto. Verificado
com `cargo test --workspace` (361 testes, 0 falhas, 5 `#[ignore]` só por lentidão)
e `cargo test --workspace -- --ignored` (5 testes, 0 falhas).

Dois itens estavam `[!]` e o motor foi corrigido nesta rodada:

- **18** — `resolve::damage_object` só consultava `prevent_all_damage`, então
  dano fora de combate atravessava proteção. Corrigido com
  `resolve::protected_from` (CR 702.16c).
- **24** — `layers::collect_runtime_mods` aplicava `Duration::WhileSourcePresent`
  mesmo com a fonte fora do campo; só a limpeza do turno removia o efeito.
  Corrigido com `layers::effect_source_present` (CR 611.2b).

Itens 62, 63 e 65 são lentos e ficam sob `#[ignore]`: rodam com
`cargo test --workspace -- --ignored`. O `#[ignore]` deles é de custo, não de
falha.

## 1. Estrutura de turno e prioridade

1. [x] `primeiro_jogador_nao_compra_no_primeiro_turno` — CR 103.7a.
2. [x] `passo_de_desvirar_desvira_so_do_jogador_ativo` — CR 502.1.
3. [x] `pool_de_mana_esvazia_no_fim_do_passo` — CR 500.4.
4. [x] `ambos_passam_com_pilha_vazia_encerra_o_passo` — CR 117.4.
5. [x] `ambos_passam_com_pilha_cheia_resolve_o_topo` — CR 117.4.
6. [x] `limpeza_descarta_ate_sete_cartas` — CR 514.1.
7. [x] `limpeza_remove_dano_marcado` — CR 514.2.
8. [x] `gatilho_na_limpeza_da_prioridade_e_repete_a_limpeza` — CR 514.3a.
9. [x] `terreno_so_pode_ser_jogado_uma_vez_por_turno` — CR 305.1.
10. [x] `feitico_nao_pode_ser_lancado_com_pilha_nao_vazia` — CR 307.1.

## 2. Pilha, alvos e anulação

11. [x] `pilha_resolve_em_ordem_inversa` — LIFO.
12. [x] `magica_com_unico_alvo_ilegal_nao_resolve` — CR 608.2b (fizzle).
13. [x] `magica_com_dois_alvos_e_um_ilegal_ainda_resolve_no_outro` — CR 608.2b.
14. [x] `contra_magica_manda_o_alvo_para_o_cemiterio` — CR 701.5a.
15. [x] `hexproof_impede_alvo_de_oponente_mas_nao_do_controlador` — CR 702.11b.
16. [x] `shroud_impede_alvo_inclusive_do_controlador` — CR 702.18a.
17. [x] `protecao_contra_vermelho_impede_alvo_de_magica_vermelha` — CR 702.16b.
18. [x] `protecao_previne_dano_da_cor_protegida` — CR 702.16c. Era `[!]`: motor
    corrigido em `resolve::protected_from`.

## 3. Camadas (CR 613)

19. [x] `anthem_soma_com_marcador_de_mais_um` — +1/+1 estático + marcador = +2/+2.
20. [x] `define_pt_aplicado_depois_de_modifica_pt_ganha` — 7b antes de 7c.
21. [x] `dois_efeitos_de_define_pt_o_mais_novo_ganha` — timestamp, CR 613.7.
22. [x] `perda_de_palavra_chave_depois_de_ganho_remove` — camada 6 por timestamp.
23. [x] `efeito_de_fim_de_turno_expira_na_limpeza` — Duration::EndOfTurn.
24. [x] `efeito_expira_quando_a_fonte_sai_do_campo` — Duration::WhileSourcePresent.
    Era `[!]`: motor corrigido em `layers::effect_source_present`.
25. [x] `marcadores_menos_um_reduzem_resistencia_e_matam` — 7d + SBA 704.5f.

## 4. Ações baseadas em estado (CR 704)

26. [x] `vida_zero_perde_o_jogo` — 704.5a.
27. [x] `comprar_de_biblioteca_vazia_perde_na_proxima_sba` — 704.5b, não na hora.
28. [x] `criatura_com_resistencia_zero_vai_para_o_cemiterio` — 704.5f.
29. [x] `dano_letal_destroi` — 704.5g.
30. [x] `indestrutivel_sobrevive_a_dano_letal` — 704.5g + 702.12b.
31. [x] `indestrutivel_com_resistencia_zero_ainda_morre` — 704.5f ignora indestrutível.
32. [x] `toque_mortal_com_um_de_dano_destroi` — 704.5h.
33. [x] `regra_da_lenda_mantem_uma` — 704.5j.
34. [x] `marcadores_opostos_se_anulam` — 704.5r.
35. [x] `aura_sem_alvo_legal_vai_para_o_cemiterio` — 704.5m.

## 5. Gatilhos (CR 603)

36. [x] `gatilho_de_entrada_vai_para_a_pilha_antes_da_proxima_prioridade` — 603.3.
37. [x] `gatilho_de_morte_ve_o_estado_de_antes_da_morte` — 603.6d / "last known information".
38. [x] `gatilhos_simultaneos_apnap_jogador_ativo_primeiro` — 603.3b.
39. [x] `condicao_de_intervencao_falsa_impede_o_disparo` — 603.4.
40. [x] `gatilho_opcional_recusado_nao_faz_nada` — "você pode".
41. [x] `uma_vez_por_turno_nao_dispara_duas_vezes`.

## 6. Combate (CR 506–511)

42. [x] `criatura_com_enjoo_nao_pode_atacar` — CR 302.6.
43. [x] `pressa_permite_atacar_no_turno_que_entrou` — CR 702.10b.
44. [x] `vigilancia_ataca_sem_virar` — CR 702.20b.
45. [x] `voar_so_e_bloqueado_por_voar_ou_alcance` — CR 702.9b.
46. [x] `ameacar_exige_dois_bloqueadores` — CR 702.110b.
47. [x] `atacante_bloqueado_cujo_bloqueador_sumiu_nao_causa_dano_ao_jogador` — CR 509.1h.
48. [x] `atropelar_passa_o_excedente_ao_defensor` — CR 702.19b.
49. [x] `atropelar_com_toque_mortal_so_precisa_atribuir_um` — CR 702.2c.
50. [x] `primeiro_golpe_mata_antes_do_dano_normal` — CR 510.4.
51. [x] `golpe_duplo_causa_dano_nos_dois_passos` — CR 702.4b.
52. [x] `dano_de_combate_e_simultaneo_troca_mutua` — CR 510.2.
53. [x] `vinculo_com_a_vida_da_vida_ao_controlador` — CR 702.15b.
54. [x] `bloqueio_multiplo_respeita_a_ordem_de_dano` — CR 510.1c.

## 7. Mana e custos

55. [x] `custo_hibrido_pode_ser_pago_com_qualquer_metade` — CR 202.2f.
56. [x] `custo_phyrexiano_pode_ser_pago_com_dois_de_vida` — CR 107.4f.
57. [x] `x_igual_a_zero_e_um_valor_legal`.
58. [x] `habilidade_de_mana_nao_usa_a_pilha` — CR 605.3a.
59. [x] `custo_adicional_de_sacrificio_e_pago_antes_de_resolver` — CR 601.2h.
60. [x] `mana_nao_pago_impede_a_magica_de_aparecer_nas_acoes_legais`.

## 8. Determinismo e robustez do simulador

61. [x] `mesma_semente_produz_partida_identica` — dois `Game` com mesmo seed e mesmos
    bots geram o mesmo log completo.
62. [x] `partida_completa_termina_dentro_do_limite_de_turnos` (`--ignored`) — 200 partidas aleatórias,
    nenhuma estoura `max_turns` sem resultado.
63. [x] `nenhuma_partida_entra_em_panico` (`--ignored`) — 200 partidas com bots aleatórios, sementes
    0..199, nenhuma `panic!`, nenhum `unwrap` estourado.
64. [x] `acao_ilegal_de_agente_e_descartada_sem_corromper_estado`.
65. [x] `todas_as_cartas_do_catalogo_sao_lancaveis` (`--ignored`) — para cada `CardDef`, montar um
    estado com mana infinita e confirmar que a carta aparece nas ações legais e
    resolve sem erro.

## 9. Multijogador e Commander (CR 101.4, 117, 704.5v, 800.4, 903)

Mesa de 3 e 4 jogadores, e as regras próprias de Commander. O motor já era
multijogador por construção (`players` é `Vec`, `opponents` devolve todos,
`next_player` faz módulo pela contagem real); estes itens cobrem o comportamento
que só aparece acima de dois, e o que Commander acrescenta em cima disso.

66. [x] `passo_so_termina_quando_todos_os_quatro_passam` — CR 117.4: com quatro na mesa,
    o passo só acaba com quatro passes em sequência, começando pelo jogador ativo.
67. [x] `magica_resolvida_reinicia_a_rodada_de_prioridade_com_quatro` — CR 117.3b/117.3c:
    lançar e resolver reiniciam a rodada, e os quatro passam de novo.
68. [x] `apnap_com_quatro_jogadores_ordena_gatilhos` — CR 101.4: ativo primeiro, depois os
    não-ativos em ordem de turno, dando a volta na mesa.
69. [x] `jogador_eliminado_leva_seus_permanentes_junto` — CR 800.4a: os objetos que ele
    possui deixam o jogo em qualquer zona, e a partida continua.
70. [x] `permanente_emprestado_volta_ao_dono_na_eliminacao` — CR 800.4a: o efeito de
    controle dele termina e o permanente volta ao controle do dono.
71. [x] `turno_termina_quando_jogador_ativo_e_eliminado` — CR 800.4: o turno para no passo
    em que ele saiu e o próximo vivo assume.
72. [x] `partida_acaba_quando_resta_um` — CR 104.2a: com 4, só há vencedor quando sobra um.
73. [x] `partida_de_quatro_jogadores_termina_com_um_vencedor` — partida inteira de 4 com bots
    determinísticos: termina com vencedor único e com ao menos uma eliminação aplicada
    com a mesa ainda de pé.

### 9b. Dano de comandante como ação baseada em estado

`commander` conta o dano (CR 903.10) e `sba` aplica a derrota (CR 704.5v). Os
dois itens abaixo são a ponte: sem eles a matriz dos 21 podia encher para sempre
sem ninguém nunca perder.

74. [x] `a_sba_derrota_quem_levou_vinte_e_um_do_mesmo_comandante` — CR 704.5v: aos 21 a
    SBA derrota, com `LossReason::CommanderDamage` e a vida intacta; aos 20, não.
75. [x] `dano_de_comandante_derrota_um_so_e_a_mesa_continua` — CR 704.5v + CR 104.2a: numa
    mesa de quatro só o alvo sai, os outros três seguem jogando, e ele não deixa órfão.

### 9c. Mesa de quatro de ponta a ponta

76. [x] `cem_mesas_de_commander_com_quatro_jogadores_terminam_sem_panico_nem_orfao` —
    CR 104.2a + CR 800.4a: 100 partidas de Commander com 4 jogadores, sementes 0..99,
    bots aleatórios. Nenhum pânico; toda partida termina dentro do teto de 160 turnos;
    um vigia em cada agente confere, a cada decisão, que ninguém que já saiu deixou
    permanente ou item de pilha para trás. Medido: 100 vencedores, 0 empates, média de
    82,5 turnos (máximo 138), 3,00 eliminações por mesa.
77. [x] `a_mesma_semente_repete_a_mesa_de_quatro` — determinismo: mesma semente, mesmo
    resultado, mesmo número de turnos e mesmas eliminações.
78. [x] `cada_jogador_comeca_com_quarenta_de_vida_e_o_comandante_na_zona_de_comando` —
    CR 903.6 e CR 903.7 nos quatro assentos, não só nos dois primeiros; o comandante
    sai da biblioteca antes do embaralho.
79. [x] `eliminacao_no_meio_da_mesa_leva_junto_o_que_era_do_jogador` — CR 800.4a com três
    vivos: os permanentes dele saem do jogo e o que ele controlava sem possuir volta ao
    controle do dono, que continua na partida.

### 9d. Montagem da mesa no servidor (`sim.rs`)

80. [x] `aceita_de_dois_a_quatro_assentos_e_recusa_o_resto` — CR 100.4a: duelo é o mínimo,
    quatro é o teto; 0, 1, 5 e 8 assentos são recusados antes de montar o `Game`.
81. [x] `commander_sem_comandante_e_recusado_e_constructed_nao_exige` — CR 903.3: sem
    comandante não existe deck de Commander; Standard/Modern/Pauper não exigem nenhum.
82. [x] `deck_vazio_e_recusado_antes_de_montar_a_partida` — validação estrutural vira
    mensagem de erro no cliente, não pânico no motor.
83. [x] `formato_escolhe_regra_de_motor_vida_e_teto_de_turnos` — CR 903.7: Commander com 40
    de vida e teto de 160 turnos na mesa; os três construídos com 20 e teto de 60.
84. [x] `cada_assento_ganha_semente_propria_e_reproduzivel` — semente de bot é função pura
    da semente da partida e distinta por assento, senão a mesa vira espelho.
85. [x] `assento_leva_o_bot_pedido_e_o_comandante_declarado` — o `Seat` carrega bot e
    comandante; sem escolha explícita cai no bot padrão do servidor.
86. [x] `duelo_antigo_vira_mesa_de_dois` — o `MatchRequest` de duas cadeiras é um
    `TableRequest` de dois assentos: não há um segundo motor de partida escondido.

### 9e. Legalidade de carta por formato (CR 100.2, 903.5)

Banimento e rotação são dado externo, não regra de motor. A fonte é o campo
`legalities` do Scryfall, já importado para a tabela `cards` do catálogo.

87. [x] `banimento_e_rotacao_valem_e_carta_ausente_e_ilegal` — `banned` e `not_legal` não
    passam, `legal` e `restricted` passam, Casual não tem lista, e carta desconhecida é
    ilegal em todo formato (nome digitado errado tem de aparecer como problema).
88. [x] `carrega_do_catalogo_em_sqlite_com_a_mesma_select_da_producao` — a `SELECT` de
    produção rodada contra um banco em memória com o mesmo esquema; linha sem
    `legalities` não entra no índice.
89. [x] `objeto_legalities_do_scryfall_e_lido_par_a_par` — o parser do objeto plano,
    contra uma linha copiada do banco de verdade.
90. [x] `linha_com_raridade_desconhecida_e_descartada` — raridade não é adivinhada:
    adivinhar faria uma carta passar ou falhar em Pauper por engano.
91. [x] `raridade_faz_ida_e_volta_pelo_slug` — `Rarity::slug`/`from_slug` fecham o ciclo
    para as cinco raridades; `bonus` do Scryfall cai em `Special`.

## Metas

| Métrica | Alvo | Aferido em 18/08/2026 | Como medir |
|---|---|---|---|
| Pass rate da suíte | 100% dos itens 1–91 | 91/91 | `cargo test --workspace` |
| Partidas sem pânico | 200/200 | 200/200 | teste 63 (`--ignored`) |
| Determinismo | 100% | 100% | teste 61 |
| Cartas jogáveis | 100% do catálogo | 100% | teste 65 (`--ignored`) |
| Tempo de partida completa | < 150 ms com bot heurístico | não medido | benchmark em `mtg-server` |
| Mesa de 4 sem pânico | 100/100 | 100/100 | teste 76 |
| Mesa de 4 que decide sozinha | > 90% com vencedor | 100/100 | teste 76 |
| Mesa de 4 sem objeto órfão | 100% | 100% | teste 76 (vigia por decisão) |
