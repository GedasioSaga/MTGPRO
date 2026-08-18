//! Taxonomia da cobertura: **pools de formato** e **capacidades faltantes**.
//!
//! O número cru de cartas jogáveis mede mal. Ninguém joga com o catálogo
//! inteiro: joga-se com um pool — Pauper, Standard, Modern — e um pool pequeno
//! e coerente pode chegar perto de 100% enquanto o catálogo inteiro anda a 12%.
//! [`Pool`] e [`PoolMask`] existem para que o relatório diga *quanto do que se
//! joga* está coberto, e não só quanto do que existe.
//!
//! A segunda metade responde à outra pergunta: **o que implementar agora.**
//! Agrupar os textos travados por frase literal produz centenas de linhas de
//! contagem pequena; agrupar por *capacidade* — "manipulação de biblioteca",
//! "custo adicional", "permanente anexado" — produz uma dúzia de linhas, cada
//! uma um pedaço de trabalho de verdade.
//!
//! # A divisão que decide quem trabalha em quê
//!
//! Cada capacidade declara um [`Gap`]:
//!
//! - [`Gap::Parser`] — **o vocabulário já existe no IR de `mtg-core`.** O campo
//!   `need` nomeia a construção exata, para que a afirmação seja conferível.
//!   Dá para fazer inteiro dentro do compilador.
//! - [`Gap::Ir`] — **falta vocabulário.** Sem mexer em `mtg-core` não sai.
//!
//! Na dúvida a classificação é `Ir`. Chamar de `Parser` algo que na verdade
//! exige motor manda alguém para o crate errado e queima o dia dele; chamar de
//! `Ir` algo que já existe custa uma leitura de `ir.rs`.
//!
//! Este módulo é puro: nada de I/O, nada de rede, nada de banco. Recebe texto
//! normalizado e devolve classificação.

// ---------------------------------------------------------------------------
// Pools
// ---------------------------------------------------------------------------

/// Recorte do catálogo sobre o qual a cobertura é medida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pool {
    /// Tudo que entrou no catálogo. É o denominador que mede pior.
    Catalog,
    /// Legal em Pauper. Pool pequeno e coerente — é onde dá para chegar perto
    /// de 100%.
    Pauper,
    /// Legal em Standard.
    Standard,
    /// Legal em Modern.
    Modern,
    /// Comuns e incomuns de qualquer época: a espinha de qualquer deck.
    CommonUncommon,
}

impl Pool {
    /// Ordem fixa — é a ordem das linhas e colunas do relatório, e não pode
    /// depender de iteração de mapa.
    pub const ALL: [Pool; 5] =
        [Pool::Catalog, Pool::Pauper, Pool::Standard, Pool::Modern, Pool::CommonUncommon];

    /// Só os pools de formato, na ordem em que aparecem como coluna.
    pub const FORMATS: [Pool; 3] = [Pool::Pauper, Pool::Standard, Pool::Modern];

    pub fn label(self) -> &'static str {
        match self {
            Pool::Catalog => "Catálogo inteiro",
            Pool::Pauper => "Pauper",
            Pool::Standard => "Standard",
            Pool::Modern => "Modern",
            Pool::CommonUncommon => "Comuns e incomuns",
        }
    }

    /// Chave estável para arquivo de baseline e para a coluna da tabela.
    pub fn slug(self) -> &'static str {
        match self {
            Pool::Catalog => "catalog",
            Pool::Pauper => "pauper",
            Pool::Standard => "standard",
            Pool::Modern => "modern",
            Pool::CommonUncommon => "common_uncommon",
        }
    }

    /// Nome do formato como o Scryfall escreve em `legalities`. `None` para os
    /// pools que não saem de legalidade.
    pub fn scryfall_format(self) -> Option<&'static str> {
        match self {
            Pool::Pauper => Some("pauper"),
            Pool::Standard => Some("standard"),
            Pool::Modern => Some("modern"),
            Pool::Catalog | Pool::CommonUncommon => None,
        }
    }

    fn bit(self) -> u8 {
        match self {
            Pool::Catalog => 1,
            Pool::Pauper => 1 << 1,
            Pool::Standard => 1 << 2,
            Pool::Modern => 1 << 3,
            Pool::CommonUncommon => 1 << 4,
        }
    }
}

