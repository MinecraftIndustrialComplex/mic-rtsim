// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine half of MOWZIE.md's slice M7 (§4.3, ledgers 1 and 17; MZB-300 to MZB-349): trains
//! through a Frostmaw's range are counted as a decayed number of legs per (faction, actor), pushed
//! as one pinned source, and a raid on it goes to the track, not a district. The meter's
//! arithmetic first, then the gate and its wakes, the owners of held passes, the source and the
//! focus, the target, and the page round trip.

use rtsim_conflict::engine::{LineCount, RailMeter};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;

const TAU: f64 = 48_000.0;

fn meter() -> RailMeter {
    RailMeter::new(ActorId::from_parts(7, 1))
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn a_late_pass_counts_as_if_on_time() {
    // The same four exits, in order and with the third and fourth late (reported after the
    // fifth), count the same: adds are exact whatever order they arrive in.
    let exits = [1_000, 9_000, 15_000, 26_000, 40_000];
    let mut in_order = meter();
    for t in exits {
        in_order.add(Tick(t));
    }
    let mut late = meter();
    for t in [1_000, 9_000, 40_000, 15_000, 26_000] {
        late.add(Tick(t));
    }
    let t = Tick(60_000);
    assert!(
        near(in_order.n_at(t), late.n_at(t), 1e-12),
        "{} against {}",
        in_order.n_at(t),
        late.n_at(t)
    );
    // The reference moved only for the on-time ones.
    assert_eq!(in_order.t_ref, Tick(40_000));
    assert_eq!(late.t_ref, Tick(40_000));
    // And a single late pass adds exactly what it would have decayed to.
    let mut m = meter();
    m.add(Tick(10_000));
    let before = m.n;
    m.add(Tick(4_000));
    assert!(near(m.n - before, (-6_000.0 / TAU).exp(), 1e-12));
    assert_eq!(m.t_ref, Tick(10_000), "a late pass leaves the reference");
}

#[test]
fn the_meter_settles_at_n_over_tau() {
    // A leg every `h` ticks settles at n = 1 / (1 - e^(-h/tau)), read just after a leg, and the
    // rate there is that times 24,000 / tau legs a day: close to 24,000 / h for a short headway.
    for h in [600u64, 1_200, 2_400, 6_000] {
        let mut m = meter();
        // Twenty decay times of legs: the start-up's e^-20 is gone.
        let legs = (20.0 * TAU / h as f64) as u64;
        for i in 1..=legs {
            m.add(Tick(i * h));
        }
        let end = Tick(legs * h);
        let settled = 1.0 / (1.0 - (-(h as f64) / TAU).exp());
        assert!(
            near(m.n_at(end), settled, 1e-6),
            "h {h}: {} against {settled}",
            m.n_at(end)
        );
        let per_day = m.per_day(end);
        assert!(
            near(per_day, settled * TICKS_PER_DAY as f64 / TAU, 1e-6),
            "h {h}: {per_day}"
        );
        // Just after a leg the rate runs a half-headway above the mean (n sawtooths from
        // settled - 1 to settled): within 1.3% of the mean legs a day at these headways.
        let mean = TICKS_PER_DAY as f64 / h as f64;
        assert!(
            (per_day - mean).abs() <= mean * (h as f64 / TAU),
            "h {h}: {per_day} against {mean}"
        );
    }
}

#[test]
fn falls_to_is_the_closed_form() {
    let mut m = meter();
    for i in 1..=100 {
        m.add(Tick(i * 1_200));
    }
    let r = m.per_day(m.t_ref);
    // To half the rate: tau ln 2 later, rounded up.
    let half = m.falls_to(r / 2.0);
    let want = m.t_ref.0 + (TAU * 2f64.ln()).ceil() as u64;
    assert_eq!(half, Tick(want));
    // The rate there is at or below the target, one tick before above it.
    assert!(m.per_day(half) <= r / 2.0 * (1.0 + 1e-12));
    assert!(m.per_day(Tick(half.0 - 1)) > r / 2.0 * (1.0 - 1e-12));
    // Already at or below the target: now. A target of nothing is never reached.
    assert_eq!(m.falls_to(r * 2.0), m.t_ref);
    assert!(m.falls_to(0.0).0 > 1 << 60);
    // Retirement: 0.01 legs a day from a rate of 24 is tau ln(2400) ticks on.
    let retire = m.falls_to(0.01);
    let ticks = (retire.0 - m.t_ref.0) as f64;
    let want = TAU * (r / 0.01).ln();
    assert!(near(ticks, want, 1.0), "{ticks} against {want}");
}

#[test]
fn a_line_decays_and_adds_as_a_meter_does() {
    let mut l = LineCount {
        a: 1,
        b: 2,
        n: 1.0,
        t_ref: Tick(1_000),
        point: Pos::new(0, 10, 20),
        y: 64,
        near: None,
    };
    assert!(near(l.n_at(Tick(1_000 + 48_000)), (-1f64).exp(), 1e-12));
    l.add(Tick(1_000 + 48_000));
    assert!(near(l.n, 1.0 + (-1f64).exp(), 1e-12));
    assert_eq!(l.t_ref, Tick(49_000));
    assert!(near(l.per_day(l.t_ref), l.n * 24_000.0 / TAU, 1e-12));
}

fn at(x: i32) -> Pos {
    Pos::new(0, x, 0)
}

#[test]
fn a_line_credit_moves_the_point() {
    let mut m = meter();
    // Either order of the two stations is the same line.
    m.credit_line((9, 4), Tick(1_000), at(100), 64, None);
    m.credit_line((4, 9), Tick(2_000), at(160), 66, None);
    assert_eq!(m.lines.len(), 1);
    let l = m.lines[0];
    assert_eq!((l.a, l.b), (4, 9));
    assert_eq!((l.point, l.y), (at(160), 66), "the latest pass's point");
    assert!(near(l.n, 1.0 + (-1_000.0 / TAU).exp(), 1e-12));
    // A pass reported late counts but leaves the point where the later pass put it (MZB-334).
    m.credit_line((4, 9), Tick(1_500), at(300), 70, None);
    let l = m.lines[0];
    assert_eq!((l.point, l.y), (at(160), 66));
    assert_eq!(l.t_ref, Tick(2_000));
    // A reroute is a second line, and the busiest is the one with more legs.
    m.credit_line((4, 12), Tick(3_000), at(40), 64, None);
    assert_eq!(m.lines.len(), 2);
    assert_eq!(m.busiest(Tick(3_000)).map(|l| (l.a, l.b)), Some((4, 9)));
    m.credit_line((4, 12), Tick(3_100), at(40), 64, None);
    m.credit_line((4, 12), Tick(3_200), at(41), 64, None);
    assert_eq!(m.busiest(Tick(3_200)).map(|l| (l.a, l.b)), Some((4, 12)));
    // A tie goes to the lower pair.
    let mut t = meter();
    t.credit_line((5, 6), Tick(100), at(1), 0, None);
    t.credit_line((1, 2), Tick(100), at(2), 0, None);
    assert_eq!(t.busiest(Tick(100)).map(|l| (l.a, l.b)), Some((1, 2)));
    assert!(meter().busiest(Tick(0)).is_none());
}

#[test]
fn the_seventeenth_line_evicts_the_smallest() {
    let mut m = meter();
    // Sixteen lines, line k credited k + 1 times at the same tick.
    for k in 0..16u64 {
        for _ in 0..=k {
            m.credit_line((k, 100 + k), Tick(5_000), at(k as i32), 0, None);
        }
    }
    assert_eq!(m.lines.len(), 16);
    assert!(
        m.lines
            .windows(2)
            .all(|w| (w[0].a, w[0].b) < (w[1].a, w[1].b))
    );
    // A seventeenth line makes the smallest (line 0, one leg) go.
    m.credit_line((50, 150), Tick(5_000), at(50), 0, None);
    assert_eq!(m.lines.len(), 16);
    assert!(m.lines.iter().all(|l| l.a != 0), "the smallest is gone");
    assert!(m.lines.iter().any(|l| l.a == 50));
    assert!(
        m.lines
            .windows(2)
            .all(|w| (w[0].a, w[0].b) < (w[1].a, w[1].b))
    );
    // The next one evicts the smallest existing line again: line 50, with its one leg (the new
    // line itself is never the one that goes).
    m.credit_line((60, 160), Tick(5_000), at(60), 0, None);
    assert!(m.lines.iter().all(|l| l.a != 50), "the one-leg line goes");
    assert!(m.lines.iter().any(|l| l.a == 60));
    assert_eq!(m.lines.len(), 16);
}

// ---- the source and the focus (MZB-313, MZB-314) ---------------------------------------------

use rtsim_conflict::SignalKind;
use rtsim_conflict::engine::rail::rail_key;
use rtsim_conflict::engine::{Conflict, Pin, Signal};
use rtsim_conflict::synth::{self, CAMP, CAMP_POS, PLAYER, REFINERY};
use rtsim_industry::ids::DistrictId;
use rtsim_world::World;

const DEN_B: u64 = 0xC0B1_0002;
const DEN_C: u64 = 0xC0B1_0003;

/// A synthetic world with a Frostmaw den at `CAMP_POS` and the player's actor.
fn dens(seed: u64) -> (World, Conflict, u32, ActorId) {
    let (mut w, _) = synth::world(seed);
    let mut c = synth::conflict(seed, &w);
    let f = c
        .add_faction(&mut w, CAMP, "frostmaw", CAMP_POS, Tick::ZERO)
        .unwrap();
    let player = c.player(&mut w, PLAYER);
    (w, c, f, player)
}

fn track(y: i32) -> Pin {
    Pin { y, station: None }
}

fn rail_source(c: &Conflict, f: u32, a: ActorId) -> Option<rtsim_conflict::engine::Source> {
    let key = (rail_key(c.factions[f as usize].key, a), SignalKind::Rail);
    c.sources.get(&key).cloned()
}

#[test]
fn set_emission_for_reaches_only_its_faction() {
    let (mut w, mut c, f, player) = dens(11);
    // A second den whose territory overlaps the first's: a pollution source here would reach both.
    let b = c
        .add_faction(
            &mut w,
            DEN_B,
            "frostmaw",
            Pos::new(0, CAMP_POS.x + 100, CAMP_POS.z),
            Tick::ZERO,
        )
        .unwrap();
    let spot = Pos::new(0, CAMP_POS.x + 50, CAMP_POS.z + 30);
    // A control: an ordinary source in the same chunk, to see `reattribute` run at all.
    let ctl = (0xC7_u64, SignalKind::Building);
    c.set_emission(&mut w, ctl.0, ctl.1, spot, None, 3.0, Tick(10));
    c.set_emission_for(&mut w, f, player, 10.0, spot, track(70), Tick(100));

    let s = rail_source(&c, f, player).expect("the source");
    assert_eq!(s.affects, vec![(f, 1.0)]);
    assert_eq!((s.pos, s.district, s.actor), (spot, None, player));
    assert!(s.chunks.is_empty());
    assert_eq!(s.pin.as_deref().copied(), Some(track(70)));
    assert!((s.rate - 10.0 / 24_000.0).abs() < 1e-15);
    assert_eq!(c.factions.row(f).reach_ids(SignalKind::Rail).len(), 1);
    assert!(c.factions.row(b).reach_ids(SignalKind::Rail).is_empty());
    assert!(c.feud(b, player).is_none(), "the other den hears nothing");
    let fe = c.feud(f, player).expect("the feud");
    assert!((fe.raw[SignalKind::Rail.index()] - 10.0 / 24_000.0).abs() < 1e-15);
    let fo = fe.focus.expect("a focus");
    assert_eq!(
        (fo.pos, fo.district, fo.track),
        (spot, None, Some(track(70)))
    );

    // Another player takes the chunk: the ordinary source follows, the rail source never does.
    let other = (0x0B0B_u64, 0x1234_u64);
    c.observe_presence(&mut w, other, spot, Tick(200));
    c.observe_presence(&mut w, other, spot, Tick(9_000));
    assert!(c.stats.reattributed >= 1, "the control moved");
    assert_eq!(rail_source(&c, f, player).unwrap().actor, player);

    // A den founded later, with the source in its reach, does not pick it up.
    let d = c
        .add_faction(
            &mut w,
            DEN_C,
            "frostmaw",
            Pos::new(0, CAMP_POS.x + 60, CAMP_POS.z + 60),
            Tick(300),
        )
        .unwrap();
    assert_eq!(rail_source(&c, f, player).unwrap().affects, vec![(f, 1.0)]);
    assert!(c.factions.row(d).reach_ids(SignalKind::Rail).is_empty());
    assert!(c.feud(d, player).is_none());

    // A rate of 0 removes the source and its reach; the feud's rate goes with it and its focus
    // stays where it was.
    c.set_emission_for(&mut w, f, player, 0.0, spot, track(70), Tick(400));
    assert!(rail_source(&c, f, player).is_none());
    assert!(c.factions.row(f).reach_ids(SignalKind::Rail).is_empty());
    let fe = c.feud(f, player).unwrap();
    assert_eq!(fe.raw[SignalKind::Rail.index()], 0.0);
    assert_eq!(fe.focus.map(|f| f.pos), Some(spot));
    // A new push at another point replaces the source under the same key.
    let elsewhere = Pos::new(0, CAMP_POS.x - 80, CAMP_POS.z);
    c.set_emission_for(&mut w, f, player, 4.0, elsewhere, track(65), Tick(500));
    c.set_emission_for(&mut w, f, player, 6.0, elsewhere, track(65), Tick(600));
    assert_eq!(c.factions.row(f).reach_ids(SignalKind::Rail).len(), 1);
    assert!((rail_source(&c, f, player).unwrap().rate - 6.0 / 24_000.0).abs() < 1e-15);
}

fn kill_at(
    w: &mut World,
    c: &mut Conflict,
    a: ActorId,
    pos: Pos,
    district: Option<DistrictId>,
    at: u64,
) {
    kill_of(w, c, a, pos, district, 0.5, at);
}

fn kill_of(
    w: &mut World,
    c: &mut Conflict,
    a: ActorId,
    pos: Pos,
    district: Option<DistrictId>,
    amount: f64,
    at: u64,
) {
    c.signal(
        w,
        Signal {
            faction: Some(CAMP),
            kind: SignalKind::Killing,
            amount,
            actor: a,
            pos,
            district,
        },
        Tick(at),
    );
}

#[test]
fn a_district_focus_is_not_displaced_by_a_track_nor_a_track_by_a_bare_pos() {
    let (mut w, mut c, f, player) = dens(12);
    let site = Pos::new(0, CAMP_POS.x + 20, CAMP_POS.z + 20);
    let rail = Pos::new(0, CAMP_POS.x + 100, CAMP_POS.z);
    let wild = Pos::new(0, CAMP_POS.x - 90, CAMP_POS.z + 40);
    let focus = |c: &Conflict| c.feud(f, player).and_then(|x| x.focus);

    // A district focus (a kill with a district) stays when a track source is pushed.
    kill_at(&mut w, &mut c, player, site, Some(REFINERY), 10);
    assert_eq!(focus(&c).unwrap().district, Some(REFINERY));
    c.set_emission_for(&mut w, f, player, 8.0, rail, track(64), Tick(20));
    let fo = focus(&c).unwrap();
    assert_eq!(
        (fo.pos, fo.district, fo.track),
        (site, Some(REFINERY), None)
    );

    // A track takes the place of a bare position, and a bare position does not take a track's.
    let (mut w, mut c, f2, player) = dens(13);
    assert_eq!(f2, f);
    kill_at(&mut w, &mut c, player, site, None, 10);
    assert_eq!(focus(&c).unwrap().track, None);
    c.set_emission_for(&mut w, f, player, 8.0, rail, track(64), Tick(20));
    let fo = focus(&c).unwrap();
    assert_eq!((fo.pos, fo.track), (rail, Some(track(64))));
    // A kill with no district (the signal path) and an ordinary source with none (refresh_rates)
    // leave the track where it is.
    kill_at(&mut w, &mut c, player, wild, None, 30);
    assert_eq!(focus(&c).unwrap().pos, rail);
    c.observe_presence(&mut w, PLAYER, wild, Tick(33));
    c.observe_presence(&mut w, PLAYER, wild, Tick(35));
    c.set_emission(
        &mut w,
        0xB1D,
        SignalKind::Building,
        wild,
        None,
        5.0,
        Tick(40),
    );
    assert!(c.feud(f, player).unwrap().raw[SignalKind::Building.index()] > 0.0);
    assert_eq!(
        focus(&c).unwrap().pos,
        rail,
        "a bare source is a lower class"
    );
    // A district does take a track's place, and a newer track replaces an older one.
    kill_at(&mut w, &mut c, player, site, Some(REFINERY), 50);
    let fo = focus(&c).unwrap();
    assert_eq!((fo.pos, fo.district), (site, Some(REFINERY)));
    let (mut w, mut c, f3, player) = dens(14);
    assert_eq!(f3, f);
    c.set_emission_for(&mut w, f, player, 8.0, rail, track(64), Tick(20));
    c.set_emission_for(&mut w, f, player, 8.0, wild, track(66), Tick(60));
    let fo = focus(&c).unwrap();
    assert_eq!((fo.pos, fo.track), (wild, Some(track(66))));
}

#[test]
fn a_zero_weight_kind_sets_no_focus() {
    // The Frostmaw has no weight for smog: a district's smog adds no grievance and no focus. The
    // Umvuthana, whose weight for it is above 0, take the district for their focus.
    let at_district = |culture: &str| {
        let (mut w, _) = synth::world(21);
        let mut c = synth::conflict(21, &w);
        let f = c
            .add_faction(&mut w, CAMP, culture, CAMP_POS, Tick::ZERO)
            .unwrap();
        let a = c.player(&mut w, PLAYER);
        let site = Pos::new(0, CAMP_POS.x + 30, CAMP_POS.z);
        c.observe_presence(&mut w, PLAYER, site, Tick(3));
        c.observe_presence(&mut w, PLAYER, site, Tick(5));
        c.set_emission(
            &mut w,
            0x5309,
            SignalKind::Smog,
            site,
            Some(REFINERY),
            12.0,
            Tick(10),
        );
        c.feud(f, a).and_then(|x| x.focus)
    };
    assert_eq!(at_district("frostmaw"), None);
    assert_eq!(
        at_district("umvuthana").and_then(|f| f.district),
        Some(REFINERY)
    );
}

// ---- the target (MZB-315 to MZB-317) ----------------------------------------------------------

use rtsim_conflict::Step;
use rtsim_conflict::engine::{Party, Phase, Target};
use rtsim_ecology::infra::{DistrictSite, Infrastructure, RaidOutcome, TrainSighting};
use rtsim_industry::ids::{Good, TrainId};
use rtsim_industry::threat::Threat;
use rtsim_kernel::handle::Handle;
use rtsim_space::topology::{Topologies, Topology};

/// An infrastructure that counts what the engine asks of it and has nothing in it, except a train
/// crossing every walk (so an ambush roll, if one is made, finds one).
#[derive(Default)]
struct Probe {
    districts_near: u32,
    trains_crossed: u32,
    raids: u32,
}

impl Infrastructure for Probe {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        self.districts_near += 1;
        Vec::new()
    }

    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        self.raids += 1;
        None
    }

    fn good_name(&self, _: Good) -> Option<String> {
        None
    }

    fn trains_crossed(
        &mut self,
        _: u16,
        _: &[(i32, i32)],
        from: Tick,
        to: Tick,
    ) -> Vec<TrainSighting> {
        self.trains_crossed += 1;
        vec![TrainSighting {
            train: TrainId(9),
            from,
            to,
        }]
    }
}

