//! Azul: contramágica, compra de cartas, bounce e voadores evasivos. É a
//! espinha dorsal do deck de controle.

use mtg_core::card::{CardDef, TriggerCondition};
use mtg_core::ir::{
    Cost, Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TimingRestriction, Value,
};
use mtg_core::types::CardType;
use mtg_core::types::Rarity::*;

use crate::builders::*;

/// "alvo de mágica" e suas variantes — contramágica é metade do azul.
fn t_any_spell() -> mtg_core::ir::TargetSpec {
    t_spell(Filter::Any, "alvo de mágica")
}

fn t_creature_spell() -> mtg_core::ir::TargetSpec {
    t_spell(Filter::HasType(CardType::Creature), "alvo de mágica de criatura")
}

pub fn cards() -> Vec<CardDef> {
    vec![
        // --- criaturas ---
        creature("Merfolk Looter", "1U", "Creature — Merfolk Rogue", 1, 1)
            .ability(activated(
                tap_cost(),
                Effect::seq([
                    draw(1, PlayerRef::You),
                    Effect::Discard {
                        count: Value::c(1),
                        player: PlayerRef::You,
                        filter: Filter::Any,
                        random: false,
                    },
                ]),
                TimingRestriction::Instant,
                "{T}: Draw a card, then discard a card.",
            ))
            .oracle("{T}: Draw a card, then discard a card.")
            .flavor("Some sift the sand for pearls. He sifts for secrets.")
            .meta(Uncommon, "M12", "63", "Greg Staples")
            .build(),
        creature("Prodigal Sorcerer", "2U", "Creature — Human Wizard", 1, 1)
            .ability(activated_targeted(
                tap_cost(),
                vec![t_any()],
                dmg_any(1),
                TimingRestriction::Instant,
                "{T}: Prodigal Sorcerer deals 1 damage to any target.",
            ))
            .oracle("{T}: Prodigal Sorcerer deals 1 damage to any target.")
            .flavor("\"A little pain teaches more than a long lecture.\"")
            .meta(Common, "6ED", "89", "Julie Baroh")
            .build(),
        creature("Man-o'-War", "2U", "Creature — Jellyfish", 2, 2)
            .ability(etb_targeted(
                "When Man-o'-War enters, return target creature to its owner's hand.",
                vec![t_creature()],
                bounce(target0()),
            ))
            .oracle("When Man-o'-War enters, return target creature to its owner's hand.")
            .flavor("Its sting is a suggestion. Its grip is not.")
            .meta(Common, "VIS", "40", "Bryon Wackwitz")
            .build(),
        creature("Aether Adept", "2U", "Creature — Human Wizard", 2, 2)
            .ability(etb_targeted(
                "When Aether Adept enters, return target creature to its owner's hand.",
                vec![t_creature()],
                bounce(target0()),
            ))
            .oracle("When Aether Adept enters, return target creature to its owner's hand.")
            .meta(Uncommon, "M11", "41", "Steve Argyle")
            .build(),
        creature("Frost Lynx", "2U", "Creature — Elemental Cat", 2, 2)
            .ability(etb_targeted(
                "When Frost Lynx enters, tap target creature an opponent controls. That creature doesn't untap during its controller's next untap step.",
                vec![t_creature_opponent()],
                Effect::seq([
                    Effect::Tap { target: target0() },
                    Effect::Freeze { target: target0() },
                ]),
            ))
            .oracle("When Frost Lynx enters, tap target creature an opponent controls. That creature doesn't untap during its controller's next untap step.")
            .flavor("Its purr is the crack of a freezing lake.")
            .meta(Common, "M15", "54", "Zack Stella")
            .build(),
        creature("Cloudkin Seer", "2U", "Creature — Elemental Bird", 2, 1)
            .kw(Keyword::Flying)
            .ability(etb(
                "When Cloudkin Seer enters, draw a card.",
                draw(1, PlayerRef::You),
            ))
            .oracle("Flying\nWhen Cloudkin Seer enters, draw a card.")
            .meta(Common, "M20", "64", "Simon Dominic")
            .build(),
        creature("Horned Turtle", "2U", "Creature — Turtle", 1, 4)
            .oracle("")
            .flavor("Its shell is a shield, and its patience is a weapon.")
            .meta(Common, "M12", "58", "Anthony S. Waters")
            .build(),
        creature("Wind Drake", "2U", "Creature — Drake", 2, 2)
            .kw(Keyword::Flying)
            .oracle("Flying")
            .flavor("Wings of storm, heart of mischief.")
            .meta(Common, "M15", "76", "Steve Prescott")
            .build(),
        creature("Wall of Frost", "1UU", "Creature — Wall", 0, 7)
            .kw(Keyword::Defender)
            .ability(trigger(
                TriggerCondition::Blocks(sel_self()),
                "Whenever Wall of Frost blocks a creature, that creature doesn't untap during its controller's next untap step.",
                Effect::Freeze {
                    target: ObjRef::TriggerObject,
                },
            ))
            .oracle("Defender\nWhenever Wall of Frost blocks a creature, that creature doesn't untap during its controller's next untap step.")
            .flavor("Its chill outlasts the winter that made it.")
            .meta(Uncommon, "M13", "72", "Erica Yang")
            .build(),
        creature("Snapping Drake", "3U", "Creature — Drake", 3, 2)
            .kw(Keyword::Flying)
            .oracle("Flying")
            .flavor("It snaps first and considers the consequences never.")
            .meta(Common, "M13", "68", "Chippy")
            .build(),
        creature("Thieving Magpie", "3U", "Creature — Bird", 1, 3)
            .kw(Keyword::Flying)
            .ability(trigger(
                TriggerCondition::DealsCombatDamageToPlayer(sel_self()),
                "Whenever Thieving Magpie deals combat damage to a player, draw a card.",
                draw(1, PlayerRef::You),
            ))
            .oracle("Flying\nWhenever Thieving Magpie deals combat damage to a player, draw a card.")
            .flavor("It collects what glitters, including secrets.")
            .meta(Uncommon, "10E", "112", "Una Fricker")
            .build(),
        creature("Air Elemental", "3UU", "Creature — Elemental", 4, 4)
            .kw(Keyword::Flying)
            .oracle("Flying")
            .flavor("Nothing to see. Everything to fear.")
            .meta(Uncommon, "M12", "43", "Adam Paquette")
            .build(),
        // --- encantamentos ---
        aura("Mind Control", "3UU", t_creature())
            .ability(etb(
                "You control enchanted creature.",
                Effect::GainControl {
                    target: ObjRef::Attached,
                    player: PlayerRef::You,
                    duration: Duration::WhileSourcePresent,
                },
            ))
            .oracle("Enchant creature\nYou control enchanted creature.")
            .flavor("The mind is a fortress with the gate left open.")
            .meta(Uncommon, "M13", "58", "Christopher Moeller")
            .build(),
        // --- contramágica ---
        instant("Counterspell", "UU")
            .target(t_any_spell())
            .spell(Effect::CounterSpell {
                target: target0(),
                unless_pays: None,
            })
            .oracle("Counter target spell.")
            .flavor("\"You have no idea how much effort it took to not exist.\"")
            .meta(Common, "7ED", "67", "Mark Zug")
            .build(),
        instant("Cancel", "1UU")
            .target(t_any_spell())
            .spell(Effect::CounterSpell {
                target: target0(),
                unless_pays: None,
            })
            .oracle("Counter target spell.")
            .meta(Common, "M15", "45", "Jason Chan")
            .build(),
        instant("Essence Scatter", "1U")
            .target(t_creature_spell())
            .spell(Effect::CounterSpell {
                target: target0(),
                unless_pays: None,
            })
            .oracle("Counter target creature spell.")
            .flavor("The summons was answered. The answer was not.")
            .meta(Common, "M19", "54", "Slawomir Maniak")
            .build(),
        instant("Negate", "1U")
            .target(t_spell(
                Filter::Not(Box::new(Filter::HasType(CardType::Creature))),
                "alvo de mágica que não seja de criatura",
            ))
            .spell(Effect::CounterSpell {
                target: target0(),
                unless_pays: None,
            })
            .oracle("Counter target noncreature spell.")
            .meta(Common, "M20", "69", "Steve Argyle")
            .build(),
        instant("Mana Leak", "1U")
            .target(t_any_spell())
            .spell(Effect::CounterSpell {
                target: target0(),
                unless_pays: Some(Cost::Mana(mana("3").symbols)),
            })
            .oracle("Counter target spell unless its controller pays {3}.")
            .flavor("The spell went in. Something less came out.")
            .meta(Common, "M12", "62", "Steven Belledin")
            .build(),
        instant("Exclude", "2U")
            .target(t_creature_spell())
            .spell(Effect::seq([
                Effect::CounterSpell {
                    target: target0(),
                    unless_pays: None,
                },
                draw(1, PlayerRef::You),
            ]))
            .oracle("Counter target creature spell. If that spell is countered this way, draw a card.")
            .meta(Common, "M20", "56", "Chris Rahn")
            .build(),
        instant("Dismiss", "3U")
            .target(t_any_spell())
            .spell(Effect::seq([
                Effect::CounterSpell {
                    target: target0(),
                    unless_pays: None,
                },
                draw(1, PlayerRef::You),
            ]))
            .oracle("Counter target spell. Draw a card.")
            .flavor("\"Next.\"")
            .meta(Uncommon, "TMP", "62", "Andrew Robinson")
            .build(),
        // --- bounce e tempo ---
        instant("Unsummon", "U")
            .target(t_creature())
            .spell(bounce(target0()))
            .oracle("Return target creature to its owner's hand.")
            .flavor("The summoner's second thought is the creature's last.")
            .meta(Common, "M19", "70", "Nils Hamm")
            .build(),
        instant("Vapor Snag", "U")
            .target(t_creature())
            // A perda de vida vem antes do retorno: depois do bounce o objeto
            // deixou o campo e a referência de controlador não existe mais.
            .spell(Effect::seq([
                Effect::LoseLife {
                    amount: Value::c(1),
                    player: PlayerRef::ControllerOf(Box::new(target0())),
                },
                bounce(target0()),
            ]))
            .oracle("Return target creature to its owner's hand. Its controller loses 1 life.")
            .meta(Common, "NPH", "43", "Steven Belledin")
            .build(),
        instant("Repulse", "2U")
            .target(t_creature())
            .spell(Effect::seq([bounce(target0()), draw(1, PlayerRef::You)]))
            .oracle("Return target creature to its owner's hand. Draw a card.")
            .meta(Common, "INV", "60", "Ron Walotsky")
            .build(),
        instant("Boomerang", "UU")
            .target(t_permanent())
            .spell(bounce(target0()))
            .oracle("Return target permanent to its owner's hand.")
            .flavor("What goes around comes around, and then goes around again.")
            .meta(Common, "9ED", "68", "Puddnhead")
            .build(),
        sorcery("Time Ebb", "2U")
            .target(t_creature())
            .spell(Effect::PutOnTopOfLibrary { target: target0() })
            .oracle("Put target creature on top of its owner's library.")
            .flavor("A moment undone is a moment repeated.")
            .meta(Common, "9ED", "104", "Adam Rex")
            .build(),
        sorcery("Sleep", "2UU")
            .target(t_player())
            .spell(for_each(
                Selector {
                    zone: mtg_core::ir::ZoneScope::Battlefield,
                    filter: Filter::creature(),
                    owner_scope: Some(PlayerRef::Target(0)),
                    max: None,
                },
                Effect::seq([
                    Effect::Tap {
                        target: ObjRef::Selected,
                    },
                    Effect::Freeze {
                        target: ObjRef::Selected,
                    },
                ]),
            ))
            .oracle("Tap all creatures target player controls. Those creatures don't untap during their controller's next untap step.")
            .flavor("Dream of victory. Wake to defeat.")
            .meta(Uncommon, "M13", "67", "Howard Lyon")
            .build(),
        // --- compra ---
        sorcery("Preordain", "U")
            .spell(Effect::seq([
                Effect::Scry {
                    count: Value::c(2),
                    player: PlayerRef::You,
                },
                draw(1, PlayerRef::You),
            ]))
            .oracle("Scry 2, then draw a card.")
            .flavor("The future is a book already written. Turn the page early.")
            .meta(Common, "M11", "78", "Ryan Pancoast")
            .build(),
        instant("Opt", "U")
            .spell(Effect::seq([
                Effect::Scry {
                    count: Value::c(1),
                    player: PlayerRef::You,
                },
                draw(1, PlayerRef::You),
            ]))
            .oracle("Scry 1.\nDraw a card.")
            .meta(Common, "DOM", "60", "Cliff Childs")
            .build(),
        sorcery("Divination", "2U")
            .spell(draw(2, PlayerRef::You))
            .oracle("Draw two cards.")
            .flavor("The stars know. The trick is asking politely.")
            .meta(Common, "M19", "52", "Howard Lyon")
            .build(),
        sorcery("Concentrate", "2UU")
            .spell(draw(3, PlayerRef::You))
            .oracle("Draw three cards.")
            .flavor("Study is the shortest road to power.")
            .meta(Uncommon, "10E", "76", "Adam Rex")
            .build(),
        instant("Jace's Ingenuity", "3UU")
            .spell(draw(3, PlayerRef::You))
            .oracle("Draw three cards.")
            .meta(Uncommon, "M11", "58", "Steven Belledin")
            .build(),
    ]
}