/// De que pools uma carta participa. Um byte por carta, não um mapa: são 33
/// mil cartas por importação.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolMask(u8);

impl PoolMask {
    pub const fn empty() -> PoolMask {
        PoolMask(0)
    }
    pub fn with(self, p: Pool) -> PoolMask {
        PoolMask(self.0 | p.bit())
    }
    pub fn contains(self, p: Pool) -> bool {
        self.0 & p.bit() != 0
    }
}

/// Raridades que contam como "espinha de deck".
const BACKBONE_RARITIES: [&str; 2] = ["common", "uncommon"];

/// Status de legalidade que permite a carta entrar num deck do formato.
///
/// `banned` fica de fora de propósito: carta banida não é jogável no formato, e
/// contá-la infla o denominador sem mexer no numerador. `restricted` é do
/// Vintage, que não é medido aqui.
const PLAYABLE_STATUS: &str = "legal";

/// De que pools esta carta participa.
///
/// `legality` recebe o nome do formato como o Scryfall escreve (`"pauper"`) e
/// devolve o status (`"legal"`, `"not_legal"`, `"banned"`). `None` significa
/// **campo ausente**, não "ilegal": o chamador tem de contar essas cartas à
/// parte e dizer no relatório, em vez de deixá-las derrubarem a porcentagem
/// como se fossem ilegais.
pub fn pools_of(rarity: &str, legality: impl Fn(&str) -> Option<&str>) -> PoolMask {
    let mut mask = PoolMask::empty().with(Pool::Catalog);
    for pool in Pool::FORMATS {
        if let Some(fmt) = pool.scryfall_format() {
            if legality(fmt) == Some(PLAYABLE_STATUS) {
                mask = mask.with(pool);
            }
        }
    }
    if BACKBONE_RARITIES.contains(&rarity.trim().to_ascii_lowercase().as_str()) {
        mask = mask.with(Pool::CommonUncommon);
    }
    mask
}

/// `true` quando a carta trouxe o campo `legalities` de que os pools de formato
/// dependem. Sem ele a coluna do relatório mentiria por omissão.
pub fn has_format_legality(legality: impl Fn(&str) -> Option<&str>) -> bool {
    Pool::FORMATS.iter().filter_map(|p| p.scryfall_format()).any(|f| legality(f).is_some())
}

// ---------------------------------------------------------------------------
// Capacidades
// ---------------------------------------------------------------------------

/// Onde está o buraco — e portanto quem pode tapá-lo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    /// O IR já tem a construção; falta o compilador reconhecer o texto.
    Parser,
    /// Falta vocabulário em `mtg-core`. Não sai sem mexer no motor.
    Ir,
}

impl Gap {
    pub fn label(self) -> &'static str {
        match self {
            Gap::Parser => "parser",
            Gap::Ir => "IR",
        }
    }
}

/// Um pedaço de trabalho: uma família de textos que se implementa junta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// Identificador estável em kebab-case. É a chave de desempate na
    /// ordenação do relatório, então mudá-lo muda o diff.
    pub id: &'static str,
    pub label: &'static str,
    pub gap: Gap,
    /// Para `Gap::Parser`, a construção do IR que resolve — nomeada para poder
    /// ser conferida em `ir.rs`. Para `Gap::Ir`, o que falta lá.
    pub need: &'static str,
    /// O padrão normalizado precisa ser exatamente uma destas strings...
    exact: &'static [&'static str],
    /// ...ou conter alguma destas...
    needles: &'static [&'static str],
    /// ...e não conter nenhuma destas. Serve para separar famílias que
    /// compartilham vocabulário ("search ... into your hand" contra
    /// "search ... onto the battlefield").
    unless: &'static [&'static str],
}

impl Capability {
    fn matches(&self, pattern: &str) -> bool {
        if self.unless.iter().any(|n| pattern.contains(n)) {
            return false;
        }
        if self.exact.iter().any(|e| pattern == *e) {
            return true;
        }
        self.needles.iter().any(|n| pattern.contains(n))
    }
}

const fn cap(
    id: &'static str,
    label: &'static str,
    gap: Gap,
    need: &'static str,
    exact: &'static [&'static str],
    needles: &'static [&'static str],
    unless: &'static [&'static str],
) -> Capability {
    Capability { id, label, gap, need, exact, needles, unless }
}

