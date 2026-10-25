// SPDX-License-Identifier: AGPL-3.0-or-later
//! A war band's ambush on a train (`Ev::Ambush`) across a crash (docs/choices-mowzie-build.md
//! RP-180 to RP-186; the harness is rtsim-ecology's `tests/common/train_raid.rs`). A band whose
//! pages are older than its ambush walks its window again in the replay and ambushes the train
//! again, one threat per class, against an industry engine that is not held. Each threat's op is
//! its identity, so Java answers DUPLICATE and the goods leave the real train once.

#[path = "../../rtsim-ecology/tests/common/train_raid.rs"]
mod train_raid;

use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind};
use rtsim_conflict::engine::{Conflict, Target};
use rtsim_conflict::{ConflictTunables, Cultures, synth};
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{GoodVec, TrainId};
use rtsim_industry::threat::{Threat, ThreatSource};
use rtsim_kernel::clock::Tick;
use rtsim_world::World;
use train_raid::{Landed, Order, Raider, Rails, across_a_crash, industry, industry_newer};

const SEED: u64 = 29;
const TRAIN: TrainId = TrainId(9);
/// The band is saved on its way, before any ambush.
const S: Tick = Tick(20);

/// Goblins that never plunder, with only the raid party (cultures.rs `goblins_with`).
fn cultures() -> Cultures {
    let mut d: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
    d.plunder_mean_days = None;
    d.parties.retain(|k, _| k == "raid");
    Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap()
}

/// Every leg a band walks may meet a train (cultures.rs `an_ambush_is_one_threat_per_class`).
fn tun() -> ConflictTunables {
    let mut t = ConflictTunables::default();
    t.ambush_milli = 1000;
    t
}

/// A goblin camp and the world it raids from.
struct Band {
    w: World,
    c: Conflict,
    raids: Vec<(Threat, Option<GoodVec>)>,
}

impl Raider for Band {
    type Saved = (Vec<u8>, Vec<u8>);

    fn save(&self) -> Self::Saved {
        (self.w.save(), self.c.save())
    }

    fn restore(saved: &Self::Saved) -> Band {
        let (goods, ids) = rtsim_world::synth::goods();
        let book = rtsim_world::synth::recipes(&ids);
        Band {
            w: World::load(synth::world_config(SEED), goods, book, &saved.0).expect("world loads"),
            c: Conflict::load(tun(), cultures(), &saved.1).expect("conflict loads"),
            raids: Vec::new(),
        }
    }

    fn run(&mut self, e: &mut Engine, to: Tick) {
        let mut rails = Rails {
            engine: e,
            train: TRAIN,
            raids: &mut self.raids,
        };
        self.c.run_until(&mut self.w, &mut rails, to);
    }

    /// The window again inside the session's replay: main's `Ev::Ambush` has no replay guard.
    fn replay(&mut self, e: &mut Engine, to: Tick) {
        self.c.begin_replay(true);
        self.run(e, to);
        self.c.replay_done();
    }
}

/// A goblin raid ordered at tick 10, two warriors and four archers (cultures.rs
/// `an_ambush_is_one_threat_per_class`), and a train under way the whole time.
fn band() -> (Band, Engine) {
    let (mut w, _) = synth::world(SEED);
    let mut c = Conflict::new(SEED, tun(), cultures(), Tick::ZERO);
    synth::define_names(&mut c, &w);
    c.add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
        .unwrap();
    let p = c
        .order_party(
            &mut w,
            synth::CAMP,
            PartyKind::Raid,
            Target::Pos(synth::FOREST_POS),
            None,
            Tick(10),
        )
        .expect("a raid");
    c.parties[p].units = vec![(0, 2), (1, 4)];
    let e = industry(SEED, TRAIN, Tick(1), 20 * 24_000, 0);
    (
        Band {
            w,
            c,
            raids: Vec::new(),
        },
        e,
    )
}

/// The first ambush's tick: the server dies just after it.
fn first_ambush() -> Tick {
    let (mut b, mut e) = band();
    b.run(&mut e, Tick(3 * 24_000));
    let t = b.raids.first().expect("an ambush").0.at;
    assert!(t > S, "{t:?}");
    t
}

/// The band's pages are older than its ambush and Java applied both threats' loot: the band
/// ambushes the train again in its replay, at industry's later `now`. The goods leave the real
/// train once, in either order of the journal's carriage results. Against main's derivation the
/// re-decided threats are new ops and Java takes the goods twice.
#[test]
fn a_bands_ambush_replayed_after_a_crash_takes_the_goods_once() {
    let c = first_ambush() + 1;
    for order in [Order::ResultFirst, Order::RaidFirst] {
        let (b, e) = band();
        let x = across_a_crash(b, e, TRAIN, S, c, Landed::J1, order);
        // The live control: one ambush, a threat per class, each with its own op.
        assert_eq!(x.raider.c.stats.ambushes, 1, "{order:?}");
        assert_eq!(x.raider.raids.len(), 2, "{order:?}");
        assert!(
            x.raider
                .raids
                .iter()
                .all(|(t, l)| t.source == ThreatSource::Bandit && l.is_some())
        );
        assert_eq!(x.java.applied, 2, "{order:?}");
        let live = x.java.taken();
        assert!(live > 0);
        // The replay ambushed again; Java knew both ops.
        assert_eq!(
            x.again.c.stats.ambushes, 1,
            "{order:?}: the band ambushes again"
        );
        assert_eq!(
            x.java_again.taken(),
            live,
            "{order:?}: the goods leave the train once"
        );
        assert_eq!(x.java_again.applied, 2, "{order:?}");
        assert_eq!(x.java_again.duplicates, 2, "{order:?}");
        let dropped = if order == Order::ResultFirst { 2 } else { 0 };
        assert_eq!(x.java_again.dropped, dropped, "{order:?}");
        assert_eq!(x.restarted.books.stolen, x.live.books.stolen, "{order:?}");
    }
}

/// Industry's pages are newer than the band's (RP-183): the band's replay meets an engine past
/// the train's cooldown that holds the ambush. Nothing is booked again and nothing reaches Java,
/// and the band is credited nothing (its pages lost the ambush: a loss to its ledger, never a
/// second theft). Against main's derivation the engine books it again under new ops.
#[test]
fn a_bands_ambush_held_by_industry_is_booked_once() {
    let c = first_ambush() + 1;
    let (b, e) = band();
    let (live, after, e) = industry_newer(b, e, TRAIN, S, c, c + 30_000);
    assert_eq!(live.applied, 2);
    assert_eq!(
        after.taken(),
        live.taken(),
        "the goods leave the train once"
    );
    assert_eq!((after.applied, after.duplicates), (2, 0));
    assert_eq!(e.books.stolen.total(), live.taken() as i128);
}
