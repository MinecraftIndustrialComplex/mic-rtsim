// SPDX-License-Identifier: AGPL-3.0-or-later
//! Villages renew (EQUILIBRIUM.md §2.4.3 and §2.4.4, slice S6; docs/overnight-choices-equilibrium.md
//! EQC150 to EQC183): the census moves a village's count both ways, a kill adds pressure, settlers
//! come to an emptied village after the wait, an unloaded one grows, and a Ribbit village refills
//! before it builds.
//!
//! The synthetic world's nodes: 0 to 5 human villages (pop 20, 18, 12, 15, 14, 10), 6 to 8 Ribbit
//! villages (12, 10, 10), 9 a haven, 10 a player base.

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_trade::build::Genesis;
use rtsim_world::inputs::{Input, apply};
use rtsim_world::{Command, World, WorldConfig, synth};
use std::collections::BTreeMap;

const BRINDLE: usize = 0;
const OAKFORD: usize = 1;
const MIREWATER: usize = 6;
const HAVEN: usize = 9;

fn config(renewal: bool) -> WorldConfig {
    let mut cfg = WorldConfig::new(7);
    cfg.check_every_event = true;
    cfg.genesis = Genesis::AtTarget;
    cfg.renewal = renewal;
    cfg
}

fn world() -> World {
    synth::build(config(true), false).0
}

fn day(d: u64) -> Tick {
    Tick(d * TICKS_PER_DAY)
}

/// A census at `at`: the world runs there first, as every input's does.
fn count(w: &mut World, i: usize, residents: u32, beds: u32, complete: bool, at: Tick) {
    w.run_until(at);
    w.census_village(i, residents, beds, complete, at);
}

// ---- EQC150 to EQC155: the census both ways ----------------------------------------------------

#[test]
fn every_village_has_a_row_at_its_adoption_guess() {
    let w = world();
    for i in 0..=8 {
        let r = w.village_row(i).expect("a village has a row");
        assert_eq!(r.members, w.nodes[i].local.pop, "node {i}");
        assert_eq!(u32::from(r.k), r.members, "node {i}");
        assert!(!r.seen);
    }
    assert!(w.village_row(HAVEN).is_none(), "a haven has none");
    assert!(w.village_row(10).is_none(), "a base has none");
}

#[test]
fn a_complete_census_sets_and_lowers_members() {
    let mut w = world();
    assert_eq!(w.village_members(BRINDLE), 20);
    count(&mut w, BRINDLE, 8, 12, true, day(1));
    let r = *w.village_row(BRINDLE).unwrap();
    assert_eq!((r.members, r.k, r.seen, r.seen_r, r.peak), (8, 12, true, 8, 8));
    assert_eq!(w.nodes[BRINDLE].local.pop, 8, "the market's pop follows");
    count(&mut w, BRINDLE, 3, 12, true, day(2));
    let r = *w.village_row(BRINDLE).unwrap();
    assert_eq!((r.members, r.seen_r, r.peak), (3, 3, 8), "a complete census lowers it");
    assert_eq!(w.nodes[BRINDLE].local.pop, 3);
    count(&mut w, BRINDLE, 9, 12, true, day(3));
    assert_eq!(w.village_members(BRINDLE), 9, "and raises it");
    assert_eq!(w.village_row(BRINDLE).unwrap().peak, 9);
    w.check_conserved(w.now()).unwrap();
}

#[test]
fn an_incomplete_census_only_raises() {
    let mut w = world();
    count(&mut w, BRINDLE, 5, 9, false, day(1));
    let r = *w.village_row(BRINDLE).unwrap();
    assert_eq!(r.members, 20, "an incomplete census never lowers it");
    assert_eq!(r.k, 20, "and never moves K");
    assert!(!r.seen, "an incomplete census sees nobody");
    count(&mut w, BRINDLE, 26, 9, false, day(2));
    assert_eq!(w.village_members(BRINDLE), 26, "but it raises it");
    assert_eq!(w.nodes[BRINDLE].local.pop, 26);
}