/// Bloqueio que não é textual: a carta tem mais de uma face, ou um layout que o
/// modelo de carta não representa. Não sai de [`classify`] — o chamador a usa
/// para as cartas cujo bloqueio veio antes de o texto ser lido.
pub const LAYOUT: Capability = cap(
    "layout-multiface",
    "Carta de mais de uma face (split, adventure, transform, MDFC)",
    Gap::Ir,
    "`CardDef` representa uma face só: falta modelo de face e de troca de face",
    &[],
    &[],
    &[],
);

/// Texto travado que nenhuma regra reconheceu. Existe para que o buraco da
/// própria taxonomia apareça no relatório em vez de sumir dentro dele.
pub const UNCLASSIFIED: Capability = cap(
    "nao-classificado",
    "Não classificado pela taxonomia",
    Gap::Ir,
    "nenhuma regra de `mtg_oracle::coverage` casou — quem está incompleta é a taxonomia",
    &[],
    &[],
    &[],
);

/// As regras, **em ordem**: a primeira que casa vence.
///
/// A ordem é a especificidade. "search your library ... onto the battlefield"
/// tem de ser testado antes de "search your library ... into your hand", e
/// "enchanted creature gets +N/+N" antes de qualquer regra de pump — senão a
/// família cai no balde errado e o número manda alguém para o crate errado.
pub const CAPABILITIES: &[Capability] = &[
    // --- mecânicas nomeadas que o vocabulário de palavra-chave não tem ------
    cap(
        "palavra-chave-fora-do-ir",
        "Palavra-chave sem variante em `ir::Keyword`",
        Gap::Ir,
        "variante nova em `ir::Keyword` (devoid, changeling, infect, shadow, bushido, toxic, crew, …)",
        &[
            "devoid",
            "changeling",
            "infect",
            "shadow",
            "horsemanship",
            "banding",
            "phasing",
            "provoke",
            "soulbond",
            "sunburst",
            "entwine",
            "madness",
            "persist",
            "undying",
            "evoke",
        ],
        &[
            "bushido N",
            "toxic N",
            "crew N",
            "modular N",
            "graft N",
            "rampage N",
            "amplify N",
            "bloodthirst N",
            "dredge N",
            "fading N",
            "vanishing N",
        ],
        &[],
    ),
    cap(
        "energia",
        "Contadores de energia {E}",
        Gap::Ir,
        "reserva de energia por jogador — `CounterKind` é de permanente, não de jogador",
        &[],
        &["{e}"],
        &[],
    ),
    cap(
        "designacao-monarca-iniciativa",
        "Designações: monarca, iniciativa, bênção da cidade",
        Gap::Ir,
        "estado de jogador designado — `GameState` não tem monarca nem iniciativa",
        &[],
        &["the monarch", "the initiative", "city's blessing", "ascend"],
        &[],
    ),
    cap(
        "velocidade",
        "Velocidade / max speed",
        Gap::Ir,
        "contador de velocidade por jogador e o gatilho que o incrementa",
        &[],
        &["start your engines", "max speed", "your speed"],
        &[],
    ),
    cap(
        "aleatoriedade",
        "Cara ou coroa, dados",
        Gap::Ir,
        "efeito de sorteio com resultado ramificado — nenhum `Effect` produz aleatório observável",
        &[],
        &["flip a coin", "roll a d", "roll N d"],
        &[],
    ),
    // --- custo -------------------------------------------------------------
    cap(
        "custo-adicional",
        "Custo adicional de lançamento",
        Gap::Ir,
        "`CardDef` não tem onde pendurar custo de lançamento: `Cost` existe, o campo não",
        &[],
        &["as an additional cost to cast"],
        &[],
    ),
    cap(
        "reducao-de-custo",
        "Redução e aumento de custo de lançamento",
        Gap::Parser,
        "`StaticAbility` com `StaticMod::CostReduction` / `StaticMod::CostIncrease` (aplicadas em `engine/cast.rs`)",
        &[],
        &["less to cast", "more to cast", "affinity for"],
        &[],
    ),
    // --- substituição na entrada -------------------------------------------
    cap(
        "efeito-de-substituicao",
        "Entra virado, entra com marcadores, substituição de entrada",
        Gap::Ir,
        "`ReplacementAbility` existe no IR mas o motor não lê nenhuma: zero ocorrências de `Ability::Replacement` em `engine/`",
        &[],
        &["enters tapped", "enters with", "enters the battlefield tapped"],
        &[],
    ),
    cap(
        "escolha-na-entrada",
        "Escolher tipo ou cor ao entrar no campo",
        Gap::Ir,
        "escolha registrada no objeto: `Value::ChosenNumber` cobre número, não tipo nem cor",
        &[],
        &[
            "as ~ enters, choose",
            "as ~ enters the battlefield, choose",
            "choose a creature type",
            "choose a color",
        ],
        &[],
    ),
    // --- permanente anexado ------------------------------------------------
    cap(
        "permanente-anexado",
        "Aura e equipamento: efeito estático sobre o que está anexado",
        Gap::Ir,
        "falta `Filter::Attached` — `StaticAbility.affects` é um `Selector`, e nenhum filtro descreve \"o que isto encanta\"",
        &[],
        &[
            "equipped creature",
            "enchanted creature",
            "enchanted permanent",
            "enchanted land",
            "enchanted artifact",
            "enchant player",
            "enchanted player",
        ],
        &[],
    ),
    // --- biblioteca --------------------------------------------------------
    cap(
        "busca-para-o-campo",
        "Buscar na biblioteca e pôr direto no campo",
        Gap::Ir,
        "`Effect::SearchLibrary` só tem `to_hand: bool` — falta destino (campo, topo, cemitério) e estado virado",
        &[],
        &["search your library", "search their library"],
        &["into your hand", "into their hand"],
    ),
    cap(
        "busca-para-a-mao",
        "Buscar na biblioteca e pôr na mão",
        Gap::Parser,
        "`Effect::SearchLibrary { to_hand: true }` seguido de `Effect::ShuffleLibrary`",
        &[],
        &["search your library", "search their library"],
        &[],
    ),
    cap(
        "olhar-o-topo",
        "Olhar o topo da biblioteca e escolher",
        Gap::Ir,
        "falta efeito de olhar-e-escolher: `Effect::Scry` decide topo ou fundo, não põe carta na mão",
        &[],
        &["look at the top"],
        &[],
    ),
    cap(
        "revelar",
        "Revelar cartas (mão, topo da biblioteca)",
        Gap::Ir,
        "falta `Effect::Reveal` — revelar é informação pública, e nenhum efeito a produz",
        &[],
        &["reveals their hand", "reveal the top", "reveals the top", "reveal cards from the top"],
        &[],
    ),
    cap(
        "vidente-e-moer",
        "Scry, surveil, moer",
        Gap::Parser,
        "`Effect::Scry`, `Effect::Surveil`, `Effect::Mill`",
        &[],
        &["scry N", "surveil N", "mills N", "mill N"],
        &[],
    ),
    // --- cemitério ---------------------------------------------------------
    cap(
        "retorno-do-cemiterio",
        "Devolver do cemitério para a mão ou para o campo",
        Gap::Parser,
        "`Effect::ReturnToHand` e `Effect::ReturnFromGraveyardToBattlefield`",
        &[],
        &[
            "from your graveyard to your hand",
            "from your graveyard to the battlefield",
            "from a graveyard to",
            "return ~ from your graveyard",
            "return this card from your graveyard",
        ],
        &[],
    ),
    cap(
        "regeneracao",
        "Regenerar",
        Gap::Ir,
        "falta escudo de regeneração — `Effect::Destroy { no_regeneration }` só sabe ignorá-lo",
        &[],
        &["regenerate"],
        &[],
    ),
    // --- pilha -------------------------------------------------------------
    cap(
        "nao-pode-ser-contraespelado",
        "Não pode ser contraespelado",
        Gap::Ir,
        "falta propriedade de objeto na pilha — `Effect::CounterSpell` não consulta nada",
        &[],
        &["can't be countered"],
        &[],
    ),
    // --- prevenção ---------------------------------------------------------
    cap(
        "prevencao-de-dano",
        "Prevenir dano (parcial, de combate, do próximo)",
        Gap::Ir,
        "só existe `StaticMod::PreventAllDamage`, contínua e total — falta escudo com quantidade e prevenção de uma vez só",
        &[],
        &["prevent the next", "prevent all damage", "prevent all combat damage", "prevent that damage"],
        &[],
    ),
    // --- combate -----------------------------------------------------------
    cap(
        "ataque-e-bloqueio-obrigatorios",
        "Ataca ou bloqueia se puder, precisa ser bloqueado",
        Gap::Ir,
        "falta `StaticMod::MustAttack` / `MustBlock` — só existem `CantAttack` e `CantBlock`",
        &[],
        &[
            "attacks each combat if able",
            "attacks each turn if able",
            "blocks each combat if able",
            "must be blocked",
        ],
        &[],
    ),
    cap(
        "nao-pode-ser-bloqueado-estatico",
        "Não pode ser bloqueado (estático, permanente)",
        Gap::Ir,
        "falta `StaticMod::CantBeBlocked` — a variante existe em `StaticModRuntime`, não na de autoria",
        &["~ can't be blocked"],
        &["~ can't be blocked by", "~ can't be blocked except by"],
        &["this turn", "until end of turn"],
    ),
    cap(
        "restricao-de-combate-temporaria",
        "Não pode bloquear nem atacar até o fim do turno",
        Gap::Parser,
        "`Effect::CantBeBlocked` e `Effect::CantAttackOrBlock`, com `Duration::EndOfTurn`",
        &[],
        &["can't block this turn", "can't attack or block", "can't be blocked this turn"],
        &[],
    ),
    cap(
        "nao-desvira",
        "Não desvira no passo de desvirar",
        Gap::Ir,
        "falta `StaticMod::DoesNotUntap` — existe em `StaticModRuntime`, não na de autoria",
        &[],
        &["doesn't untap during", "choose not to untap"],
        &[],
    ),
    // --- fichas ------------------------------------------------------------
    cap(
        "ficha-copia",
        "Ficha que é cópia de outro permanente",
        Gap::Ir,
        "`TokenSpec` é ficha literal — não há como copiar um objeto do jogo",
        &[],
        &["a token that's a copy", "copy of target", "copy of that"],
        &[],
    ),
    cap(
        "ficha-com-habilidade",
        "Ficha com habilidade ativada (Clue, Treasure, Food, …)",
        Gap::Ir,
        "`TokenSpec` tem `keywords`, não `abilities` — ficha com \"{2}, sacrifique: compre\" não é representável",
        &[],
        &[
            "investigate",
            "clue token",
            "treasure token",
            "food token",
            "blood token",
            "map token",
            "junk token",
            "incubator token",
            "powerstone token",
        ],
        &[],
    ),
    // --- pump e estáticos --------------------------------------------------
    cap(
        "pump-alvo-com-palavra-chave",
        "Pump em alvo somado a palavra-chave, até o fim do turno",
        Gap::Parser,
        "`Effect::Sequence([Effect::ModifyPT, Effect::GrantKeywords])` com `Duration::EndOfTurn`",
        &[],
        &["gets +N/+N and gains", "gets +N/+N and has", "gets -N/-N and gains"],
        &[],
    ),
    cap(
        "pump-de-massa",
        "Pump ou debuff de massa",
        Gap::Parser,
        "`Effect::ModifyPT { target: ObjRef::All(Selector), … }` — `ObjRef::All` já é resolvido em `engine/query.rs`",
        &[],
        &[
            "creatures you control get +",
            "creatures you control get -",
            "all creatures get +",
            "all creatures get -",
            "creatures your opponents control get",
            "<tipo> you control get +",
            "<tipo>s you control get +",
        ],
        &[],
    ),
    cap(
        "pt-condicional",
        "P/T que depende do estado do jogo",
        Gap::Parser,
        "`StaticAbility { condition, modification: StaticMod::ModifyPT(Value, Value) }` com `Value::Count`",
        &[],
        &["as long as", "for each"],
        &[],
    ),
    // --- efeitos que o IR já tem em cheio ----------------------------------
    cap(
        "mana-de-qualquer-cor",
        "Mana de qualquer cor",
        Gap::Parser,
        "`ManaProduction::AnyColor` e `Effect::AddManaAnyColor`",
        &[],
        &["mana of any color", "mana of any one color"],
        &[],
    ),
    cap(
        "controle-de-permanente",
        "Ganhar controle de permanente",
        Gap::Parser,
        "`Effect::GainControl { duration }`",
        &[],
        &["gain control of"],
        &[],
    ),
    cap(
        "modal",
        "Modo: escolha uma ou mais opções",
        Gap::Parser,
        "`Effect::Modal { choose, options }`",
        &[],
        &["choose one", "choose two", "choose up to"],
        &[],
    ),
    cap(
        "marcadores",
        "Pôr e tirar marcadores",
        Gap::Parser,
        "`Effect::AddCounters` e `Effect::RemoveCounters`",
        &[],
        &["counter on", "counters on", "counter from", "counters from"],
        &[],
    ),
    cap(
        "sacrificio-e-descarte",
        "Sacrificar e descartar",
        Gap::Parser,
        "`Effect::Sacrifice` e `Effect::Discard`",
        &[],
        &["sacrifice a", "sacrifices a", "discards a card", "discard a card", "discards their hand"],
        &[],
    ),
    cap(
        "exilio",
        "Exilar",
        Gap::Parser,
        "`Effect::Exile { until_source_leaves }`",
        &[],
        &["exile target", "exile all", "exile that", "exile it"],
        &[],
    ),
    cap(
        "dano",
        "Causar dano",
        Gap::Parser,
        "`Effect::DealDamage`, `Effect::DealDamageToPlayer`, `Effect::DivideDamage`",
        &[],
        &["deals N damage", "divided as you choose"],
        &[],
    ),
    cap(
        "virar-e-desvirar",
        "Virar e desvirar",
        Gap::Parser,
        "`Effect::Tap`, `Effect::Untap`, `Effect::Freeze`",
        &[],
        &["tap target", "untap target", "tap all", "untap all", "tap up to"],
        &[],
    ),
];