/// Runs the engine hour by hour to day `days`, noting each party the first time it is seen and
/// the phases the first Raid and Invasion parties pass through.
struct Watch {
    seen: Vec<(Handle<Party>, Step, Target)>,
    phases: Vec<(Step, Phase)>,
}

fn watch(c: &mut Conflict, w: &mut World, probe: &mut Probe, from: Tick, days: u64) -> Watch {
    let mut out = Watch {
        seen: Vec::new(),
        phases: Vec::new(),
    };
    let mut t = from;
    while t < Tick::from_days(days) {
        t = Tick(t.0 + 1_000);
        c.run_until(w, probe, t);
        for (h, p) in c.parties.iter() {
            let step = if p.kind == rtsim_conflict::culture::PartyKind::Raid {
                Step::Raid
            } else {
                Step::Invasion
            };
            if !out.seen.iter().any(|s| s.0 == h) {
                out.seen.push((h, step, p.target));
            }
            if out.phases.last() != Some(&(step, p.phase)) {
                out.phases.push((step, p.phase));
            }
        }
    }
    out
}

#[test]
fn a_raid_on_a_track_focus_targets_the_track_and_an_invasion_the_station() {
    let rail = Pos::new(0, CAMP_POS.x + 100, CAMP_POS.z);
    // A track that names no station: both rungs go to the track, five blocks toward the den.
    let (mut w, mut c, f, player) = dens(31);
    c.set_emission_for(&mut w, f, player, 30.0, rail, track(66), Tick(10));
    let seen = watch(&mut c, &mut w, &mut Probe::default(), Tick(10), 40).seen;
    let spot = Pos::new(0, rail.x - 5, rail.z);
    let want = Target::Track {
        track: rail,
        spot,
        y: 66,
    };
    let raid = seen.iter().find(|s| s.1 == Step::Raid).expect("a raid");
    assert_eq!(raid.2, want);
    let inv = seen
        .iter()
        .find(|s| s.1 == Step::Invasion)
        .expect("an invasion");
    assert_eq!(inv.2, want, "no station: the invasion goes to the track");

    // A track that names a station with a district: the invasion marches on the station.
    let (mut w, mut c, f, player) = dens(32);
    let station = Pos::new(0, rail.x + 40, rail.z + 10);
    let pin = Pin {
        y: 66,
        station: Some((station, REFINERY)),
    };
    c.set_emission_for(&mut w, f, player, 30.0, rail, pin, Tick(10));
    let seen = watch(&mut c, &mut w, &mut Probe::default(), Tick(10), 40).seen;
    let raid = seen.iter().find(|s| s.1 == Step::Raid).expect("a raid");
    assert_eq!(raid.2, want, "a raid still goes to the track");
    let inv = seen
        .iter()
        .find(|s| s.1 == Step::Invasion)
        .expect("an invasion");
    assert_eq!(inv.2, Target::District(REFINERY, station));
}

