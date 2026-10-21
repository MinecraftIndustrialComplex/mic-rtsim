// SPDX-License-Identifier: AGPL-3.0-or-later
//! The walkthrough end to end (PLAN.md §10.3): a goblin tribe, a polluting refinery and a logged
//! forest, run three ways, and restored from a save in the middle.

use rtsim_conflict::Step;
use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::scenario::{self, BACK_DAY, OFFLINE_DAY, Options, Outcome, Variant};
use rtsim_conflict::synth;
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::TICKS_PER_DAY;

fn run(variant: Variant, restart_day: Option<f64>) -> Outcome {
    scenario::run(&Options {
        restart_day,
        ..Options::new(7, 24, variant)
    })
}

/// The rungs of the feud the refinery's smog feeds: P14 (changed) gives it to the player who
/// spends the most time in the refinery's chunk.
fn settlers_rungs(o: &Outcome) -> Vec<Step> {
    assert_ne!(
        o.smog_owner, "the settlers",
        "the refinery's player owns its smog"
    );
    o.rungs
        .iter()
        .filter(|r| r.actor == o.smog_owner)
        .map(|r| r.step)
        .collect()
}

#[test]
fn ignored_the_band_marches_and_raids_while_the_player_is_offline() {
    let o = run(Variant::Ignore, None);
    let r = settlers_rungs(&o);
    let pos = |s: Step| r.iter().position(|x| *x == s);
    let (n, e, sc, ra) = (
        pos(Step::Notice).expect("notice"),
        pos(Step::Envoy).expect("envoy"),
        pos(Step::Scouts).expect("scouts"),
        pos(Step::Raid).expect("raid"),
    );
    assert!(n < e && e < sc && sc < ra, "the ladder in order: {r:?}");
    assert!(o.conflict.stats.demands >= 1 && o.conflict.stats.expired >= 1);
    // The raid while the owner is offline is resolved abstractly by the industry engine.
    let offline: Vec<_> = o
        .attacks
        .iter()
        .filter(|a| {
            let d = a.at.0 as f64 / TICKS_PER_DAY as f64;
            (OFFLINE_DAY..BACK_DAY).contains(&d) && a.skipped.is_none()
        })
        .collect();
    assert!(
        !offline.is_empty(),
        "an unwatched raid while offline: {:?}",
        o.attacks
    );
    let tun = Tunables::default().threat;
    for a in &offline {
        assert!(a.outcome.is_some());
        assert!(a.destroyed <= tun.destroy_cap_blocks, "capped");
    }
    // Applied on load (C18): the blocks it broke are broken once the player is back.
    let destroyed: u32 = offline.iter().map(|a| a.destroyed).sum();
    assert!(destroyed > 0, "the raid broke something");
    assert_eq!(
        o.blocks_broken as u32, destroyed,
        "applied when the district loaded"
    );
    assert!(o.reloaded_day.is_some());
    // Stolen stock went home to the haven and every book closes.
    assert!(!o.conflict.ledger.landed_from_players.is_empty());
    o.conflict.check(&o.world).expect("conflict books");
    o.fake.conservation().expect("industry world totals");
    // At home the player broke a band at the base: the director's waves, the war chief killed,
    // the heir holding the grudge.
    assert!(o.conflict.stats.raids_watched >= 1);
    assert!(o.conflict.stats.chiefs_killed >= 1 && o.conflict.stats.successions >= 1);
    let arcs: Vec<&String> = o
        .plugin
        .commands
        .iter()
        .map(|c| &c.1)
        .filter(|c| c.starts_with("BAND_MATERIALIZE") && c.contains("arc 112 blocks"))
        .collect();
    assert!(!arcs.is_empty(), "waves on the arc");
    for a in arcs {
        let n: u32 = a
            .split(": ")
            .nth(2)
            .and_then(|s| s.split(' ').next())
            .and_then(|s| s.parse().ok())
            .expect("members");
        assert!(n <= o.conflict.tun.spawn_quota, "{a}");
    }
    let f = o.conflict.faction_of_key(synth::CAMP).unwrap();
    let chief = o.conflict.factions[f as usize].chief;
    let player = o.world.knowledge.actors().find(
        rtsim_knowledge::ActorKind::Player,
        rtsim_world::interest::uuid_hash(synth::PLAYER.0, synth::PLAYER.1),
    );
    let feel = o
        .world
        .knowledge
        .sentiment(chief, player.unwrap(), o.world.now());
    assert!(feel < 0, "the heir inherited the grudge ({feel})");
}

