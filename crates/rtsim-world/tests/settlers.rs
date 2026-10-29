// SPDX-License-Identifier: AGPL-3.0-or-later
//! Villages renew, the settlers (EQUILIBRIUM.md §2.4.3, slice S6; docs/overnight-choices-equilibrium.md
//! EQC153 to EQC162): a village found empty after it was seen alive is settled by a pair after the
//! wait, a player's kills lengthen the wait, bookings are answered by the bridge.
//!
//! The synthetic world's nodes: 0 to 5 human villages (pop 20, 18, 12, 15, 14, 10), 6 to 8 Ribbit
//! villages (12, 10, 10), 9 a haven, 10 a player base.

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_trade::build::Genesis;
use rtsim_world::inputs::{Input, Uuid, apply};
use rtsim_world::{Arrive, Command, SpawnResident, World, WorldConfig, synth};
use std::collections::BTreeMap;

const BRINDLE: usize = 0;
const OAKFORD: usize = 1;

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

// ---- EQC153 to EQC162: found empty, kills, the wait, the pair, the bookings ---------------------

type Nodes = BTreeMap<u64, usize>;

/// A player's UUID for the kill reports.
const PLAYER: Uuid = (0x11, 0x22);
/// Highmoor: K 12 at its adoption guess.
const HIGHMOOR: usize = 2;

fn nodes_of(w: &World) -> Nodes {
    (0..w.nodes.len()).map(|i| (w.nodes[i].key, i)).collect()
}

fn census(w: &mut World, nodes: &mut Nodes, i: usize, residents: u32, beds: u32, at: Tick) {
    let key = w.nodes[i].key;
    let out = apply(
        w,
        nodes,
        &Input::Census { key, residents, beds, complete: true },
        at.0,
    );
    assert!(!out.refused, "a census of village {i}");
}

fn kills(w: &mut World, nodes: &mut Nodes, i: usize, kind: u8, n: u32, at: Tick) {
    let key = w.nodes[i].key;
    for _ in 0..n {
        let out = apply(w, nodes, &Input::VillageReport { key, player: PLAYER, kind }, at.0);
        assert!(!out.refused, "a report of kind {kind}");
    }
}

fn result(w: &mut World, nodes: &mut Nodes, op: u64, placed: bool, at: Tick) {
    let out = apply(w, nodes, &Input::ResidentResult { op, placed }, at.0);
    assert!(!out.refused, "the world holds op {op:x}");
}

/// Runs to `to` and takes the resident orders out since the last call.
fn orders(w: &mut World, to: Tick) -> Vec<SpawnResident> {
    w.run_until(to);
    w.take_commands()
        .into_iter()
        .filter_map(|c| match c {
            Command::SpawnResident(s) => Some(s),
            _ => None,
        })
        .collect()
}

/// `d` days and `h` twentyfourths of a day past `from`.
fn after(from: Tick, d: u64, h: u64) -> Tick {
    Tick(from.0 + d * TICKS_PER_DAY + h * TICKS_PER_DAY / 24)
}

/// A village seen alive with `k` beds and all of them wiped, the wiping `kills` of them by a
/// player: at `day(1)`, with the world run there and no orders yet.
fn wiped(w: &mut World, nodes: &mut Nodes, i: usize, beds: u32, killed: u32) {
    census(w, nodes, i, beds, beds, day(1));
    kills(w, nodes, i, 8, killed, day(1));
    census(w, nodes, i, 0, beds, day(1));
    assert!(w.take_commands().iter().all(|c| !matches!(c, Command::SpawnResident(_))));
}

#[test]
fn a_village_never_seen_alive_gets_no_settlers() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    census(&mut w, &mut nodes, OAKFORD, 0, 14, day(1));
    assert_eq!(w.village_row(OAKFORD).unwrap().wait_due, u64::MAX, "no wait runs");
    let got = orders(&mut w, day(400));
    assert!(got.iter().all(|s| s.village != w.nodes[OAKFORD].key), "{got:?}");
    // And one never counted at all (a village the engine adopted and nobody loaded).
    assert!(got.iter().all(|s| s.village != w.nodes[BRINDLE].key));
}

