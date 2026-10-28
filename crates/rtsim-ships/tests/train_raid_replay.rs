// SPDX-License-Identifier: AGPL-3.0-or-later
//! A pirate's coast raid on a train (`coast_raid`) across a crash (docs/choices-mowzie-build.md
//! RP-180 to RP-188; the harness is rtsim-ecology's `tests/common/train_raid.rs`). A raid the
//! ships re-decide while the session's replay runs is deferred and dropped (`Outbox::defer`,
//! slice S): lost, never doubled. One they re-decide after the replay's end, when the replay
//! clock stopped short of it (it moves on non-industry records only), runs live against an
//! industry engine at a later `now`: its op is its identity, so Java answers DUPLICATE and the
//! goods leave the real train once.

#[path = "../../rtsim-ecology/tests/common/train_raid.rs"]
mod train_raid;

use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{GoodVec, TrainId};
use rtsim_industry::threat::{Threat, ThreatSource};
use rtsim_kernel::clock::Tick;
use rtsim_ships::synth::{self, Coastal};
use rtsim_ships::{ShipTunables, Ships};
use rtsim_world::World;
use train_raid::{Landed, Order, Raider, Rails, across_a_crash, industry, industry_newer};

const SEED: u64 = 61;
const TRAIN: TrainId = TrainId(5);

fn tun() -> ShipTunables {
    ShipTunables {
        coast_raid_milli: 2_000,
        ..ShipTunables::default()
    }
}

/// Where in the restarted session a re-decided raid falls (RP-188).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Falls {
    /// Inside the replay (the replay clock passed it): deferred.
    InReplay,
    /// After the replay's end (the replay clock stopped before it): live.
    AfterReplay,
}

/// The pirates' coast and the world.
struct Pirates {
    c: Coastal,
    falls: Falls,
    raids: Vec<(Threat, Option<GoodVec>)>,
}

impl Raider for Pirates {
    type Saved = (Vec<u8>, Vec<u8>, Falls);

    fn save(&self) -> Self::Saved {
        (self.c.world.save(), self.c.ships.save(), self.falls)
    }

    fn restore(saved: &Self::Saved) -> Pirates {
        let (goods, ids) = rtsim_world::synth::goods();
        let book = rtsim_world::synth::recipes(&ids);
        Pirates {
            c: Coastal {
                world: World::load(synth::world_config(SEED), goods, book, &saved.0)
                    .expect("world loads"),
                ships: Ships::load(tun(), &saved.1).expect("ships load"),
                mask: synth::coast(),
                haven: 0,
                navy: None,
            },
            falls: saved.2,
            raids: Vec::new(),
        }
    }

    fn run(&mut self, e: &mut Engine, to: Tick) {
        let mut rails = Rails {
            engine: e,
            train: TRAIN,
            raids: &mut self.raids,
        };
        self.c.ships.run_until(&mut self.c.world, &mut rails, to);
    }

    fn replay(&mut self, e: &mut Engine, to: Tick) {
        self.c.ships.begin_replay(true);
        match self.falls {
            Falls::InReplay => {
                self.run(e, to);
                self.c.ships.replay_done(&mut self.c.world);
            }
            Falls::AfterReplay => {
                self.c.ships.replay_done(&mut self.c.world);
                self.run(e, to);
            }
        }
    }
}

fn pirates(falls: Falls) -> (Pirates, Engine) {
    let e = industry(SEED, TRAIN, Tick(1), 20 * 24_000, 0);
    (
        Pirates {
            c: synth::coastal(SEED, tun()),
            falls,
            raids: Vec::new(),
        },
        e,
    )
}

/// The first raid that took something from the train, live: the pirates are saved shortly
/// before it, and the server dies just after it.
fn first_raid() -> (Tick, Tick) {
    let (mut p, mut e) = pirates(Falls::AfterReplay);
    p.run(&mut e, Tick(12 * 24_000));
    let t = p
        .raids
        .iter()
        .find(|(_, l)| l.is_some())
        .expect("a raid on the train")
        .0
        .at;
    assert!(t.0 > 100, "{t:?}");
    (Tick(t.0 - 100), Tick(t.0 + 1))
}

/// The re-decided raid falls after the replay's end: it runs live, at industry's later `now`. The
/// goods leave the real train once, in either order of the journal's carriage result. Against
/// main's derivation it is a new op and Java takes the goods twice.
#[test]
fn a_coast_raid_after_the_replay_takes_the_goods_once() {
    let (s, c) = first_raid();
    for order in [Order::ResultFirst, Order::RaidFirst] {
        let (p, e) = pirates(Falls::AfterReplay);
        let x = across_a_crash(p, e, TRAIN, s, c, Landed::J1, order);
        let took: Vec<_> = x.raider.raids.iter().filter(|(_, l)| l.is_some()).collect();
        assert_eq!(took.len(), 1, "{order:?}");
        assert_eq!(took[0].0.source, ThreatSource::Bandit);
        assert_eq!(x.java.applied, 1, "{order:?}");
        let live = x.java.taken();
        assert!(live > 0);
        // The pirates raided again, live, and Java knew the op.
        assert_eq!(
            x.again.raids.len(),
            x.raider.raids.len(),
            "{order:?}: the pirates raid again"
        );
        assert_eq!(
            x.java_again.taken(),
            live,
            "{order:?}: the goods leave the train once"
        );
        assert_eq!(x.java_again.applied, 1, "{order:?}");
        assert_eq!(x.java_again.duplicates, 1, "{order:?}");
        let dropped = u32::from(order == Order::ResultFirst);
        assert_eq!(x.java_again.dropped, dropped, "{order:?}");
        assert_eq!(x.restarted.books.stolen, x.live.books.stolen, "{order:?}");
    }
}

/// The re-decided raid falls inside the replay: deferred, so nothing reaches industry or Java
/// again. The goods left the train once (live) and the pirates' ledger lost the raid (the
/// replay-gate's "lose, never double"), on main as with A.
#[test]
fn a_coast_raid_inside_the_replay_is_deferred() {
    let (s, c) = first_raid();
    let (p, e) = pirates(Falls::InReplay);
    let x = across_a_crash(p, e, TRAIN, s, c, Landed::J1, Order::ResultFirst);
    assert_eq!(x.java.applied, 1);
    assert!(x.again.raids.is_empty(), "{:?}", x.again.raids);
    assert_eq!(x.java_again.taken(), x.java.taken());
    assert_eq!((x.java_again.applied, x.java_again.duplicates), (1, 0));
    assert!(x.again.c.ships.stats.coast_raids < x.raider.c.ships.stats.coast_raids);
}

/// Industry's pages are newer than the pirates' (RP-183): the raid re-decided after the replay
/// meets an engine past the train's cooldown that holds it. Nothing is booked again and nothing
/// reaches Java. Against main's derivation the engine books it again under a new op.
#[test]
fn a_coast_raid_held_by_industry_is_booked_once() {
    let (s, c) = first_raid();
    let (p, e) = pirates(Falls::AfterReplay);
    let (live, after, e) = industry_newer(p, e, TRAIN, s, c, c + 30_000);
    assert_eq!(live.applied, 1);
    assert_eq!(
        after.taken(),
        live.taken(),
        "the goods leave the train once"
    );
    assert_eq!((after.applied, after.duplicates), (1, 0));
    assert_eq!(e.books.stolen.total(), live.taken() as i128);
}
