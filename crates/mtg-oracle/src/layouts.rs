//! Política de layout: qual face o compilador lê, e qual layout do Scryfall
//! tem representação no IR.
//!
//! Vive aqui, e não no importador, porque é decisão de **regras**, não de
//! formato de arquivo. O Scryfall só informa o nome do layout; o que cada
//! layout significa para o motor é conhecimento do compilador.
//!
//! A regra é a mesma do resto do compilador — fidelidade acima de cobertura.
//! Um layout só é compilável quando **tudo o que a carta faz cabe numa face
//! só**:
//!
//! * Layouts de uma face (`saga`, `class`, `case`, `prototype`, `mutate`,
//!   `leveler`) só eram bloqueados por serem diferentes de `normal`. O texto
//!   deles é texto comum e carrega a mecânica inteira; se o compilador de
//!   texto não der conta, ele mesmo recusa, com o padrão registrado no
//!   relatório. Bloquear pelo nome do layout escondia essa informação.
//! * `transform` tem duas faces, mas **só a frontal é lançável**: a de trás só
//!   se alcança por uma instrução de transformar. Uma frente que não fala em
//!   transformar é uma carta completa por si.
//! * `split`, `adventure`, `modal_dfc` e `prepare` têm **mais de uma face
//!   lançável da mão**. Compilar a face esquerda daria uma carta que o motor
//!   nunca joga do outro jeito — jogada legal que nunca acontece é partida
//!   errada em silêncio. Ficam bloqueados até o IR ter como representar custo
//!   alternativo por face.
//! * `flip` e `meld` mudam a identidade do permanente em jogo, e isso não
//!   existe no IR.

/// O que fazer com uma carta, decidido só pelo layout e pela contagem de faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutPlan {
    /// Índice da face de onde sair nome, custo, tipo, P/T, texto e imagem.
    /// `None` quer dizer "a raiz da carta", que é o caso de carta de uma face.
    pub face: Option<usize>,
    pub verdict: LayoutVerdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutVerdict {
    /// Tente compilar o texto da face escolhida como carta de uma face só.
    Compile,
    /// Layout conhecido e sem representação fiel no IR. O texto é o motivo
    /// que vai para o relatório.
    Blocked(&'static str),
    /// Layout que o Scryfall passou a emitir depois deste código. Nunca é
    /// compilado: layout desconhecido significa regra desconhecida.
    Unknown,
}

/// Layouts de uma face só cujo texto de oráculo carrega a mecânica inteira.
/// Para o compilador eles são indistinguíveis de `normal`.
const SINGLE_FACE_COMPILABLE: [&str; 7] =
    ["normal", "saga", "class", "case", "prototype", "mutate", "leveler"];

/// Layouts de mais de uma face em que só a frontal é lançável.
const FRONT_FACE_COMPILABLE: [&str; 1] = ["transform"];

/// Decide o plano de compilação. `face_count` é quantas entradas vieram em
/// `card_faces` (0 quando o campo não veio).
pub fn plan_for(layout: &str, face_count: usize) -> LayoutPlan {
    let layout = if layout.trim().is_empty() { "normal" } else { layout.trim() };
    let face = if face_count > 0 { Some(0) } else { None };

    if SINGLE_FACE_COMPILABLE.contains(&layout) || FRONT_FACE_COMPILABLE.contains(&layout) {
        return LayoutPlan { face, verdict: LayoutVerdict::Compile };
    }
    match blocked_reason(layout) {
        Some(why) => LayoutPlan { face, verdict: LayoutVerdict::Blocked(why) },
        None => LayoutPlan { face, verdict: LayoutVerdict::Unknown },
    }
}

/// Motivo estável por layout bloqueado. Estável porque vira chave de
/// agrupamento no relatório: mudar a frase muda o diff inteiro.
fn blocked_reason(layout: &str) -> Option<&'static str> {
    let why = match layout {
        "split" => "layout 'split': as duas metades sao lancaveis, cada uma com custo proprio",
        "adventure" => {
            "layout 'adventure': criatura e aventura sao lancaveis, cada uma com custo proprio"
        }
        "modal_dfc" => "layout 'modal_dfc': a face de tras e lancavel da mao, com custo proprio",
        "prepare" => "layout 'prepare': a face de tras e lancavel como copia, com custo proprio",
        "flip" => "layout 'flip': virar a carta em jogo troca nome, tipo e P/T",
        "meld" => "layout 'meld': fundir duas cartas numa terceira",
        "token" | "double_faced_token" | "emblem" | "art_series" | "reversible_card"
        | "minigame" | "front_card" => "layout sem carta de jogo",
        "planar" | "scheme" | "vanguard" | "augment" | "host" | "attraction" => {
            "layout de formato casual sem regras no motor"
        }
        _ => return None,
    };
    Some(why)
}

/// Marcas de que a face escolhida depende da outra face da carta.
///
/// Devolve a marca encontrada, para o motivo dizer **o que** travou. Aplicada
/// só a carta de mais de uma face: numa carta de uma face "transform" é verbo
/// comum e não indica dependência nenhuma.
///
/// A lista é deliberadamente ampla. Falso positivo custa uma carta a menos no
/// catálogo jogável; falso negativo custa uma carta que joga metade do que
/// está escrito — e essa é a falha que este projeto não pode ter.
pub fn references_other_face(oracle_text: &str) -> Option<&'static str> {
    const MARKS: [&str; 18] = [
        "transform",
        "//",
        "back face",
        "front face",
        "other face",
        "double-faced",
        "daybound",
        "nightbound",
        "disturb",
        "melds with",
        "meld",
        "it becomes day",
        "it becomes night",
        "becomes day",
        "becomes night",
        // Sinônimos de "transform" que o oráculo usa sem dizer a palavra.
        // "convert" é o verbo dos Transformers, "craft" é o de Ixalan e
        // "more than meets the eye" é custo alternativo para lançar a carta
        // já virada. Sem estes três, 18 frentes das 401 do bulk passavam pelo
        // filtro e virariam cartas que ignoram metade do que está escrito.
        "convert",
        "craft",
        "more than meets the eye",
    ];
    let hay = oracle_text.to_ascii_lowercase();
    MARKS.into_iter().find(|m| hay.contains(m))
}

