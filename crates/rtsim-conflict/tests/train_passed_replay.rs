// SPDX-License-Identifier: AGPL-3.0-or-later
//! A lurking band's robbery of a passing train (M8's `train_passed`) across a crash
//! (docs/choices-mowzie-build.md RP-180 to RP-187; the harness is rtsim-ecology's
//! `tests/common/train_raid.rs`, from fix/replay-train-ops).
//!
//! M8's `train_passed` has no replay guard (RP-187; RP-154's guard B, which counted a replayed
//! pass and rolled nothing, is gone): the pass the replay brings back rolls again, at its
//! journaled tick, so it rolls what it rolled live and its threats carry the live ids. The last test
//! models the train as slice T2 has it (the ring) and as slice T had it (the journal's index), as
//! the other raiders' tests do.
//!
//! A band whose pages are older than a pass it robbed comes back in the replay and takes the
//! journaled TRAIN_PASSED again, at its tick. Industry is not held (docs/replay-gate.md §3.6): it
//! jumps to Java's present before the band re-decides. Each threat's op is its identity
//! (`attack_key`: id, source, class, train), so Java answers DUPLICATE and the goods leave the
//! real train once. Against main's derivation (the op from industry's `now`) the re-decided
//! threats are new ops: `java_again.taken()` is twice the live take and `duplicates` is 0.

#[path = "../../rtsim-ecology/tests/common/train_raid.rs"]
mod train_raid;

use rtsim_conflict::boundary::{Obs, TrainPassed};
use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::engine::{Conflict, Party, Target};
use rtsim_conflict::synth::{self, CAMP, CAMP_POS, PLAYER};
use rtsim_conflict::{ConflictTunables, Cultures};
use rtsim_ecology::infra::{
    DistrictSite, Infrastructure, NoInfrastructure, RaidOutcome, TrainSighting,
};
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{Good, GoodVec, TrainId};
use rtsim_industry::threat::Threat;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::handle::Handle;
use rtsim_settle::Pos;
use rtsim_world::World;
use train_raid::{
    Dedup, Landed, Order, Raider, Rails, across_a_crash, across_a_crash_on, industry,
    industry_newer,
};

const SEED: u64 = 75;
const TRAIN: TrainId = TrainId(7);

/// lurk.rs `track_target`: the track a hundred blocks east of the den, the spot five toward it.
fn track_target() -> Target {
    let track = Pos::new(0, CAMP_POS.x + 100, CAMP_POS.z);
    Target::Track {
        track,
        spot: Pos::new(0, track.x - 5, track.z),
        y: 66,
    }
}

/// The harness's `Rails` with no train crossing any leg: only the pass attacks the train, so a
/// band that left its track would not ambush on its way.
struct Track<'a>(Rails<'a>);

impl Infrastructure for Track<'_> {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }

    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        None
    }

    fn good_name(&self, g: Good) -> Option<String> {
        self.0.good_name(g)
    }

    fn trains_crossed(&mut self, _: u16, _: &[(i32, i32)], _: Tick, _: Tick) -> Vec<TrainSighting> {
        Vec::new()
    }

    fn raid_train(&mut self, t: &Threat) -> Option<GoodVec> {
        self.0.raid_train(t)
    }
}

/// A Frostmaw band lying in wait by the track, and the pass of [`TRAIN`] it robs: the
/// TRAIN_PASSED the journal holds, at its tick.
struct Lurker {
    w: World,
    c: Conflict,
    p: Handle<Party>,
    tp: TrainPassed,
    pass: Tick,
    raids: Vec<(Threat, Option<GoodVec>)>,
}

/// Every roll a hit (lurk.rs sets the same on its lurks): tunables are not in the pages.
fn tun() -> ConflictTunables {
    ConflictTunables {
        ambush_milli: 1000,
        ..ConflictTunables::default()
    }
}

impl Raider for Lurker {
    type Saved = (Vec<u8>, Vec<u8>, Handle<Party>, TrainPassed, Tick);

    fn save(&self) -> Self::Saved {
        (self.w.save(), self.c.save(), self.p, self.tp, self.pass)
    }