#[test]
fn the_spot_is_five_blocks_toward_the_den() {
    let (_w, c, _f, _) = dens(33);
    let at = |x: i32, z: i32| Pos::new(0, x, z);
    // Toward the den along each axis, and on a 3-4-5 diagonal.
    assert_eq!(c.spot_toward(at(100, 0), at(0, 0)), at(95, 0));
    assert_eq!(c.spot_toward(at(0, 100), at(0, 0)), at(0, 95));
    assert_eq!(c.spot_toward(at(-60, -80), at(0, 0)), at(-57, -76));
    // The den on top of the point, or a block from it, or in another frame: +x.
    assert_eq!(c.spot_toward(at(40, 40), at(40, 40)), at(45, 40));
    assert_eq!(c.spot_toward(at(40, 40), at(40, 41)), at(45, 40));
    assert_eq!(c.spot_toward(at(40, 40), Pos::new(1, 0, 0)), at(45, 40));
    // The result is whole blocks: a direction that is not a multiple rounds.
    assert_eq!(c.spot_toward(at(10, 10), at(0, 0)), at(6, 6));

    // Across a seam: x wraps over [0, 2000), so the den at 10 is 20 blocks east of a track at
    // 1990, the short way, and one at 1998 is 4 blocks west of a track at 2.
    let (_w, mut c, _, _) = dens(34);
    let mut topo = Topologies::new();
    topo.set(0, Topology::x_wrap(0, 2000));
    c.set_topologies(topo.shared());
    assert_eq!(c.spot_toward(at(1990, 0), at(10, 0)), at(1995, 0));
    assert_eq!(c.spot_toward(at(1990, 0), at(10, 30)), at(1993, 4));
    assert_eq!(
        c.spot_toward(at(2, 0), at(1998, 0)),
        at(1997, 0),
        "2 - 5 = -3, which is 1997 canonical"
    );
}