/// Tipos cuja face de trás é alcançada por **regra**, sem uma palavra sequer
/// no texto da frente.
///
/// `Battle — Siege` é o caso: CR 310.9 manda exilar a batalha e lançar a face
/// de trás quando sai o último contador de defesa. Uma frente de Siege lida
/// sozinha é uma carta que entra em jogo e nunca faz o que a carta faz.
///
/// Só se aplica a carta de mais de uma face: sem outra face não há de que
/// depender.
pub fn type_depends_on_other_face(type_line: &str) -> Option<&'static str> {
    if type_line.to_ascii_lowercase().contains("battle") {
        return Some("battle");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_face_layouts_are_compiled_like_normal() {
        for layout in ["normal", "saga", "class", "case", "prototype", "mutate", "leveler"] {
            let plan = plan_for(layout, 0);
            assert_eq!(
                plan,
                LayoutPlan { face: None, verdict: LayoutVerdict::Compile },
                "layout '{layout}' e carta de uma face e deve chegar ao compilador de texto"
            );
        }
    }

    #[test]
    fn transform_reads_the_front_face() {
        let plan = plan_for("transform", 2);
        assert_eq!(plan.face, Some(0), "a face lancavel de uma carta transform e a frontal");
        assert_eq!(plan.verdict, LayoutVerdict::Compile);
    }

    #[test]
    fn layouts_with_two_castable_faces_are_blocked_with_a_reason() {
        for layout in ["split", "adventure", "modal_dfc", "prepare"] {
            let plan = plan_for(layout, 2);
            assert_eq!(plan.face, Some(0), "mesmo bloqueada, a face 0 da o metadado do catalogo");
            match plan.verdict {
                LayoutVerdict::Blocked(why) => {
                    assert!(
                        why.contains("custo proprio"),
                        "o motivo de '{layout}' tem de dizer que ha mais de um custo, veio: {why}"
                    );
                }
                other => panic!("layout '{layout}' nao pode compilar, veio {other:?}"),
            }
        }
    }

    #[test]
    fn flip_and_meld_are_blocked() {
        assert!(matches!(plan_for("flip", 2).verdict, LayoutVerdict::Blocked(_)));
        assert!(matches!(plan_for("meld", 0).verdict, LayoutVerdict::Blocked(_)));
    }

    #[test]
    fn unknown_layout_never_compiles() {
        assert_eq!(plan_for("hyperspace_dfc", 2).verdict, LayoutVerdict::Unknown);
        assert_eq!(plan_for("hyperspace_dfc", 2).face, Some(0));
    }

    #[test]
    fn empty_layout_counts_as_normal() {
        assert_eq!(plan_for("", 0).verdict, LayoutVerdict::Compile);
        assert_eq!(plan_for("   ", 0).verdict, LayoutVerdict::Compile);
    }

    #[test]
    fn normal_card_with_faces_reads_face_zero() {
        assert_eq!(plan_for("normal", 2).face, Some(0));
    }

    #[test]
    fn front_face_that_talks_about_the_other_side_is_detected() {
        assert_eq!(
            references_other_face(
                "At the beginning of your upkeep, look at the top card of your library. \
                 You may reveal that card. If an instant or sorcery card is revealed this way, \
                 transform Delver of Secrets."
            ),
            Some("transform")
        );
        assert_eq!(references_other_face("Daybound (reminder)"), Some("daybound"));
        assert_eq!(
            references_other_face("Melds with Gisela, the Broken Blade."),
            Some("melds with")
        );
    }

    #[test]
    fn transform_synonyms_are_treated_as_the_word_transform() {
        // "Arcee, Sharpshooter" vira sem nunca dizer "transform".
        assert_eq!(
            references_other_face(
                "{1}, Remove one or more +1/+1 counters from Arcee: It deals that much \
                 damage to target creature. Convert Arcee."
            ),
            Some("convert")
        );
        // "Braided Net": craft exila a carta e devolve a face de trás.
        assert_eq!(references_other_face("Craft with artifact {1}{U}"), Some("craft"));
        // "Optimus Prime, Hero": custo alternativo para lançar já virada.
        assert_eq!(
            references_other_face("More Than Meets the Eye {2}{U}{R}{W}"),
            Some("more than meets the eye")
        );
    }

    #[test]
    fn a_siege_depends_on_its_back_face_without_saying_so() {
        // "Invasion of Alara": o texto da frente não cita a outra face, mas
        // CR 310.9 lança a de trás quando sai o último contador de defesa.
        assert_eq!(type_depends_on_other_face("Battle \u{2014} Siege"), Some("battle"));
        assert_eq!(type_depends_on_other_face("Creature \u{2014} Human Wizard"), None);
        assert_eq!(type_depends_on_other_face(""), None);
    }

    #[test]
    fn self_contained_front_face_is_not_flagged() {
        assert_eq!(references_other_face("Flying\nWhen this creature enters, draw a card."), None);
        assert_eq!(references_other_face(""), None);
    }
}