    fn restore(saved: &Self::Saved) -> Lurker {
        let (goods, ids) = rtsim_world::synth::goods();
        let book = rtsim_world::synth::recipes(&ids);
        Lurker {
            w: World::load(synth::world_config(SEED), goods, book, &saved.0).expect("world loads"),
            c: Conflict::load(tun(), Cultures::builtin(), &saved.1).expect("conflict loads"),
            p: saved.2,
            tp: saved.3,
            pass: saved.4,
            raids: Vec::new(),
        }
    }

    /// Runs to `to`, taking the pass at its tick on the way.
    fn run(&mut self, e: &mut Engine, to: Tick) {
        let mut track = Track(Rails {
            engine: e,
            train: TRAIN,
            raids: &mut self.raids,
        });
        if self.c.now() < self.pass && self.pass <= to {
            self.c.ingest(
                &mut self.w,
                &mut track,
                &Obs::TrainPassed(self.tp),
                self.pass,
            );
        }
        self.c.run_until(&mut self.w, &mut track, to);
        self.c.take_commands();
    }

    /// The session's replay: the journaled pass again, at its tick (guard B gone, it rolls).
    fn replay(&mut self, e: &mut Engine, to: Tick) {
        self.c.begin_replay(true);
        self.run(e, to);
        self.c.replay_done();
    }
}

/// lurk.rs `Lurk::raid` and `until_lurking`: a Frostmaw raid ordered to the track at tick 10,
/// run hour by hour with no infrastructure until it lurks. The band is saved there (`s`), the
/// train passes ten ticks later and the server dies a tick after the pass (`c`).
fn lurker() -> (Lurker, Engine, Tick, Tick) {
    let (mut w, _) = synth::world(SEED);
    let mut c = synth::conflict(SEED, &w);
    c.tun = tun();
    c.add_faction(&mut w, CAMP, "frostmaw", CAMP_POS, Tick::ZERO)
        .unwrap();
    c.player(&mut w, PLAYER);
    let p = c
        .order_party(
            &mut w,
            CAMP,
            PartyKind::Raid,
            track_target(),
            None,
            Tick(10),
        )
        .expect("a band");
    let mut t = Tick(10);
    while !c.lurking(p) {
        t = Tick(t.0 + 1_000);
        assert!(t < Tick::from_days(20), "the band never got there");
        c.run_until(&mut w, &mut NoInfrastructure, t);
    }
    c.take_commands();
    let s = c.now();
    let pass = Tick(s.0 + 10);
    let tp = TrainPassed {
        agent: Party::agent(p),
        epoch: c.parties[p].epoch,
        flags: 0,
        carriage: 0,
        train: TRAIN.0,
    };
    let e = industry(SEED, TRAIN, Tick(1), 20 * 24_000, 0);
    let r = Lurker {
        w,
        c,
        p,
        tp,
        pass,
        raids: Vec::new(),
    };
    (r, e, s, Tick(pass.0 + 1))
}