#[test]
fn a_seen_village_emptied_gets_settlers() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    census(&mut w, &mut nodes, BRINDLE, 6, 12, day(1));
    census(&mut w, &mut nodes, BRINDLE, 0, 12, day(2));
    let key = w.nodes[BRINDLE].key;
    assert!(orders(&mut w, after(day(2), 15, 12)).is_empty(), "not before the wait");
    let got = orders(&mut w, after(day(2), 16, 12));
    assert_eq!(got.len(), 2, "a pair: {got:?}");
    assert!(got.iter().all(|s| s.village == key));
    assert!(got.iter().all(|s| matches!(s.arrive, Arrive::Settler { .. })));
    assert_ne!(got[0].op, got[1].op);
    assert_eq!(w.village_members(BRINDLE), 2, "a human village books them abstractly");
    assert_eq!(w.village_row(BRINDLE).unwrap().owed, 2);
}

#[test]
fn a_village_with_fewer_than_two_beds_has_no_settlers() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    census(&mut w, &mut nodes, BRINDLE, 1, 1, day(1));
    census(&mut w, &mut nodes, BRINDLE, 0, 1, day(2));
    assert_eq!(w.village_row(BRINDLE).unwrap().wait_due, u64::MAX);
    assert!(orders(&mut w, day(300)).is_empty());
}

#[test]
fn a_player_kill_adds_one_over_k() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    census(&mut w, &mut nodes, HIGHMOOR, 12, 12, day(1));
    assert_eq!(w.village_pressure(HIGHMOOR), 0.0);
    kills(&mut w, &mut nodes, HIGHMOOR, 8, 1, day(1));
    assert!((w.village_pressure(HIGHMOOR) - 1.0 / 12.0).abs() < 1e-6);
    kills(&mut w, &mut nodes, HIGHMOOR, 8, 11, day(1));
    assert!((w.village_pressure(HIGHMOOR) - 1.0).abs() < 1e-5, "twelve kills at K 12 are 1.00");
}

#[test]
fn other_losses_add_nothing() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    census(&mut w, &mut nodes, HIGHMOOR, 12, 12, day(1));
    for kind in [1, 2, 3, 4, 5, 6, 7, 9] {
        kills(&mut w, &mut nodes, HIGHMOOR, kind, 3, day(1));
    }
    assert_eq!(w.village_pressure(HIGHMOOR), 0.0, "only a resident killed with credit counts");
    // A census that finds fewer is a loss with no cause: no pressure either (EQC156).
    census(&mut w, &mut nodes, HIGHMOOR, 3, 12, day(2));
    assert_eq!(w.village_pressure(HIGHMOOR), 0.0);
}

#[test]
fn kill_reports_do_not_move_members() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    census(&mut w, &mut nodes, HIGHMOOR, 12, 12, day(1));
    kills(&mut w, &mut nodes, HIGHMOOR, 8, 12, day(1));
    assert_eq!(w.village_members(HIGHMOOR), 12, "only a census moves the count");
    assert_eq!(w.stats.kills, 12);
}

#[test]
fn settlers_come_after_the_wait() {
    // A full player wipe at K 12: P is 1.00, the wait 32 days.
    let mut w = world();
    let mut nodes = nodes_of(&w);
    wiped(&mut w, &mut nodes, HIGHMOOR, 12, 12);
    assert!(orders(&mut w, after(day(1), 31, 12)).is_empty(), "not at day 31.5");
    assert_eq!(orders(&mut w, after(day(1), 32, 12)).len(), 2, "a pair by day 32.5");

    // Emptied by a siege (no player kill): P is 0, the wait 16 days.
    let mut w = world();
    let mut nodes = nodes_of(&w);
    wiped(&mut w, &mut nodes, HIGHMOOR, 12, 0);
    assert!(orders(&mut w, after(day(1), 15, 12)).is_empty(), "not at day 15.5");
    assert_eq!(orders(&mut w, after(day(1), 16, 12)).len(), 2, "a pair by day 16.5");

    // A census of 2 inside the wait (vanilla bred, a zombie was cured) cancels it.
    let mut w = world();
    let mut nodes = nodes_of(&w);
    wiped(&mut w, &mut nodes, HIGHMOOR, 12, 12);
    census(&mut w, &mut nodes, HIGHMOOR, 2, 12, day(10));
    assert_eq!(w.village_row(HIGHMOOR).unwrap().wait_due, u64::MAX);
    assert!(orders(&mut w, day(80)).is_empty(), "nobody comes to a village that has two");
}

