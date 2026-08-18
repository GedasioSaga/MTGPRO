//! Preto: remoção incondicional, descarte, dreno de vida e criaturas com
//! toque mortal. Metade do deck de meio-de-curva.

use mtg_core::card::{CardDef, StaticMod, TriggeredAbility, TriggerCondition};
use mtg_core::ir::{
    Condition, Cost, Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector,
    TimingRestriction, Value,
};
use mtg_core::mana::{Color, ManaSymbol};
use mtg_core::types::CardType;
use mtg_core::types::Rarity::*;

use crate::builders::*;

/// Pântanos que você controla — Nightmare e Corrupt escalam por eles.
fn swamps_you_control() -> Value {
    Value::Count(
        Selector::battlefield(Filter::HasSubtype("Swamp".to_string())).yours(),
    )
}

pub fn cards() -> Vec<CardDef> {
    vec![
        // --- criaturas ---
        creature("Typhoid Rats", "B", "Creature — Rat", 1, 1)
            .kw(Keyword::Deathtouch)
            .oracle("Deathtouch")
            .flavor("One bite is all it takes, no matter how large the beast.")
            .meta(Common, "M14", "111", "Ryan Alexander Lee")
            .build(),
        creature("Child of Night", "1B", "Creature — Vampire", 2, 1)
            .kw(Keyword::Lifelink)
            .oracle("Lifelink")
            .flavor("\"The night is generous. It gives what it takes.\"")
            .meta(Common, "M15", "94", "Steve Argyle")
            .build(),
        creature("Ravenous Rats", "1B", "Creature — Rat", 1, 1)
            .ability(etb_targeted(
                "When Ravenous Rats enters, target opponent discards a card.",
                vec![t_opponent()],
                Effect::Discard {
                    count: Value::c(1),
                    player: PlayerRef::Target(0),
                    filter: Filter::Any,
                    random: false,
                },
            ))
            .oracle("When Ravenous Rats enters, target opponent discards a card.")
            .flavor("They eat what you feed them, then what you don't.")
            .meta(Common, "10E", "158", "Kev Walker")
            .build(),
        creature("Walking Corpse", "1B", "Creature — Zombie", 2, 2)
            .oracle("")
            .flavor("It walks because no one told it to stop.")
            .meta(Common, "M15", "116", "Karl Kopinski")
            .build(),
        creature("Vampire Interloper", "1B", "Creature — Vampire Scout", 2, 1)
            .kw(Keyword::Flying)
            .ability(static_ability(
                sel_self(),
                StaticMod::CantBlock,
                "Vampire Interloper can't block.",
            ))
            .oracle("Flying\nVampire Interloper can't block.")
            .flavor("It hunts. It does not guard.")
            .meta(Common, "ISD", "111", "Kev Walker")
            .build(),
        creature("Blood Artist", "1B", "Creature — Vampire", 0, 1)
            .ability(mtg_core::card::Ability::Triggered(TriggeredAbility {
                trigger: TriggerCondition::Dies(sel_creatures()),
                intervening_if: Condition::Always,
                targets: vec![t_player()],
                effect: Effect::seq([
                    lose_life(1, PlayerRef::Target(0)),
                    gain_life(1, PlayerRef::You),
                ]),
                optional: false,
                once_per_turn: false,
                // CR 603.6c: o gatilho enxerga a própria morte do campo de batalha.
                triggers_from_graveyard: true,
                text: "Whenever Blood Artist or another creature dies, target player loses 1 life and you gain 1 life.".to_string(),
            }))
            .oracle("Whenever Blood Artist or another creature dies, target player loses 1 life and you gain 1 life.")
            .flavor("\"Every death is a brushstroke.\"")
            .meta(Uncommon, "AVR", "84", "Johannes Voss")
            .build(),
        creature("Vampire Nighthawk", "1BB", "Creature — Vampire Shaman", 2, 3)
            .kws([Keyword::Flying, Keyword::Deathtouch, Keyword::Lifelink])
            .oracle("Flying, deathtouch, lifelink")
            .flavor("Zendikar's night has teeth.")
            .meta(Uncommon, "M21", "121", "Jason Chan")
            .build(),
        creature("Nantuko Husk", "2B", "Creature — Zombie Insect Warrior", 2, 2)
            .ability(activated(
                Cost::Sacrifice(1, Filter::creature()),
                pump(ObjRef::SelfObject, 2, 2, Duration::EndOfTurn),
                TimingRestriction::Instant,
                "Sacrifice a creature: Nantuko Husk gets +2/+2 until end of turn.",
            ))
            .oracle("Sacrifice a creature: Nantuko Husk gets +2/+2 until end of turn.")
            .flavor("It hungers for what it once protected.")
            .meta(Uncommon, "ORI", "111", "Wayne Reynolds")
            .build(),
        creature("Phyrexian Rager", "2B", "Creature — Horror", 2, 2)
            .ability(etb(
                "When Phyrexian Rager enters, you draw a card and you lose 1 life.",
                Effect::seq([draw(1, PlayerRef::You), lose_life(1, PlayerRef::You)]),
            ))
            .oracle("When Phyrexian Rager enters, you draw a card and you lose 1 life.")
            .flavor("Knowledge is bought with blood, as all things are.")
            .meta(Common, "APC", "56", "Ron Spears")
            .build(),
        creature("Nekrataal", "2BB", "Creature — Human Assassin", 2, 1)
            .kw(Keyword::FirstStrike)
            .ability(etb_targeted(
                "When Nekrataal enters, destroy target nonartifact, nonblack creature. That creature can't be regenerated.",
                vec![t_creature_filtered(
                    Filter::Not(Box::new(Filter::Or(vec![
                        Filter::HasType(CardType::Artifact),
                        Filter::HasColor(Color::Black),
                    ]))),
                    "alvo de criatura que não seja artefato nem preta",
                )],
                destroy_hard(target0()),
            ))
            .oracle("First strike\nWhen Nekrataal enters, destroy target nonartifact, nonblack creature. That creature can't be regenerated.")
            .flavor("He does not negotiate. He concludes.")
            .meta(Uncommon, "9ED", "148", "Pete Venters")
            .build(),
        creature("Bloodhunter Bat", "3B", "Creature — Bat", 2, 2)
            .kw(Keyword::Flying)
            .ability(etb_targeted(
                "When Bloodhunter Bat enters, target opponent loses 2 life and you gain 2 life.",
                vec![t_opponent()],
                Effect::seq([
                    lose_life(2, PlayerRef::Target(0)),
                    gain_life(2, PlayerRef::You),
                ]),
            ))
            .oracle("Flying\nWhen Bloodhunter Bat enters, target opponent loses 2 life and you gain 2 life.")
            .meta(Common, "M13", "88", "Chris Rahn")
            .build(),
        creature("Barony Vampire", "3B", "Creature — Vampire", 3, 2)
            .oracle("")
            .flavor("It keeps the old title and the old appetites.")
            .meta(Common, "M12", "85", "Anthony Palumbo")
            .build(),
        creature("Zombie Goliath", "4B", "Creature — Zombie Giant", 5, 3)
            .oracle("")
            .flavor("Big enough to be two problems at once.")
            .meta(Common, "M15", "117", "Karl Kopinski")
            .build(),
        creature("Nightmare", "5B", "Creature — Nightmare Horse", 0, 0)
            .kw(Keyword::Flying)
            .ability(static_ability(
                sel_self(),
                StaticMod::SetPT(swamps_you_control(), swamps_you_control()),
                "Nightmare's power and toughness are each equal to the number of Swamps you control.",
            ))
            .oracle("Flying\nNightmare's power and toughness are each equal to the number of Swamps you control.")
            .flavor("The swamp rides out to meet you.")
            .meta(Rare, "M12", "94", "Carl Critchlow")
            .build(),
        // --- encantamento ---
        enchantment("Bad Moon", "1B")
            .ability(anthem(
                Selector::battlefield(Filter::all([
                    Filter::creature(),
                    Filter::HasColor(Color::Black),
                ])),
                1,
                0,
                "Black creatures get +1/+0.",
            ))
            .oracle("Black creatures get +1/+0.")
            .flavor("Under it, every shadow stands a little taller.")
            .meta(Rare, "4ED", "17", "Jeff A. Menges")
            .build(),
        // --- remoção ---
        instant("Doom Blade", "1B")
            .target(t_creature_filtered(
                Filter::Not(Box::new(Filter::HasColor(Color::Black))),
                "alvo de criatura que não seja preta",
            ))
            .spell(destroy(target0()))
            .oracle("Destroy target nonblack creature.")
            .flavor("One cut. One conclusion.")
            .meta(Common, "M13", "89", "Chris Rahn")
            .build(),
        instant("Murder", "1BB")
            .target(t_creature())
            .spell(destroy(target0()))
            .oracle("Destroy target creature.")
            .flavor("\"It was simple. It was quiet. It was done.\"")
            .meta(Common, "M19", "112", "Slawomir Maniak")
            .build(),
        instant("Victim of Night", "BB")
            .target(t_creature_filtered(
                Filter::Not(Box::new(Filter::Or(vec![
                    Filter::HasSubtype("Vampire".to_string()),
                    Filter::HasSubtype("Werewolf".to_string()),
                    Filter::HasSubtype("Zombie".to_string()),
                ]))),
                "alvo de criatura que não seja Vampiro, Lobisomem nem Zumbi",
            ))
            .spell(destroy(target0()))
            .oracle("Destroy target creature that isn't a Vampire, Werewolf, or Zombie.")
            .meta(Common, "ISD", "116", "Kev Walker")
            .build(),
        instant("Diabolic Edict", "1B")
            .target(t_player())
            .spell(Effect::Sacrifice {
                player: PlayerRef::Target(0),
                count: Value::c(1),
                filter: Filter::creature(),
            })
            .oracle("Target player sacrifices a creature.")
            .flavor("The debt is collected by whichever hand is nearest.")
            .meta(Common, "TMP", "127", "Jon J Muth")
            .build(),
        instant("Grasp of Darkness", "BB")
            .target(t_creature())
            .spell(pump(target0(), -4, -4, Duration::EndOfTurn))
            .oracle("Target creature gets -4/-4 until end of turn.")
            .flavor("The dark does not hold. It squeezes.")
            .meta(Common, "BFZ", "108", "Chris Rallis")
            .build(),
        instant("Sorin's Thirst", "1B")
            .target(t_creature())
            .spell(Effect::seq([dmg(2, target0()), gain_life(2, PlayerRef::You)]))
            .oracle("Sorin's Thirst deals 2 damage to target creature and you gain 2 life.")
            .meta(Common, "M12", "104", "Steve Argyle")
            .build(),
        sorcery("Innocent Blood", "B")
            .spell(Effect::Sacrifice {
                player: PlayerRef::Each,
                count: Value::c(1),
                filter: Filter::creature(),
            })
            .oracle("Each player sacrifices a creature.")
            .flavor("The guilty are never the ones who pay.")
            .meta(Common, "ODY", "132", "Ron Spencer")
            .build(),
        sorcery("Languish", "2BB")
            .spell(for_each(
                sel_creatures(),
                pump(ObjRef::Selected, -4, -4, Duration::EndOfTurn),
            ))
            .oracle("All creatures get -4/-4 until end of turn.")
            .flavor("The field went quiet, and stayed quiet.")
            .meta(Rare, "ORI", "107", "Chris Rallis")
            .build(),
        sorcery("Cower in Fear", "2B")
            .spell(for_each(
                sel_opponent_creatures(),
                pump(ObjRef::Selected, -1, -1, Duration::EndOfTurn),
            ))
            .oracle("Creatures your opponents control get -1/-1 until end of turn.")
            .flavor("Courage is the first thing the dark takes.")
            .meta(Common, "ISD", "97", "Kev Walker")
            .build(),
        // --- descarte e compra ---
        sorcery("Duress", "B")
            .target(t_opponent())
            .spell(Effect::Discard {
                count: Value::c(1),
                player: PlayerRef::Target(0),
                filter: f_noncreature_nonland(),
                random: false,
            })
            .oracle("Target opponent reveals their hand. You choose a noncreature, nonland card from it. That player discards that card.")
            .flavor("Plans are the easiest thing to steal.")
            .meta(Common, "M13", "91", "Steve Prescott")
            .build(),
        sorcery("Mind Rot", "2B")
            .target(t_player())
            .spell(Effect::Discard {
                count: Value::c(2),
                player: PlayerRef::Target(0),
                filter: Filter::Any,
                random: false,
            })
            .oracle("Target player discards two cards.")
            .flavor("What was that idea again?")
            .meta(Common, "M19", "111", "Anthony Palumbo")
            .build(),
        sorcery("Sign in Blood", "BB")
            .target(t_player())
            .spell(Effect::seq([
                draw(2, PlayerRef::Target(0)),
                lose_life(2, PlayerRef::Target(0)),
            ]))
            .oracle("Target player draws two cards and loses 2 life.")
            .flavor("The ink is the price.")
            .meta(Common, "M13", "108", "Steve Argyle")
            .build(),
        sorcery("Read the Bones", "2B")
            .spell(Effect::seq([
                Effect::Scry {
                    count: Value::c(2),
                    player: PlayerRef::You,
                },
                draw(2, PlayerRef::You),
                lose_life(2, PlayerRef::You),
            ]))
            .oracle("Scry 2, then draw two cards. You lose 2 life.")
            .flavor("The future costs what the past did.")
            .meta(Common, "M15", "106", "Wesley Burt")
            .build(),
        instant("Dark Ritual", "B")
            .spell(Effect::AddMana {
                symbols: vec![
                    ManaSymbol::Colored(Color::Black),
                    ManaSymbol::Colored(Color::Black),
                    ManaSymbol::Colored(Color::Black),
                ],
                player: PlayerRef::You,
            })
            .oracle("Add {B}{B}{B}.")
            .flavor("Power flows to those who ask in the right voice.")
            .meta(Common, "7ED", "138", "Clyde Caldwell")
            .build(),
        // --- dreno ---
        sorcery("Corrupt", "5B")
            .target(t_any())
            .spell(Effect::seq([
                Effect::DealDamage {
                    amount: swamps_you_control(),
                    target: target0(),
                },
                Effect::GainLife {
                    amount: swamps_you_control(),
                    player: PlayerRef::You,
                },
            ]))
            .oracle("Corrupt deals X damage to any target, where X is the number of Swamps you control. You gain X life.")
            .flavor("The swamp collects its due with interest.")
            .meta(Uncommon, "M12", "87", "Steven Belledin")
            .build(),
        sorcery("Consume Spirit", "XB")
            .target(t_any())
            .spell(Effect::seq([
                Effect::DealDamage {
                    amount: Value::X,
                    target: target0(),
                },
                Effect::GainLife {
                    amount: Value::X,
                    player: PlayerRef::You,
                },
            ]))
            .oracle("Consume Spirit deals X damage to any target. You gain X life.")
            .flavor("What leaves one body must enter another.")
            .meta(Uncommon, "10E", "141", "Justin Sweet")
            .build(),
    ]
}