#[test]
fn a_raid_at_a_track_waits_and_goes_home_raiding_nothing() {
    let rail = Pos::new(0, CAMP_POS.x + 100, CAMP_POS.z);
    let (mut w, mut c, f, player) = dens(35);
    // One raid only: the climb stops short of Invasion.
    c.set_emission_for(&mut w, f, player, 14.0, rail, track(66), Tick(10));
    let mut probe = Probe::default();
    let run = watch(&mut c, &mut w, &mut probe, Tick(10), 12);
    let raid = run.seen.iter().find(|s| s.1 == Step::Raid).expect("a raid");
    let Target::Track { spot, .. } = raid.2 else {
        panic!("{:?}", raid.2);
    };
    // It musters, walks out, waits at the spot and goes home.
    let phases: Vec<Phase> = run.phases.iter().map(|p| p.1).collect();
    assert!(
        phases.starts_with(&[Phase::Mustering, Phase::Out, Phase::Waiting, Phase::Home]),
        "{phases:?}"
    );
    assert!(
        c.parties
            .iter()
            .all(|(_, p)| p.kind != rtsim_conflict::culture::PartyKind::Raid)
    );
    assert_eq!(c.stats.bored, 1, "it waited out its patience");
    // Nothing was raided, looked for, ambushed or asked of the industry.
    assert_eq!(
        (probe.districts_near, probe.trains_crossed, probe.raids),
        (0, 0, 0)
    );
    assert_eq!(
        (
            c.stats.raids_district,
            c.stats.raids_village,
            c.stats.ambushes
        ),
        (0, 0, 0)
    );
    assert!(c.ledger.from_players.is_empty() && c.ledger.from_villages.is_empty());
    // It waited at the spot, five blocks from the track point on the den's side.
    assert_eq!(spot, Pos::new(0, rail.x - 5, rail.z));
    assert!(c.feud(f, player).unwrap().raids >= 1);

    // The control: the same den raided by kills at a bare position walks to that place, where
    // the engine looks for a district and rolls for a train on the way.
    let (mut w, mut c, _, player) = dens(36);
    let mut probe = Probe::default();
    for day in 0..14u64 {
        let t = Tick::from_days(day) + 10;
        kill_of(&mut w, &mut c, player, rail, None, 2.0, t.0);
        c.run_until(&mut w, &mut probe, Tick::from_days(day + 1));
    }
    assert!(c.stats.parties[rtsim_conflict::culture::PartyKind::Raid.index()] >= 1);
    assert!(
        probe.trains_crossed > 0,
        "a walk to a place rolls for trains"
    );
    assert!(
        probe.districts_near > 0,
        "and looks for a district at the end"
    );
}

