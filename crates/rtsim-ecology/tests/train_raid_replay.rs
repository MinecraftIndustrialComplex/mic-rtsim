// SPDX-License-Identifier: AGPL-3.0-or-later
//! A dragon's train raid across a crash (docs/choices-mowzie-build.md RP-180 to RP-186; the
//! harness is `common/train_raid.rs`). The dragons have no replay mode: a dragon whose pages are
//! older than its raid flies its window again after a restart and raids the train again, against
//! an industry engine that is not held. The raid's op is its identity, so Java answers DUPLICATE
//! and the goods leave the real train once.

#[path = "common/train_raid.rs"]
mod train_raid;

use rtsim_ecology::dragon::LairKind;
use rtsim_ecology::engine::{Dragons, NewDragon};
use rtsim_ecology::infra::{DistrictIndex, IndustryLink};
use rtsim_ecology::species::{Gender, Species};
use rtsim_ecology::synth;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::TrainId;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};
use train_raid::{Landed, Order, Raider, across_a_crash, industry, industry_newer};

const SEED: u64 = 19;
const TRAIN: TrainId = TrainId(7);
/// The dragon is saved after it leaves for lair B, and the server dies when the raid is long
/// over (the dragons' own train test runs to 30,000 too).
const S: Tick = Tick(1_500);
const C: Tick = Tick(30_000);

/// A hungry fire dragon and the world it flies in.
struct Dragon {
    w: World,
    d: Dragons,
    index: DistrictIndex,
}

impl Raider for Dragon {
    type Saved = (Vec<u8>, Vec<u8>);

    fn save(&self) -> Self::Saved {
        (self.w.save(), self.d.save())
    }

    fn restore(saved: &Self::Saved) -> Dragon {
        let (goods, ids) = synth::goods();
        let book = synth::recipes(&ids);
        Dragon {
            w: World::load(WorldConfig::new(SEED), goods, book, &saved.0).expect("world loads"),
            d: Dragons::load(DragonTunables::default(), &saved.1).expect("dragons load"),
            index: DistrictIndex::default(),
        }
    }

    fn run(&mut self, e: &mut Engine, to: Tick) {
        let mut link = IndustryLink {
            engine: e,
            index: &mut self.index,
        };
        self.d.run_until(&mut self.w, &mut link, to);
    }

    /// No replay mode: the window is flown again as it was flown.
    fn replay(&mut self, e: &mut Engine, to: Tick) {
        self.run(e, to);
    }
}

/// The dragons' own train scenario (dragons.rs `train_run`): lairs either side of a line at z
/// 6,000, a hungry adult leaving A for B at 1,500. The leg is long (100,000 ticks), so that it is
/// still under way when an industry engine far past the raid meets the dragon again.
fn dragon() -> (Dragon, Engine) {
    let (mut w, _) = synth::world(SEED);
    let mut d = Dragons::new(SEED, DragonTunables::default(), Tick::ZERO);
    let mut e = industry(SEED, TRAIN, Tick(1_000), 100_000, 6_000);
    let lair = |d: &mut Dragons, w: &mut World, key: u64, name: &str, z: i32| {
        d.add_lair(
            w,
            key,
            name,
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, 0, z),
            100,
            Tick(0),
        )
    };
    let a = lair(&mut d, &mut w, 1, "A", 3_000);
    let b = lair(&mut d, &mut w, 2, "B", 9_000);
    let x = d.add_dragon(
        &mut w,
        NewDragon {
            name: "x".into(),
            species: Species::Fire,
            gender: Gender::Female,
            variant: 0,
            age_ticks: 110 * TICKS_PER_DAY,
            lair: Some(a),
            fullness: 40,
            health: None,
            seed: None,
            at: None,
        },
        Tick(0),
    );
    let mut r = Dragon {
        w,
        d,
        index: DistrictIndex::default(),
    };
    r.run(&mut e, S);
    r.d.relocate(&mut r.w, x, b, S);
    (r, e)
}

/// The dragon's pages are older than its raid and Java applied it (both of Java's saves landed
/// after it): the dragon raids the train again in its replay, at industry's later `now`. The
/// goods leave the real train once, in either order of the journal's carriage result, and the
/// books agree with the live run's. Against main's derivation the re-decided raid is a new op
/// and Java takes the goods twice.
#[test]
fn a_dragons_train_raid_replayed_after_a_crash_takes_the_goods_once() {
    for order in [Order::ResultFirst, Order::RaidFirst] {
        let (r, e) = dragon();
        let x = across_a_crash(r, e, TRAIN, S, C, Landed::J1, order);
        // The live control: one raid, its loot off the train once.
        assert_eq!(x.raider.d.stats.train_raids, 1, "{order:?}");
        assert_eq!(x.java.applied, 1, "{order:?}");
        let live = x.java.taken();
        assert!(live > 0, "{order:?}");
        assert_eq!(live as i128, x.live.books.stolen.total());
        // The replay raided again, and Java knew the op.
        assert_eq!(
            x.again.d.stats.train_raids, 1,
            "{order:?}: the dragon raids again"
        );
        assert_eq!(
            x.java_again.taken(),
            live,
            "{order:?}: the goods leave the train once"
        );
        assert_eq!(x.java_again.applied, 1, "{order:?}");
        assert_eq!(x.java_again.duplicates, 1, "{order:?}");
        // O2 drops the exchange the re-decision made; in O1 the result took it first.
        let dropped = u32::from(order == Order::ResultFirst);
        assert_eq!(x.java_again.dropped, dropped, "{order:?}");
        assert_eq!(x.restarted.books.stolen, x.live.books.stolen, "{order:?}");
        x.restarted.check_books().unwrap();
    }
}

/// Industry's pages are newer than the dragon's (unreachable with one save batch, RP-183): the
/// dragon's replay meets an engine past the train's cooldown that holds the raid. Nothing is
/// booked again and nothing reaches Java. Against main's derivation the engine books it again
/// under a new op and Java takes the goods twice.
#[test]
fn a_dragons_train_raid_held_by_industry_is_booked_once() {
    let (r, e) = dragon();
    let (live, after, e) = industry_newer(r, e, TRAIN, S, C, Tick(60_000));
    assert_eq!(live.applied, 1);
    assert_eq!(
        after.taken(),
        live.taken(),
        "the goods leave the train once"
    );
    assert_eq!((after.applied, after.duplicates), (1, 0));
    assert_eq!(e.books.stolen.total(), live.taken() as i128);
}

/// What A guarantees by which of Java's saves landed after the raid's application (RP-184): both
/// (J1) or neither (J2), the goods leave once; the train's goods without the journal (J3), twice,
/// as on main (the double RP-185's Java change would turn into a loss); the journal without the
/// goods (J4), never: Java remembers the op and the goods it restored stay on the train while
/// the books hold them stolen (a loss).
#[test]
fn which_of_javas_saves_landed_decides_once_twice_or_never() {
    let (r, e) = dragon();
    let live = across_a_crash(r, e, TRAIN, S, C, Landed::J1, Order::ResultFirst)
        .java
        .taken();
    for (landed, taken) in [
        (Landed::J1, live),
        (Landed::J2, live),
        (Landed::J3, 2 * live),
        (Landed::J4, 0),
    ] {
        let (r, e) = dragon();
        let x = across_a_crash(r, e, TRAIN, S, C, landed, Order::ResultFirst);
        assert_eq!(x.java_again.taken(), taken, "{landed:?}");
        // The engine's books hold the re-decided raid's loot whatever Java did.
        assert_eq!(x.restarted.books.stolen.total(), live as i128, "{landed:?}");
    }
}