#[test]
fn a_running_wait_is_not_pushed_out() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    wiped(&mut w, &mut nodes, HIGHMOOR, 12, 0);
    // More kills of the dead while the wait runs (a late report) change nothing.
    kills(&mut w, &mut nodes, HIGHMOOR, 8, 12, day(5));
    assert!(w.village_pressure(HIGHMOOR) > 0.9);
    assert_eq!(orders(&mut w, after(day(1), 16, 12)).len(), 2, "still the 16 days it started with");
}

#[test]
fn a_pair_is_clamped_to_k() {
    // One survivor at K 2: one settler fills the second bed.
    let mut w = world();
    let mut nodes = nodes_of(&w);
    census(&mut w, &mut nodes, HIGHMOOR, 2, 2, day(1));
    census(&mut w, &mut nodes, HIGHMOOR, 1, 2, day(2));
    assert_eq!(orders(&mut w, day(40)).len(), 1);
    assert_eq!(w.village_members(HIGHMOOR), 2, "never above the beds");
    // One survivor at K 12 gets the pair.
    let mut w = world();
    let mut nodes = nodes_of(&w);
    census(&mut w, &mut nodes, HIGHMOOR, 2, 12, day(1));
    census(&mut w, &mut nodes, HIGHMOOR, 1, 12, day(2));
    assert_eq!(orders(&mut w, day(40)).len(), 2);
    assert_eq!(w.village_members(HIGHMOOR), 3);
}

#[test]
fn the_bearing_points_away_from_the_neighbours() {
    // Highmoor (1200, -300) is the eastern edge of the synthetic world: the rest lie west.
    let mut w = world();
    let mut nodes = nodes_of(&w);
    wiped(&mut w, &mut nodes, HIGHMOOR, 12, 0);
    let got = orders(&mut w, day(40));
    let Arrive::Settler { x, z } = got[0].arrive else { panic!("{:?}", got[0]) };
    let me = w.nodes[HIGHMOOR].pos;
    let (dx, dz) = (f64::from(x - me.x), f64::from(z - me.z));
    assert!((dx.hypot(dz) - 192.0).abs() < 2.0, "192 blocks out: {dx} {dz}");
    assert!(dx > 150.0, "east, away from the others: {dx} {dz}");
    assert_eq!(got[0].arrive, got[1].arrive, "one family, one source");
}

#[test]
fn a_failed_arrival_lapses_the_booking() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    wiped(&mut w, &mut nodes, HIGHMOOR, 12, 0);
    let got = orders(&mut w, day(20));
    assert_eq!(got.len(), 2);
    result(&mut w, &mut nodes, got[0].op, false, day(20));
    let r = *w.village_row(HIGHMOOR).unwrap();
    assert_eq!((r.members, r.owed), (1, 1), "one booking lapsed");
    assert_eq!(w.stats.lapsed, 1);
    result(&mut w, &mut nodes, got[1].op, false, day(20));
    assert_eq!(w.village_members(HIGHMOOR), 0);
    assert!(w.village_row(HIGHMOOR).unwrap().wait_due != u64::MAX, "short again: a new wait runs");
    let again = orders(&mut w, day(60));
    assert_eq!(again.len(), 2, "and a new pair is ordered");
    assert!(again.iter().all(|a| got.iter().all(|g| g.op != a.op)), "under new op ids");
    // An answer for an op the world never held is not an answer.
    let out = apply(&mut w, &mut nodes, &Input::ResidentResult { op: 1, placed: true }, day(60).0);
    assert!(out.refused);
}

#[test]
fn a_census_never_undoes_a_booking_java_has_not_answered() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    wiped(&mut w, &mut nodes, HIGHMOOR, 12, 0);
    let got = orders(&mut w, day(20));
    assert_eq!(w.village_members(HIGHMOOR), 2);
    // Java counts nobody yet: the booked pair stands.
    census(&mut w, &mut nodes, HIGHMOOR, 0, 12, day(20));
    assert_eq!(w.village_members(HIGHMOOR), 2, "residents 0 plus 2 owed");
    // Java placed one; its census counts one, the other is still owed.
    result(&mut w, &mut nodes, got[0].op, true, day(21));
    census(&mut w, &mut nodes, HIGHMOOR, 1, 12, day(21));
    assert_eq!(w.village_members(HIGHMOOR), 2);
    result(&mut w, &mut nodes, got[1].op, true, day(21));
    census(&mut w, &mut nodes, HIGHMOOR, 2, 12, day(22));
    let r = *w.village_row(HIGHMOOR).unwrap();
    assert_eq!((r.members, r.owed, r.owed_at), (2, 0, u64::MAX));
}