// ---- the gate, the wake and the held passes (MZB-305 to MZB-311, MZB-319) ----------------------

use rtsim_conflict::boundary::{Obs, RailPass, Uuid};
use rtsim_conflict::engine::{RailCall, StationEnd};

/// A RAIL_PASS of `train` for the den `faction`, a few blocks off its camp, exiting at `exit`.
fn pass_at(faction: u64, train: u64, owner: Uuid, pos: (f64, f64, f64), exit: u64) -> Obs {
    Obs::RailPass(RailPass {
        faction,
        train,
        owner,
        dimension: 0,
        pos,
        entry: exit as i64 - 100,
        exit: exit as i64,
    })
}

/// The usual pass: 80 blocks east of the den, on a track at y 66.
fn pass(train: u64, owner: Uuid, exit: u64) -> Obs {
    pass_at(CAMP, train, owner, (980.5, 66.25, 1000.5), exit)
}

const NOBODY: Uuid = (0, 0);

fn end(id: u64, x: i32, z: i32, district: Option<u64>) -> StationEnd {
    StationEnd {
        id,
        dim: 0,
        x,
        z,
        district,
    }
}

fn meter_of(c: &Conflict, f: u32, a: ActorId) -> RailMeter {
    c.factions.row(f).rail_meter(a).cloned().expect("a meter")
}

#[test]
fn the_gate_pushes_only_on_the_band_and_the_day() {
    // A leg a day settles at 1.3 legs a day, under the culture's tolerance of 2: counted, never
    // pushed.
    let (mut w, mut c, f, player) = dens(41);
    let mut probe = Probe::default();
    for day in 1..=5u64 {
        let t = Tick(day * TICKS_PER_DAY);
        c.ingest(&mut w, &mut probe, &pass(day, PLAYER, t.0), t);
    }
    let m = meter_of(&c, f, player);
    assert!(m.per_day(Tick(5 * TICKS_PER_DAY)) < 2.0);
    assert_eq!((c.stats.rail_pushes, m.pushed_at), (0, None));
    assert!(rail_source(&c, f, player).is_none());

    // Twelve a day: every push is the band or the day, the first one is the first rate at the
    // tolerance, and the wakes stay under three a day.
    let (mut w, mut c, f, player) = dens(42);
    let mut last: (f64, Option<Tick>) = (0.0, None);
    let mut pushes = 0;
    for step in 1..=(6 * 240u64) {
        let t = Tick(step * 100);
        if t.0 % 2_000 == 0 {
            c.ingest(&mut w, &mut probe, &pass(1, PLAYER, t.0), t);
        } else {
            c.run_until(&mut w, &mut probe, t);
        }
        let Some(m) = c.factions.row(f).rail_meter(player) else {
            continue;
        };
        if m.pushed_at == last.1 {
            continue;
        }
        pushes += 1;
        let (old_p, old_at) = last;
        let now = m.pushed_at.unwrap();
        let day = old_at.is_none_or(|a| now.0 >= a.0 + TICKS_PER_DAY);
        let band = (m.pushed - old_p).abs() >= (0.1 * old_p).max(1.0) - 1e-6;
        assert!(
            day || band,
            "a push at {now:?} from {old_p} to {}",
            m.pushed
        );
        if old_at.is_none() {
            assert!(m.pushed >= 2.0, "first push {}", m.pushed);
        }
        last = (m.pushed, m.pushed_at);
    }
    assert_eq!(pushes, c.stats.rail_pushes);
    assert!(pushes >= 6, "a push a day at least: {pushes}");
    assert!(c.stats.rail_wakes <= 3 * 6, "{} wakes", c.stats.rail_wakes);
    let s = rail_source(&c, f, player).expect("the source");
    assert!(
        (s.rate * TICKS_PER_DAY as f64 - 12.0).abs() < 2.0,
        "{}",
        s.rate * TICKS_PER_DAY as f64
    );
    // Its position is the pass's point, its pin the track's y.
    assert_eq!(s.pos, Pos::new(0, 980, 1000));
    assert_eq!(s.pin.as_deref().copied(), Some(track(66)));
}

