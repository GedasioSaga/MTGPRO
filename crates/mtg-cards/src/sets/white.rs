//! Branco: criaturas pequenas eficientes, voadores, remoção condicional e
//! varredura. É a cor que sustenta o deck agressivo de soldados e a metade
//! branca do deck de controle.

use mtg_core::card::{CardDef, StaticMod, TriggerCondition};
use mtg_core::ir::{
    Duration, Effect, Filter, Keyword, ObjRef, PlayerRef, Selector, TimingRestriction, Value,
    ZoneScope,
};
use mtg_core::mana::Color;
use mtg_core::types::{CounterKind, Rarity::*};

use crate::builders::*;

/// Ficha 1/1 branca de Soldado — usada por três cartas diferentes.
fn soldier_token() -> mtg_core::ir::TokenSpec {
    token("Soldier", "Creature — Soldier", &[Color::White], 1, 1)
}

pub fn cards() -> Vec<CardDef> {
    vec![
        creature("Elite Vanguard", "W", "Creature — Human Soldier", 2, 1)
            .oracle("")
            .flavor("\"I am the first line of defense, and the last one you'll ever see.\"")
            .meta(Uncommon, "M10", "9", "Greg Staples")
            .build(),
        creature("Savannah Lions", "W", "Creature — Cat", 2, 1)
            .oracle("")
            .flavor("The lions of the savannah hunt in silence and strike in unison.")
            .meta(Rare, "M12", "31", "Jesper Ejsing")
            .build(),
        creature("Suntail Hawk", "W", "Creature — Bird", 1, 1)
            .kw(Keyword::Flying)
            .oracle("Flying")
            .flavor("It dives from the sun's glare, where no eye can follow.")
            .meta(Common, "M10", "36", "Jim Nelson")
            .build(),
        creature("Soul Warden", "W", "Creature — Human Cleric", 1, 1)
            .ability(trigger(
                // Qualquer criatura que não seja ela mesma, de qualquer controlador.
                TriggerCondition::EntersBattlefield(Selector::battlefield(Filter::all([
                    Filter::creature(),
                    Filter::IsOther,
                ]))),
                "Whenever another creature enters, you gain 1 life.",
                gain_life(1, PlayerRef::You),
            ))
            .oracle("Whenever another creature enters, you gain 1 life.")
            .flavor("\"Every life is a gift. Guard it well.\"")
            .meta(Uncommon, "M15", "35", "Steve Prescott")
            .build(),
        creature("Gideon's Lawkeeper", "W", "Creature — Human Soldier", 1, 1)
            .ability(activated_targeted(
                tap_mana_cost("W"),
                vec![t_creature()],
                Effect::Tap {
                    target: target0(),
                },
                TimingRestriction::Instant,
                "{W}, {T}: Tap target creature.",
            ))
            .oracle("{W}, {T}: Tap target creature.")
            .meta(Common, "M12", "13", "Jason Chan")
            .build(),
        creature("Leonin Skyhunter", "WW", "Creature — Cat Knight", 2, 2)
            .kw(Keyword::Flying)
            .oracle("Flying")
            .flavor("The leonin patrol the skies of Mirrodin on borrowed wings.")
            .meta(Uncommon, "M13", "18", "Wayne Reynolds")
            .build(),
        creature("Wall of Omens", "1W", "Creature — Wall", 0, 4)
            .kw(Keyword::Defender)
            .ability(etb(
                "When Wall of Omens enters, draw a card.",
                draw(1, PlayerRef::You),
            ))
            .oracle("Defender\nWhen Wall of Omens enters, draw a card.")
            .flavor("Those who gaze into it see what must be done.")
            .meta(Uncommon, "ROE", "40", "Howard Lyon")
            .build(),
        creature("Ajani's Pridemate", "1W", "Creature — Cat Soldier", 2, 2)
            .ability(trigger(
                TriggerCondition::LifeGained(PlayerRef::You),
                "Whenever you gain life, put a +1/+1 counter on Ajani's Pridemate.",
                counters(ObjRef::SelfObject, CounterKind::PlusOnePlusOne, 1),
            ))
            .oracle("Whenever you gain life, put a +1/+1 counter on Ajani's Pridemate.")
            .meta(Uncommon, "M14", "3", "Jesper Ejsing")
            .build(),
        creature("Precinct Captain", "WW", "Creature — Human Soldier", 2, 2)
            .kw(Keyword::FirstStrike)
            .ability(trigger(
                TriggerCondition::DealsCombatDamageToPlayer(sel_self()),
                "Whenever Precinct Captain deals combat damage to a player, create a 1/1 white Soldier creature token.",
                create_tokens(soldier_token(), 1),
            ))
            .oracle("First strike\nWhenever Precinct Captain deals combat damage to a player, create a 1/1 white Soldier creature token.")
            .meta(Rare, "RTR", "22", "Chris Rahn")
            .build(),
        creature("Benalish Knight", "2W", "Creature — Human Knight", 2, 2)
            .kws([Keyword::Flash, Keyword::FirstStrike])
            .oracle("Flash\nFirst strike")
            .flavor("Benalia's knights ride to war before the news of it arrives.")
            .meta(Common, "DOM", "6", "Zoltan Boros")
            .build(),
        creature("Griffin Sentinel", "2W", "Creature — Griffin", 1, 3)
            .kws([Keyword::Flying, Keyword::Vigilance])
            .oracle("Flying, vigilance")
            .flavor("It never sleeps, and it never blinks.")
            .meta(Common, "M13", "12", "Jesper Ejsing")
            .build(),
        creature("Angelic Wall", "1W", "Creature — Wall", 0, 4)
            .kws([Keyword::Defender, Keyword::Flying])
            .oracle("Defender, flying")
            .flavor("\"The Ancestor left us this gift: a wall that could fly.\"")
            .meta(Common, "M12", "3", "Greg Staples")
            .build(),
        creature("Skyhunter Skirmisher", "1WW", "Creature — Cat Knight", 1, 1)
            .kws([Keyword::Flying, Keyword::DoubleStrike])
            .oracle("Flying, double strike")
            .meta(Uncommon, "MRD", "26", "Greg Staples")
            .build(),
        creature("Attended Knight", "3W", "Creature — Human Knight", 2, 2)
            .kw(Keyword::FirstStrike)
            .ability(etb(
                "When Attended Knight enters, create a 1/1 white Soldier creature token.",
                create_tokens(soldier_token(), 1),
            ))
            .oracle("First strike\nWhen Attended Knight enters, create a 1/1 white Soldier creature token.")
            .meta(Common, "M14", "4", "Steve Prescott")
            .build(),
        creature("Angel of Mercy", "4W", "Creature — Angel", 3, 3)
            .kw(Keyword::Flying)
            .ability(etb(
                "When Angel of Mercy enters, you gain 3 life.",
                gain_life(3, PlayerRef::You),
            ))
            .oracle("Flying\nWhen Angel of Mercy enters, you gain 3 life.")
            .flavor("Every tear shed is a drop of immortality.")
            .meta(Uncommon, "M12", "2", "Volkan Baga")
            .build(),
        creature("Serra Angel", "3WW", "Creature — Angel", 4, 4)
            .kws([Keyword::Flying, Keyword::Vigilance])
            .oracle("Flying, vigilance")
            .flavor("Born with wings of light and a sword of faith.")
            .meta(Uncommon, "M10", "34", "Greg Staples")
            .build(),
        creature("Captain of the Watch", "4WW", "Creature — Human Soldier", 3, 3)
            .kw(Keyword::Vigilance)
            .ability(anthem(
                sel_other_soldiers(),
                1,
                1,
                "Other Soldier creatures you control get +1/+1 and have vigilance.",
            ))
            .ability(static_ability(
                sel_other_soldiers(),
                StaticMod::GrantKeywords(vec![Keyword::Vigilance]),
                "Other Soldier creatures you control have vigilance.",
            ))
            .ability(etb(
                "When Captain of the Watch enters, create three 1/1 white Soldier creature tokens.",
                create_tokens(soldier_token(), 3),
            ))
            .oracle("Vigilance\nOther Soldier creatures you control get +1/+1 and have vigilance.\nWhen Captain of the Watch enters, create three 1/1 white Soldier creature tokens.")
            .meta(Rare, "M12", "6", "Greg Staples")
            .build(),
        creature("Sun Titan", "4WW", "Creature — Giant", 6, 6)
            .kw(Keyword::Vigilance)
            .ability(trigger_targeted(
                TriggerCondition::Any(vec![
                    TriggerCondition::EntersBattlefield(sel_self()),
                    TriggerCondition::Attacks(sel_self()),
                ]),
                "Whenever Sun Titan enters or attacks, return target permanent card with mana value 3 or less from your graveyard to the battlefield.",
                vec![spec(
                    mtg_core::ir::TargetKind::Object(Selector {
                        zone: ZoneScope::Graveyard,
                        filter: Filter::ManaValueAtMost(3),
                        owner_scope: Some(PlayerRef::You),
                        max: None,
                    }),
                    "alvo de card de permanente com valor de mana 3 ou menos no seu cemitério",
                )],
                Effect::ReturnFromGraveyardToBattlefield { target: target0() },
            ))
            .oracle("Vigilance\nWhenever Sun Titan enters or attacks, return target permanent card with mana value 3 or less from your graveyard to the battlefield.")
            .flavor("Dawn does not forget what the night has taken.")
            .meta(Mythic, "M11", "35", "Jesper Ejsing")
            .build(),
        // --- encantamentos ---
        aura("Pacifism", "1W", t_creature())
            .ability(etb(
                "Enchanted creature can't attack or block.",
                Effect::CantAttackOrBlock {
                    target: ObjRef::Attached,
                    duration: Duration::WhileSourcePresent,
                },
            ))
            .oracle("Enchant creature\nEnchanted creature can't attack or block.")
            .flavor("Fighting is a poor way to settle a disagreement.")
            .meta(Common, "M10", "22", "Kev Walker")
            .build(),
        enchantment("Glorious Anthem", "1WW")
            .ability(anthem(
                sel_your_creatures(),
                1,
                1,
                "Creatures you control get +1/+1.",
            ))
            .oracle("Creatures you control get +1/+1.")
            .flavor("The soldiers sang, and the walls of the enemy fell.")
            .meta(Rare, "M10", "13", "Rob Alexander")
            .build(),
        enchantment("Honor of the Pure", "1W")
            .ability(anthem(
                Selector::battlefield(Filter::all([
                    Filter::creature(),
                    Filter::HasColor(Color::White),
                ]))
                .yours(),
                1,
                1,
                "White creatures you control get +1/+1.",
            ))
            .oracle("White creatures you control get +1/+1.")
            .meta(Rare, "M10", "16", "Greg Staples")
            .build(),
        enchantment("Oblivion Ring", "2W")
            .ability(etb_targeted(
                "When Oblivion Ring enters, exile another target nonland permanent.",
                vec![t_object(
                    Filter::all([
                        Filter::Not(Box::new(f_type(mtg_core::types::CardType::Land))),
                        Filter::IsOther,
                    ]),
                    "outro alvo de permanente que não seja terreno",
                )],
                Effect::Exile {
                    target: target0(),
                    until_source_leaves: true,
                },
            ))
            .oracle("When Oblivion Ring enters, exile another target nonland permanent.\nWhen Oblivion Ring leaves the battlefield, return the exiled card to the battlefield under its owner's control.")
            .meta(Common, "M10", "20", "Jim Murray")
            .build(),
        // --- mágicas ---
        instant("Swords to Plowshares", "W")
            .target(t_creature())
            // Ganha vida antes do exílio: depois, o poder da criatura sumiu da zona.
            .spell(Effect::seq([
                Effect::GainLife {
                    amount: Value::PowerOf(target0()),
                    player: PlayerRef::ControllerOf(Box::new(target0())),
                },
                Effect::Exile {
                    target: target0(),
                    until_source_leaves: false,
                },
            ]))
            .oracle("Exile target creature. Its controller gains life equal to its power.")
            .flavor("The smallest scrap of iron makes a fine plow.")
            .meta(Uncommon, "3ED", "36", "Jeff A. Menges")
            .build(),
        instant("Disenchant", "1W")
            .target(t_object(
                f_artifact_or_enchantment(),
                "alvo de artefato ou encantamento",
            ))
            .spell(destroy(target0()))
            .oracle("Destroy target artifact or enchantment.")
            .flavor("Some things are better unmade.")
            .meta(Common, "9ED", "10", "Kev Walker")
            .build(),
        instant("Mighty Leap", "1W")
            .target(t_creature())
            .spell(Effect::seq([
                pump(target0(), 2, 0, Duration::EndOfTurn),
                grant(target0(), vec![Keyword::Flying], Duration::EndOfTurn),
            ]))
            .oracle("Target creature gets +2/+0 and gains flying until end of turn.")
            .flavor("Faith is the only wing a soldier needs.")
            .meta(Common, "M13", "23", "Greg Staples")
            .build(),
        instant("Raise the Alarm", "1W")
            .spell(create_tokens(soldier_token(), 2))
            .oracle("Create two 1/1 white Soldier creature tokens.")
            .flavor("The bell rings once for warning and twice for war.")
            .meta(Common, "M15", "27", "Chris Rahn")
            .build(),
        instant("Divine Verdict", "3W")
            .target(t_creature_filtered(
                Filter::Or(vec![Filter::Attacking, Filter::Blocking]),
                "alvo de criatura atacante ou bloqueadora",
            ))
            .spell(destroy(target0()))
            .oracle("Destroy target attacking or blocking creature.")
            .flavor("Judgment comes swiftly to those who draw the first blade.")
            .meta(Common, "M13", "9", "Kev Walker")
            .build(),
        instant("Revitalize", "1W")
            .spell(Effect::seq([
                gain_life(3, PlayerRef::You),
                draw(1, PlayerRef::You),
            ]))
            .oracle("You gain 3 life. Draw a card.")
            .meta(Common, "M19", "35", "Jason A. Engle")
            .build(),
        sorcery("Smite the Monstrous", "3W")
            .target(t_creature_filtered(
                Filter::PowerAtLeast(4),
                "alvo de criatura com poder 4 ou maior",
            ))
            .spell(destroy(target0()))
            .oracle("Destroy target creature with power 4 or greater.")
            .flavor("The bigger they are, the harder they fall.")
            .meta(Common, "M19", "36", "Zack Stella")
            .build(),
        sorcery("Wrath of God", "2WW")
            .spell(for_each(sel_creatures(), destroy_hard(ObjRef::Selected)))
            .oracle("Destroy all creatures. They can't be regenerated.")
            .flavor("Wipe the slate clean; begin again.")
            .meta(Rare, "10E", "62", "Kev Walker")
            .build(),
        sorcery("Day of Judgment", "2WW")
            .spell(for_each(sel_creatures(), destroy(ObjRef::Selected)))
            .oracle("Destroy all creatures.")
            .flavor("The Day of Judgment came, and the sun rose on an empty field.")
            .meta(Rare, "ZEN", "9", "Vincent Proce")
            .build(),
    ]
}

/// "Outras criaturas Soldado que você controla" — o corpo do Captain of the Watch.
fn sel_other_soldiers() -> Selector {
    Selector::battlefield(Filter::all([
        Filter::creature(),
        Filter::HasSubtype("Soldier".to_string()),
        Filter::IsOther,
    ]))
    .yours()
}