#[test]
fn a_wiped_village_keeps_its_market_at_pop_one() {
    let mut w = world();
    count(&mut w, BRINDLE, 12, 12, true, day(1));
    count(&mut w, BRINDLE, 0, 12, true, day(2));
    let r = *w.village_row(BRINDLE).unwrap();
    assert_eq!(r.members, 0, "the row holds the true count");
    assert_eq!(w.nodes[BRINDLE].local.pop, 1, "the market keeps its floor of 1");
    let m = w.market(BRINDLE);
    assert!(m.currency.is_some_and(|c| m.line(c).target > 0));
    w.run_until(day(20));
    w.check_conserved(w.now()).unwrap();
}

#[test]
fn a_census_of_zero_on_a_village_never_seen_alive_sets_the_count_and_not_seen() {
    let mut w = world();
    count(&mut w, OAKFORD, 0, 14, true, day(1));
    let r = *w.village_row(OAKFORD).unwrap();
    assert_eq!((r.members, r.seen, r.seen_r), (0, false, 0));
    assert_eq!(r.k, 14);
}

#[test]
fn k_follows_beds_and_is_capped_at_32() {
    let mut w = world();
    count(&mut w, BRINDLE, 8, 50, true, day(1));
    assert_eq!(w.village_row(BRINDLE).unwrap().k, 32, "capped");
    count(&mut w, BRINDLE, 8, 12, true, day(2));
    assert_eq!(w.village_row(BRINDLE).unwrap().k, 12, "and it follows the beds down");
    count(&mut w, BRINDLE, 8, 0, false, day(3));
    assert_eq!(w.village_row(BRINDLE).unwrap().k, 12, "an incomplete count of beds is ignored");
}

#[test]
fn the_census_is_an_input_and_a_refused_one_names_no_village() {
    let mut w = world();
    let mut nodes: BTreeMap<u64, usize> = (0..w.nodes.len()).map(|i| (w.nodes[i].key, i)).collect();
    let key = w.nodes[BRINDLE].key;
    let out = apply(
        &mut w,
        &mut nodes,
        &Input::Census { key, residents: 4, beds: 10, complete: true },
        day(1).0,
    );
    assert!(!out.refused);
    assert_eq!(w.village_members(BRINDLE), 4);
    let out = apply(
        &mut w,
        &mut nodes,
        &Input::Census { key: 99_999, residents: 4, beds: 10, complete: true },
        day(1).0,
    );
    assert!(out.refused, "no such village");
    let hk = w.nodes[HAVEN].key;
    let out = apply(
        &mut w,
        &mut nodes,
        &Input::Census { key: hk, residents: 4, beds: 10, complete: true },
        day(1).0,
    );
    assert!(out.refused, "a haven has no census");
}

#[test]
fn renewal_off_is_todays_world() {
    let (mut w, _) = synth::build(config(false), false);
    assert!(w.villages.is_empty(), "no rows");
    let before = w.nodes[BRINDLE].local.pop;
    let mut nodes: BTreeMap<u64, usize> = (0..w.nodes.len()).map(|i| (w.nodes[i].key, i)).collect();
    let key = w.nodes[BRINDLE].key;
    let out = apply(
        &mut w,
        &mut nodes,
        &Input::Census { key, residents: 2, beds: 10, complete: true },
        day(1).0,
    );
    assert!(out.refused, "the census input is refused with renewal off");
    assert_eq!(w.nodes[BRINDLE].local.pop, before);
    w.run_until(day(30));
    assert!(w.take_commands().iter().all(|c| !matches!(c, Command::SpawnResident(_))));
}

// ---- EQC168 to EQC173: the formats --------------------------------------------------------------

#[test]
fn village_rows_round_trip() {
    let mut w = world();
    count(&mut w, BRINDLE, 8, 12, true, day(1));
    count(&mut w, MIREWATER, 5, 0, true, day(1));
    w.run_until(day(3));
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let back = World::load(config(true), goods, book, &w.save()).expect("reads back");
    assert_eq!(back.villages, w.villages);
    assert_eq!(back.state_hash(), w.state_hash());
    assert_eq!(back.save(), w.save(), "and its next snapshot is the same bytes");
}

#[test]
fn an_older_world_format_is_named_not_read() {
    let w = world();
    let mut bytes = w.save();
    assert_eq!(&bytes[..4], b"RTW5");
    bytes[..4].copy_from_slice(b"RTW4");
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let err = World::load(config(true), goods, book, &bytes).err().expect("RTW4 is refused");
    let msg = err.to_string();
    assert!(msg.contains("format RTW4") && msg.contains("reads RTW5"), "{msg}");
}