#[test]
fn the_wake_is_the_band_edge_or_the_day() {
    // Four legs at once: 2.0 a day, which is not under the tolerance, so it is pushed. The band's
    // lower edge (1.0 a day) is 33,272 ticks on and the day is 24,000: the day comes first.
    let (mut w, mut c, f, player) = dens(43);
    let mut probe = Probe::default();
    for _ in 0..4 {
        c.ingest(&mut w, &mut probe, &pass(1, PLAYER, 10_000), Tick(10_000));
    }
    let m = meter_of(&c, f, player);
    assert_eq!((m.pushed, m.pushed_at), (2.0, Some(Tick(10_000))));
    assert_eq!(m.falls_to(1.0), Tick(10_000 + 33_272));
    assert_eq!(c.stats.rail_pushes, 1);
    let before = c.stats.rail_wakes;
    c.run_until(&mut w, &mut probe, Tick(33_999));
    assert_eq!(c.stats.rail_wakes, before, "not before the day is out");
    c.run_until(&mut w, &mut probe, Tick(34_000));
    assert_eq!(c.stats.rail_wakes, before + 1);
    // The resync pushed the rate now (1.21 a day, under the tolerance, so the meter is quiet).
    let m = meter_of(&c, f, player);
    assert_eq!(c.stats.rail_pushes, 2);
    assert_eq!(m.pushed_at, Some(Tick(34_000)));
    assert!(
        (m.pushed - 2.0 * (-0.5f64).exp()).abs() < 1e-9,
        "{}",
        m.pushed
    );

    // Twelve a day, then nothing: the band's lower edge comes well before the day.
    let (mut w, mut c, f, player) = dens(44);
    // The stream stops at the first pass after the fourth day that pushes: the day is then a full
    // day off and the band's edge a few thousand ticks.
    let mut t_last = Tick::ZERO;
    for k in 1..=(8 * 12u64) {
        let t = Tick(k * 2_000);
        c.ingest(&mut w, &mut probe, &pass(1, PLAYER, t.0), t);
        t_last = t;
        if t.0 > 4 * TICKS_PER_DAY && meter_of(&c, f, player).pushed_at == Some(t) {
            break;
        }
    }
    assert!(
        t_last.0 < 8 * TICKS_PER_DAY,
        "a pass pushed after the fourth day"
    );
    let m = meter_of(&c, f, player);
    let (p, pushed_at) = (m.pushed, m.pushed_at.unwrap());
    let edge = m.falls_to(p - (0.1 * p).max(1.0));
    let expect = edge
        .min(Tick(pushed_at.0 + TICKS_PER_DAY))
        .max(Tick(t_last.0 + 1));
    assert!(
        expect == edge,
        "the band edge is first: {edge:?} {pushed_at:?}"
    );
    let before = c.stats.rail_wakes;
    c.run_until(&mut w, &mut probe, Tick(expect.0 - 1));
    assert_eq!(c.stats.rail_wakes, before);
    let pushes = c.stats.rail_pushes;
    c.run_until(&mut w, &mut probe, expect);
    assert_eq!(c.stats.rail_wakes, before + 1);
    assert_eq!(
        c.stats.rail_pushes,
        pushes + 1,
        "the wake at the edge pushes"
    );
    let m = meter_of(&c, f, player);
    assert!(m.pushed < p * 0.91, "{} from {p}", m.pushed);
}

#[test]
fn a_quiet_meter_retires_and_pushes_zero() {
    // A meter that was pushed and has fallen under the tolerance keeps its source at the last
    // rate until it has fallen to 0.01 a day, when it pushes 0 and goes with its lines.
    let (mut w, mut c, f, player) = dens(45);
    let mut probe = Probe::default();
    for _ in 0..4 {
        c.ingest(&mut w, &mut probe, &pass(1, PLAYER, 10_000), Tick(10_000));
    }
    c.run_until(&mut w, &mut probe, Tick(34_000));
    let m = meter_of(&c, f, player);
    assert!(m.pushed > 0.0 && m.pushed < 2.0);
    let retire = m.falls_to(0.01);
    c.run_until(&mut w, &mut probe, Tick(retire.0 - 1));
    assert!(c.factions.row(f).rail_meter(player).is_some());
    assert!(rail_source(&c, f, player).is_some());
    c.run_until(&mut w, &mut probe, retire);
    assert!(c.factions.row(f).rail_meter(player).is_none(), "retired");
    assert!(rail_source(&c, f, player).is_none(), "pushed to 0");
    assert!(c.factions.row(f).reach_ids(SignalKind::Rail).is_empty());
    // Its feud, if it has not been forgotten by now, hears no rail.
    if let Some(fe) = c.feud(f, player) {
        assert_eq!(fe.raw[SignalKind::Rail.index()], 0.0);
    }

    // One leg, never pushed: it retires without a push.
    let (mut w, mut c, f, player) = dens(46);
    c.ingest(&mut w, &mut probe, &pass(1, PLAYER, 1_000), Tick(1_000));
    let retire = meter_of(&c, f, player).falls_to(0.01);
    assert!(retire.0 > 4 * TICKS_PER_DAY, "{retire:?}");
    c.run_until(&mut w, &mut probe, retire);
    assert!(c.factions.row(f).rail_meter(player).is_none());
    assert_eq!(c.stats.rail_pushes, 0);
    assert!(c.feud(f, player).is_none(), "it never made a feud");
}

#[test]
fn a_stale_rail_wake_is_counted() {
    // Each gate books the meter's wake anew: the one before it is stale when it fires.
    let (mut w, mut c, f, player) = dens(47);
    let mut probe = Probe::default();
    c.ingest(&mut w, &mut probe, &pass(1, PLAYER, 1_000), Tick(1_000));
    c.ingest(&mut w, &mut probe, &pass(1, PLAYER, 2_000), Tick(2_000));
    assert_eq!(c.pending_by_kind().get("rail"), Some(&2));
    let retire = meter_of(&c, f, player).falls_to(0.01);
    let stale = c.stats.stale;
    c.run_until(&mut w, &mut probe, Tick(retire.0 + 10));
    assert_eq!(c.stats.stale, stale + 1, "the first booking");
    assert_eq!(c.stats.rail_wakes, 1, "the second one retired the meter");
    assert!(c.factions.row(f).rail_meter(player).is_none());
    assert_eq!(c.pending_by_kind().get("rail"), None);
}