/// A que capacidade este texto travado pertence.
///
/// A entrada é o padrão já normalizado — minúsculo, com `~` no lugar do nome,
/// `N` no lugar de número, `<tipo>`/`<cor>`/`<terreno>` no lugar de subtipo, cor
/// e terreno básico. Devolve [`UNCLASSIFIED`] quando nenhuma regra casa, nunca
/// `None`: capacidade não reconhecida é informação, não ausência.
pub fn classify(pattern: &str) -> &'static Capability {
    CAPABILITIES.iter().find(|c| c.matches(pattern)).unwrap_or(&UNCLASSIFIED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn legal_only_in(formats: &'static [&'static str]) -> impl Fn(&str) -> Option<&'static str> {
        move |f: &str| {
            if formats.contains(&f) {
                Some("legal")
            } else {
                Some("not_legal")
            }
        }
    }

    #[test]
    fn pools_follow_legality_and_rarity() {
        let m = pools_of("common", legal_only_in(&["pauper", "modern"]));
        assert!(m.contains(Pool::Catalog));
        assert!(m.contains(Pool::Pauper));
        assert!(m.contains(Pool::Modern));
        assert!(!m.contains(Pool::Standard));
        assert!(m.contains(Pool::CommonUncommon), "comum é espinha de deck");

        let rare = pools_of("mythic", legal_only_in(&["standard", "modern"]));
        assert!(!rare.contains(Pool::CommonUncommon));
        assert!(rare.contains(Pool::Standard));
        assert!(!rare.contains(Pool::Pauper));
    }

    #[test]
    fn banned_is_not_playable_in_the_format() {
        // Há 72 banidas em Pauper no bulk. Contá-las como pool infla o
        // denominador sem mexer no numerador.
        let m =
            pools_of("common", |f| if f == "pauper" { Some("banned") } else { Some("not_legal") });
        assert!(!m.contains(Pool::Pauper));
        assert!(m.contains(Pool::Catalog), "continua no catálogo");
    }

    #[test]
    fn missing_legalities_is_not_illegal() {
        let m = pools_of("rare", |_| None);
        assert_eq!(m, PoolMask::empty().with(Pool::Catalog));
        assert!(!has_format_legality(|_| None), "ausência tem que ser detectável");
        assert!(has_format_legality(legal_only_in(&[])));
    }

    #[test]
    fn capability_ids_are_unique_and_kebab_case() {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for c in CAPABILITIES.iter().chain([&LAYOUT, &UNCLASSIFIED]) {
            assert!(seen.insert(c.id), "id repetido: {}", c.id);
            assert!(
                c.id.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'),
                "id fora do padrão kebab-case: {}",
                c.id
            );
            assert!(!c.need.is_empty(), "{} não diz o que falta", c.id);
        }
        assert!(seen.len() >= 30, "taxonomia rasa demais: {} capacidades", seen.len());
    }

    /// Cada caso é um padrão que o relatório de hoje lista, com a capacidade em
    /// que ele **tem** que cair. É o teste que impede a ordem das regras de ser
    /// mexida sem que alguém perceba.
    #[test]
    fn real_patterns_land_in_the_intended_capability() {
        let cases: &[(&str, &str)] = &[
            ("as an additional cost to cast this spell, sacrifice a creature", "custo-adicional"),
            ("this spell costs {N} less to cast", "reducao-de-custo"),
            ("look at the top N cards of your library", "olhar-o-topo"),
            (
                "search your library for a <terreno> card, put it onto the battlefield tapped, then shuffle",
                "busca-para-o-campo",
            ),
            (
                "search your library for a <terreno> card, reveal it, put it into your hand, then shuffle",
                "busca-para-a-mao",
            ),
            ("creatures you control get +N/+N until end of turn", "pump-de-massa"),
            (
                "target creature gets +N/+N and gains trample until end of turn",
                "pump-alvo-com-palavra-chave",
            ),
            ("regenerate ~", "regeneracao"),
            ("this spell can't be countered", "nao-pode-ser-contraespelado"),
            ("devoid", "palavra-chave-fora-do-ir"),
            ("add one mana of any color", "mana-de-qualquer-cor"),
            ("target opponent reveals their hand", "revelar"),
            ("~ can't be blocked", "nao-pode-ser-bloqueado-estatico"),
            ("target creature can't block this turn", "restricao-de-combate-temporaria"),
            ("~ enters with x +N/+N counters on it", "efeito-de-substituicao"),
            ("enchant player", "permanente-anexado"),
            ("investigate", "ficha-com-habilidade"),
            ("you get {e}{e}", "energia"),
            ("you become the monarch", "designacao-monarca-iniciativa"),
            (
                "prevent the next N damage that would be dealt to any target this turn",
                "prevencao-de-dano",
            ),
            ("~ attacks each combat if able", "ataque-e-bloqueio-obrigatorios"),
            ("~ doesn't untap during your untap step", "nao-desvira"),
            ("equipped creature gets +N/+N and has vigilance", "permanente-anexado"),
            ("gain control of target creature until end of turn", "controle-de-permanente"),
            ("choose one —", "modal"),
            ("~ gets +N/+N as long as you control a <tipo>", "pt-condicional"),
            ("create a token that's a copy of target creature", "ficha-copia"),
            ("flip a coin", "aleatoriedade"),
            ("start your engines!", "velocidade"),
            (
                "return target creature card from your graveyard to your hand",
                "retorno-do-cemiterio",
            ),
        ];
        for (pattern, expected) in cases {
            assert_eq!(
                classify(pattern).id,
                *expected,
                "padrão {pattern:?} caiu na capacidade errada"
            );
        }
    }

    #[test]
    fn unknown_text_is_reported_not_swallowed() {
        let c = classify("o sol nasce atras do morro e nada em magic diz isso");
        assert_eq!(c.id, "nao-classificado");
        assert_eq!(c.gap, Gap::Ir, "na dúvida, o buraco é do IR");
    }

    #[test]
    fn parser_gaps_name_an_existing_ir_construct() {
        // A promessa da coluna: `Gap::Parser` significa "o vocabulário já
        // existe". Sem citar a construção ninguém consegue conferir a promessa,
        // e promessa não conferível manda gente para o crate errado.
        for c in CAPABILITIES.iter().filter(|c| c.gap == Gap::Parser) {
            assert!(
                c.need.contains("Effect::")
                    || c.need.contains("StaticMod::")
                    || c.need.contains("ManaProduction::"),
                "{} promete parser sem nomear a construção do IR: {}",
                c.id,
                c.need
            );
        }
    }

    #[test]
    fn classification_is_deterministic() {
        let pattern =
            "search your library for a <terreno> card, put it onto the battlefield tapped, then shuffle";
        let first = classify(pattern).id;
        for _ in 0..10 {
            assert_eq!(classify(pattern).id, first);
        }
    }
}