/// The band's pages are older than the pass it robbed and Java applied the robbery (both of
/// Java's saves landed after it): the replayed pass rolls the same hit at the same tick and the
/// band robs the train again, at industry's later `now`. The goods leave the real train once, in
/// either order of the journal's carriage results, and the books agree with the live run's.
/// Against main's derivation: `java_again.taken()` is `2 * live`, `duplicates` 0.
#[test]
fn a_replayed_train_passed_robs_the_train_once() {
    for order in [Order::ResultFirst, Order::RaidFirst] {
        let (r, e, s, c) = lurker();
        let x = across_a_crash(r, e, TRAIN, s, c, Landed::J1, order);
        // The live control: the pass robbed, a threat per class that took something.
        assert_eq!(x.raider.c.stats.passes, 1, "{order:?}");
        assert_eq!(x.raider.c.stats.lurk_robberies, 1, "{order:?}");
        let ops = x.java.applied;
        assert!(ops >= 1, "{order:?}");
        let live = x.java.taken();
        assert!(live > 0, "{order:?}");
        assert_eq!(live as i128, x.live.books.stolen.total());
        // The replay brought the pass back, it rolled and robbed again (guard B is gone), and
        // Java knew every op.
        assert_eq!(x.again.c.stats.passes, 1, "{order:?}");
        assert_eq!(
            x.again.c.stats.lurk_robberies, 1,
            "{order:?}: the replayed pass robs again"
        );
        let ids = |r: &Lurker| r.raids.iter().map(|(t, _)| t.id).collect::<Vec<_>>();
        assert_eq!(
            ids(&x.again),
            ids(&x.raider),
            "{order:?}: the live threat ids"
        );
        assert_eq!(
            x.java_again.taken(),
            live,
            "{order:?}: the goods leave the train once"
        );
        assert_eq!(x.java_again.applied, ops, "{order:?}");
        assert_eq!(x.java_again.duplicates, ops, "{order:?}");
        // O2 drops each exchange the re-decision made; in O1 the results took them first.
        let dropped = if order == Order::ResultFirst { ops } else { 0 };
        assert_eq!(x.java_again.dropped, dropped, "{order:?}");
        assert_eq!(x.restarted.books.stolen, x.live.books.stolen, "{order:?}");
        x.restarted.check_books().unwrap();
        x.again.c.check(&x.again.w).expect("books");
    }
}

/// Industry's pages are newer than the band's (RP-183): the replayed pass meets an engine past
/// the train's cooldown that holds the robbery. Nothing is booked again, nothing reaches Java,
/// and the band sends no TRAIN_ROBBED (`ambush` took nothing). Against main's derivation the
/// engine books it again under new ops and Java takes the goods twice.
#[test]
fn a_replayed_train_passed_held_by_industry_is_booked_once() {
    let (r, e, s, c) = lurker();
    let (live, after, e) = industry_newer(r, e, TRAIN, s, c, Tick(c.0 + 30_000));
    assert!(live.applied >= 1);
    assert_eq!(
        after.taken(),
        live.taken(),
        "the goods leave the train once"
    );
    assert_eq!((after.applied, after.duplicates), (live.applied, 0));
    assert_eq!(e.books.stolen.total(), live.taken() as i128);
}

/// What each of Java's saves landing after the robbery gives (RP-184, RP-196), by T2's ring and
/// by the journal's index alone (slice T, and T2's fallback when the ring's mixin did not apply),
/// as for the other raiders. Both saves (J1) or neither (J2): once, by either. By the ring, the
/// train's goods and the op it remembers are one save, so J3 (the goods without the journal) and
/// J4 (the journal without the goods) give once too. By the journal's index: J3 twice (Java
/// forgot the op the goods remember); J4 never, a dupe (Java remembers the op the restored goods
/// forgot, so the train keeps its goods while the books hold them stolen).
#[test]
fn which_of_javas_saves_landed_by_the_ring_and_by_the_journal() {
    let (r, e, s, c) = lurker();
    let live = across_a_crash(r, e, TRAIN, s, c, Landed::J1, Order::ResultFirst)
        .java
        .taken();
    assert!(live > 0);
    for (dedup, landed, taken) in [
        (Dedup::Ring, Landed::J1, live),
        (Dedup::Ring, Landed::J2, live),
        (Dedup::Ring, Landed::J3, live),
        (Dedup::Ring, Landed::J4, live),
        (Dedup::Journal, Landed::J1, live),
        (Dedup::Journal, Landed::J2, live),
        (Dedup::Journal, Landed::J3, 2 * live),
        (Dedup::Journal, Landed::J4, 0),
    ] {
        let (r, e, s, c) = lurker();
        let x = across_a_crash_on(dedup, r, e, TRAIN, s, c, landed, Order::ResultFirst);
        assert_eq!(x.java_again.taken(), taken, "{dedup:?} {landed:?}");
        // The band robbed the train again in its replay, whatever Java did.
        assert_eq!(x.again.c.stats.lurk_robberies, 1, "{dedup:?} {landed:?}");
        // The engine's books hold the re-decided robbery's loot whatever Java did.
        assert_eq!(
            x.restarted.books.stolen.total(),
            live as i128,
            "{dedup:?} {landed:?}"
        );
    }
}