#[test]
fn rail_pass_for_an_unknown_faction_or_a_zero_weight_culture_is_dropped() {
    let (mut w, mut c, f, player) = dens(48);
    let mut probe = Probe::default();
    // An unknown faction key.
    c.ingest(
        &mut w,
        &mut probe,
        &pass_at(0xDEAD, 3, PLAYER, (980.5, 66.0, 1000.5), 1_000),
        Tick(1_000),
    );
    // An Umvuthana den minds no rail.
    let u = c
        .add_faction(
            &mut w,
            DEN_B,
            "umvuthana",
            Pos::new(0, CAMP_POS.x + 300, CAMP_POS.z),
            Tick::ZERO,
        )
        .unwrap();
    c.ingest(
        &mut w,
        &mut probe,
        &pass_at(DEN_B, 4, PLAYER, (1180.5, 66.0, 1000.5), 1_100),
        Tick(1_100),
    );
    assert_eq!((c.stats.rail_unknown, c.stats.rail_passes), (2, 0));
    assert!(!c.holds_train(3) && !c.holds_train(4));
    assert!(c.factions.row(u).rail.is_empty() && c.factions.row(f).rail.is_empty());
    assert!(c.feud(u, player).is_none());
    // A pass for the Frostmaw den is taken.
    c.ingest(&mut w, &mut probe, &pass(5, PLAYER, 1_200), Tick(1_200));
    assert_eq!((c.stats.rail_unknown, c.stats.rail_passes), (2, 1));
}

#[test]
fn an_owned_pass_counts_at_once_and_an_ownerless_one_on_credit() {
    let (mut w, mut c, f, player) = dens(49);
    let mut probe = Probe::default();
    // An owned pass is counted when it comes (its exit clamped to the present) and held.
    c.ingest(&mut w, &mut probe, &pass(7, PLAYER, 90_000), Tick(1_000));
    let m = meter_of(&c, f, player);
    assert_eq!(
        (m.n, m.t_ref),
        (1.0, Tick(1_000)),
        "an exit after now is now"
    );
    assert_eq!(m.last, Some((Pos::new(0, 980, 1000), 66)));
    assert!(c.holds_train(7) && c.held_passes() == 1);
    // The train arrives: the line is credited, the count is not made twice.
    let (a, b) = (
        end(11, CAMP_POS.x - 200, CAMP_POS.z, None),
        end(12, CAMP_POS.x + 300, CAMP_POS.z, Some(5)),
    );
    c.rail_call(
        &mut w,
        &RailCall {
            train: 7,
            line: Some((b, a)),
        },
        Tick(2_000),
    );
    let m = meter_of(&c, f, player);
    assert_eq!(m.n, 1.0);
    assert_eq!(m.lines.len(), 1);
    let l = &m.lines[0];
    assert_eq!((l.a, l.b, l.n), (11, 12, 1.0), "the lower id first");
    assert_eq!(
        (l.point, l.y, l.near),
        (
            Pos::new(0, 980, 1000),
            66,
            Some((Pos::new(0, CAMP_POS.x + 300, CAMP_POS.z), DistrictId(5)))
        ),
        "the only station with a district"
    );
    assert!(!c.holds_train(7) && c.held_passes() == 0);
    assert_eq!((c.stats.rail_calls, c.stats.rail_let_go), (1, 0));
    // The busiest line is the source's pin: the next push carries the station.
    for k in 0..5 {
        c.ingest(
            &mut w,
            &mut probe,
            &pass(8, PLAYER, 3_000 + k),
            Tick(3_000 + k),
        );
    }
    c.rail_call(
        &mut w,
        &RailCall {
            train: 8,
            line: Some((a, b)),
        },
        Tick(3_100),
    );
    c.run_until(&mut w, &mut probe, Tick(3_200));
    let s = rail_source(&c, f, player).expect("pushed");
    assert_eq!(
        s.pin.as_deref().and_then(|p| p.station),
        Some((Pos::new(0, CAMP_POS.x + 300, CAMP_POS.z), DistrictId(5)))
    );

    // An ownerless pass is held and counted for nobody until its line is named; then it counts
    // against the player with the most time in the two stations' chunks.
    let (mut w, mut c, f, player) = dens(50);
    let station_a = Pos::new(0, CAMP_POS.x - 200, CAMP_POS.z);
    c.observe_presence(&mut w, PLAYER, station_a, Tick(100));
    c.observe_presence(&mut w, PLAYER, station_a, Tick(600));
    c.ingest(&mut w, &mut probe, &pass(9, NOBODY, 1_000), Tick(1_000));
    assert!(c.factions.row(f).rail.is_empty(), "held, not counted");
    assert!(c.holds_train(9));
    c.rail_call(
        &mut w,
        &RailCall {
            train: 9,
            line: Some((
                end(21, station_a.x, station_a.z, None),
                end(22, 1180, 1000, None),
            )),
        },
        Tick(1_500),
    );
    let m = meter_of(&c, f, player);
    assert_eq!((m.n, m.t_ref), (1.0, Tick(1_000)), "counted at its exit");
    assert_eq!(m.lines.len(), 1);
    assert_eq!(m.lines[0].near, None, "neither station has a district");
    // With nobody at either station it counts against nobody.
    let (mut w, mut c, f, _) = dens(51);
    c.ingest(&mut w, &mut probe, &pass(9, NOBODY, 1_000), Tick(1_000));
    c.rail_call(
        &mut w,
        &RailCall {
            train: 9,
            line: Some((end(21, 700, 1000, None), end(22, 1180, 1000, None))),
        },
        Tick(1_500),
    );
    assert!(c.factions.row(f).rail.is_empty());
    assert!(!c.holds_train(9));
    assert_eq!(c.stats.rail_let_go, 0, "credited, not let go");
}

#[test]
fn an_ownerless_pass_let_go_counts_against_the_chunks_top_or_nobody() {
    let day = TICKS_PER_DAY;
    // The player stood where the train passed: a day on, the pass counts against them, at its
    // exit. Letting go runs first in the next call into the engine, whichever it is.
    let (mut w, mut c, f, player) = dens(52);
    let mut probe = Probe::default();
    let point = Pos::new(0, 980, 1000);
    c.observe_presence(&mut w, PLAYER, point, Tick(100));
    c.observe_presence(&mut w, PLAYER, point, Tick(600));
    c.ingest(&mut w, &mut probe, &pass(9, NOBODY, 1_000), Tick(1_000));
    c.rail_call(
        &mut w,
        &RailCall {
            train: 99,
            line: None,
        },
        Tick(1_000 + day - 1),
    );
    assert!(c.holds_train(9), "not before the day is out");
    assert_eq!(c.stats.rail_let_go, 0);
    c.rail_call(
        &mut w,
        &RailCall {
            train: 99,
            line: None,
        },
        Tick(1_000 + day),
    );
    assert!(!c.holds_train(9));
    assert_eq!(c.stats.rail_let_go, 1);
    let m = meter_of(&c, f, player);
    assert_eq!((m.n, m.t_ref), (1.0, Tick(1_000)));
    assert!(m.lines.is_empty(), "no line was named");

    // Nobody there: the pass counts against nobody; the settlers do not get it.
    let (mut w, mut c, f, _) = dens(53);
    c.ingest(&mut w, &mut probe, &pass(9, NOBODY, 1_000), Tick(1_000));
    c.ingest(
        &mut w,
        &mut probe,
        &pass(10, NOBODY, 1_000 + day),
        Tick(1_000 + day),
    );
    assert_eq!(c.stats.rail_let_go, 1, "the first went, the second is held");
    assert!(c.holds_train(10) && !c.holds_train(9));
    assert!(c.factions.row(f).rail.is_empty());
    // A rail wake does it too, and an owned pass is only dropped (it was counted when it came).
    let (mut w, mut c, f, player) = dens(54);
    c.ingest(&mut w, &mut probe, &pass(9, PLAYER, 1_000), Tick(1_000));
    c.run_until(&mut w, &mut probe, Tick(9 * day));
    assert!(!c.holds_train(9), "the retirement wake let it go");
    assert!(
        c.factions.row(f).rail_meter(player).is_none(),
        "and retired it"
    );
    assert_eq!(c.stats.rail_let_go, 1);
}