#[test]
fn owed_expires_after_two_years() {
    let mut w = world();
    let mut nodes = nodes_of(&w);
    wiped(&mut w, &mut nodes, HIGHMOOR, 12, 0);
    orders(&mut w, day(20));
    assert_eq!(w.village_row(HIGHMOOR).unwrap().owed, 2);
    // Inside the 192 days a census of 0 still honours them.
    census(&mut w, &mut nodes, HIGHMOOR, 0, 12, day(150));
    assert_eq!(w.village_members(HIGHMOOR), 2);
    // Past them it releases them: a lost order must not hold the count for ever.
    census(&mut w, &mut nodes, HIGHMOOR, 0, 12, day(300));
    let r = *w.village_row(HIGHMOOR).unwrap();
    assert_eq!((r.members, r.owed), (0, 0));
    assert_eq!(w.stats.lapsed, 2);
    assert!(r.wait_due != u64::MAX, "and the village is short again");
}

#[test]
fn a_replayed_engine_gives_the_same_ops() {
    let run = |save_at: Option<u64>| {
        let mut w = world();
        let mut nodes = nodes_of(&w);
        wiped(&mut w, &mut nodes, HIGHMOOR, 12, 12);
        let mut got = Vec::new();
        if let Some(d) = save_at {
            got.extend(orders(&mut w, day(d)));
            let (goods, ids) = synth::goods();
            let book = synth::recipes(&ids);
            w = World::load(config(true), goods, book, &w.save()).expect("reads back");
        }
        got.extend(orders(&mut w, day(60)));
        (got, w.state_hash())
    };
    let (a, ha) = run(None);
    let (b, hb) = run(None);
    assert_eq!(a, b, "the same inputs give the same orders");
    assert_eq!(ha, hb);
    assert_eq!(a.len(), 2);
    // A restart in the middle of the wait changes nothing, and one after the order keeps the
    // booking so its answer is still known.
    let (c, hc) = run(Some(10));
    assert_eq!(a, c);
    assert_eq!(ha, hc);
    let mut w = world();
    let mut nodes = nodes_of(&w);
    wiped(&mut w, &mut nodes, HIGHMOOR, 12, 12);
    let got = orders(&mut w, day(40));
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut back = World::load(config(true), goods, book, &w.save()).expect("reads back");
    assert!(back.wants_resident_result(got[0].op, true), "the booking survived the restart");
    let mut nodes = nodes_of(&back);
    result(&mut back, &mut nodes, got[0].op, true, day(41));
}

#[test]
fn farmed_village_yields_closed_form() {
    // A player kills every pair within 2 days of its arrival, for 60 years (§3.3): the village
    // gets 3.1 pairs a year at K 12, a pair counted as two kills (the rule read at K/2).
    let mut w = world();
    let mut nodes = nodes_of(&w);
    census(&mut w, &mut nodes, HIGHMOOR, 12, 12, day(1));
    kills(&mut w, &mut nodes, HIGHMOOR, 8, 12, day(1));
    census(&mut w, &mut nodes, HIGHMOOR, 0, 12, day(1));
    let end = day(60 * 365);
    let mut pairs = 0u32;
    let mut first_after = 0u64;
    loop {
        let due = w.village_row(HIGHMOOR).unwrap().wait_due;
        if due == u64::MAX || due >= end.0 {
            break;
        }
        let got = orders(&mut w, Tick(due));
        assert_eq!(got.len(), 2);
        pairs += 1;
        if pairs == 10 {
            first_after = due;
        }
        for g in &got {
            result(&mut w, &mut nodes, g.op, true, Tick(due));
        }
        census(&mut w, &mut nodes, HIGHMOOR, 2, 12, Tick(due));
        let at = Tick(due + 2 * TICKS_PER_DAY);
        kills(&mut w, &mut nodes, HIGHMOOR, 8, 2, at);
        census(&mut w, &mut nodes, HIGHMOOR, 0, 12, at);
    }
    let (_, days_per_pair) = w.cfg.village_rule.farmed(6, 2.0);
    let years = (end.0 - first_after) as f64 / TICKS_PER_DAY as f64 / 365.0;
    let per_year = f64::from(pairs - 10) / years;
    let closed = 365.0 / f64::from(days_per_pair);
    assert!(
        (per_year - closed).abs() / closed < 0.05,
        "{per_year:.3} pairs a year against the closed form {closed:.3}"
    );
}