#[test]
fn a_tribute_de_escalates() {
    let o = run(Variant::Tribute, None);
    let st = &o.conflict.stats;
    assert!(st.tributes >= 1);
    assert_eq!(st.parties[PartyKind::Raid.index()], 0, "no war band");
    assert_eq!(st.parties[PartyKind::Invasion.index()], 0);
    assert!(settlers_rungs(&o).iter().all(|s| *s <= Step::Envoy));
    assert!(
        !o.conflict.ledger.tribute.is_empty(),
        "tribute stocked at the haven"
    );
    assert_eq!(o.blocks_broken, 0);
    o.conflict.check(&o.world).expect("books");
}

#[test]
fn a_catalytic_converter_de_escalates() {
    let o = run(Variant::Converter, None);
    let st = &o.conflict.stats;
    assert!(st.cuts >= 1, "the demand met by cutting the signal");
    assert_eq!(st.parties[PartyKind::Raid.index()], 0);
    assert_eq!(st.parties[PartyKind::Scouts.index()], 0);
    // P14 (changed): the smog is the refinery player's, and that player's own logging (a direct
    // act) shares the feud, so it calms to notice rather than to nothing.
    assert!(
        *settlers_rungs(&o).last().unwrap() <= Step::Notice,
        "back to notice at most"
    );
    assert!(o.converter_day.is_some());
    o.conflict.check(&o.world).expect("books");
}

#[test]
fn a_restore_continues_with_the_same_state() {
    let whole = run(Variant::Ignore, None);
    for day in [2.0, 9.3, 15.8, 21.5] {
        let r = run(Variant::Ignore, Some(day));
        assert_eq!(r.hashes, whole.hashes, "restarted on day {day}");
        assert_eq!(
            r.plugin.commands, whole.plugin.commands,
            "same commands, day {day}"
        );
    }
}

/// The event archive's rows (VIEWER.md §10.1): every kind the conflict records is registered in
/// its group, and each walkthrough's story is there, positioned and with real ids.
#[test]
fn the_archive_records_each_walkthrough() {
    use rtsim_archive::kinds::{self, Kind};
    use rtsim_archive::record::{kinds_on, rows_on};
    use rtsim_archive::{Row, Severity};
    let common = [
        kinds::CONFLICT_FACTION,
        kinds::CONFLICT_STEP,
        kinds::CONFLICT_DEMAND,
        kinds::CONFLICT_PARTY,
        kinds::CONFLICT_PARTY_LEG,
        kinds::CONFLICT_PARTY_HOME,
    ];
    let runs: [(Variant, &[Kind]); 3] = [
        (
            Variant::Ignore,
            &[kinds::CONFLICT_UNMET, kinds::CONFLICT_RAID],
        ),
        (Variant::Tribute, &[kinds::CONFLICT_TRIBUTE]),
        (Variant::Converter, &[kinds::CONFLICT_COMPLIED]),
    ];
    for (v, own) in runs {
        let o = run(v, None);
        let got = kinds_on(&o.conflict.tape);
        for k in &got {
            assert_eq!(k.group(), "conflict", "{v:?}: {}", k.name());
            assert_eq!(kinds::by_name(k.name()), Some(*k));
        }
        for k in common.iter().chain(own) {
            assert!(got.contains(k), "{v:?}: no {} in {got:?}", k.name());
        }
        let rows = rows_on(&o.conflict.tape);
        for (k, dim, r) in &rows {
            assert_eq!(*dim, 0, "{v:?}: the walkthrough is in the overworld");
            match r {
                Row::Event(e) => {
                    assert_ne!(e.subject.value, 0, "{v:?}: {} has no subject", k.name());
                    assert!(e.x != 0 || e.z != 0, "{v:?}: {} at the origin", k.name());
                }
                Row::Leg(l) => assert_ne!(l.subject.value, 0),
            }
        }
        if v == Variant::Ignore {
            // The war band's raid on the refinery: major, with what it took.
            let raid = rows
                .iter()
                .find(|(k, _, _)| *k == kinds::CONFLICT_RAID)
                .expect("a raid");
            let Row::Event(e) = raid.2 else {
                panic!("a raid is an event")
            };
            assert_eq!(e.severity, Severity::Major);
            assert!(e.magnitude > 0 && e.payload, "{e:?}");
            // Its legs are a named mover's.
            assert!(rows.iter().any(|(k, _, r)| *k == kinds::CONFLICT_PARTY_LEG
                && matches!(r, Row::Leg(l) if l.severity == Severity::Notable)));
        }
    }
}