#[test]
fn held_passes_are_capped_per_train_and_in_all() {
    // A train's 65th pass lets its oldest go. Nobody is at the points; a player stands at the
    // stations, so what the call credits is what was still held.
    let (mut w, mut c, f, player) = dens(55);
    let mut probe = Probe::default();
    let station_a = Pos::new(0, CAMP_POS.x - 200, CAMP_POS.z);
    c.observe_presence(&mut w, PLAYER, station_a, Tick(10));
    c.observe_presence(&mut w, PLAYER, station_a, Tick(60));
    for k in 1..=70u64 {
        c.ingest(
            &mut w,
            &mut probe,
            &pass(5, NOBODY, 100 + k * 100),
            Tick(100 + k * 100),
        );
    }
    assert_eq!(c.held_passes(), 64);
    assert_eq!(c.stats.rail_let_go, 6);
    c.rail_call(
        &mut w,
        &RailCall {
            train: 5,
            line: Some((
                end(21, station_a.x, station_a.z, None),
                end(22, 1180, 1000, None),
            )),
        },
        Tick(8_000),
    );
    let m = meter_of(&c, f, player);
    // The 64 newest (exits 800 to 7,100), not the first six.
    let want: f64 = (7..=70u64)
        .map(|k| (-((7_100 - (100 + k * 100)) as f64) / TAU).exp())
        .sum();
    assert!((m.n_at(Tick(7_100)) - want).abs() < 1e-9, "{} {want}", m.n);
    assert_eq!(c.held_passes(), 0);

    // 65 trains of 64 passes make 4,160: the table's own cap lets the 64 oldest of all go, which
    // are the first train's.
    let (mut w, mut c, _, _) = dens(56);
    for train in 1..=65u64 {
        for j in 1..=64u64 {
            let t = 1_000 + train * 64 + j;
            c.ingest(&mut w, &mut probe, &pass(train, NOBODY, t), Tick(t));
        }
    }
    assert_eq!(c.held_passes(), 4_096);
    assert_eq!(c.stats.rail_let_go, 64);
    assert!(!c.holds_train(1) && c.holds_train(2) && c.holds_train(65));
}

#[test]
fn a_self_edge_or_no_departure_lets_go() {
    // The pass is let go now, not credited to any line: counted against the point chunk's top
    // when it had no owner, and no line is made.
    for line in [
        None,
        Some((end(21, 700, 1000, None), end(21, 700, 1000, None))),
    ] {
        let (mut w, mut c, f, player) = dens(57);
        let mut probe = Probe::default();
        let point = Pos::new(0, 980, 1000);
        c.observe_presence(&mut w, PLAYER, point, Tick(100));
        c.observe_presence(&mut w, PLAYER, point, Tick(600));
        c.ingest(&mut w, &mut probe, &pass(9, NOBODY, 1_000), Tick(1_000));
        assert!(c.holds_train(9));
        c.rail_call(&mut w, &RailCall { train: 9, line }, Tick(1_200));
        assert!(!c.holds_train(9));
        assert_eq!((c.stats.rail_calls, c.stats.rail_let_go), (1, 1));
        let m = meter_of(&c, f, player);
        assert_eq!(m.n, 1.0);
        assert!(m.lines.is_empty());
    }
    // A call for a train that holds nothing is only counted.
    let (mut w, mut c, f, _) = dens(58);
    c.rail_call(
        &mut w,
        &RailCall {
            train: 1,
            line: None,
        },
        Tick(5),
    );
    assert_eq!((c.stats.rail_calls, c.stats.rail_let_go), (1, 0));
    assert!(c.factions.row(f).rail.is_empty());
}

#[test]
fn the_nearer_station_with_a_district_is_the_pin() {
    let (mut w, mut c, f, player) = dens(59);
    let mut probe = Probe::default();
    let (a, b) = (
        end(31, CAMP_POS.x + 100, CAMP_POS.z, Some(8)),
        end(32, CAMP_POS.x + 600, CAMP_POS.z, Some(9)),
    );
    // The point is 80 east of the den: A (100 east) is the nearer.
    c.ingest(&mut w, &mut probe, &pass(1, PLAYER, 1_000), Tick(1_000));
    c.rail_call(
        &mut w,
        &RailCall {
            train: 1,
            line: Some((b, a)),
        },
        Tick(1_100),
    );
    let m = meter_of(&c, f, player);
    assert_eq!(
        m.lines[0].near,
        Some((Pos::new(0, CAMP_POS.x + 100, CAMP_POS.z), DistrictId(8)))
    );
    // Another pass far east: the same line's point and station follow the latest pass.
    c.ingest(
        &mut w,
        &mut probe,
        &pass_at(CAMP, 2, PLAYER, (1480.5, 66.0, 1000.5), 2_000),
        Tick(2_000),
    );
    c.rail_call(
        &mut w,
        &RailCall {
            train: 2,
            line: Some((a, b)),
        },
        Tick(2_100),
    );
    let m = meter_of(&c, f, player);
    assert_eq!(m.lines.len(), 1);
    assert_eq!(m.lines[0].point, Pos::new(0, 1480, 1000));
    assert_eq!(
        m.lines[0].near,
        Some((Pos::new(0, CAMP_POS.x + 600, CAMP_POS.z), DistrictId(9)))
    );
}
