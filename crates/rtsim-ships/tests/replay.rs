// SPDX-License-Identifier: AGPL-3.0-or-later
//! The session's replay on the ships (docs/replay-gate.md §4.7, §5, §6, §9.5; RP-90 to RP-99): a
//! crash restores the pages of the last save, and the journal's records of the session that went
//! on are applied again. The fake plugin plays the session that went on; the test crashes it and
//! replays what it journaled into an engine read back from the pages (`begin_replay(true)`), or
//! into a fresh one (`begin_replay(false)`).
//!
//! Every test has its control: the same records into the engine that never crashed, or the same
//! engine without a replay.

use std::collections::BTreeMap;

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_ships::boundary::{self, By, Cmd, Obs, Role};
use rtsim_ships::engine::Ship;
use rtsim_ships::synth::{self, Coastal, FakeShips};
use rtsim_ships::water::AllWater;
use rtsim_ships::{ShipId, ShipTunables, Ships};
use rtsim_trade::MILLI;
use rtsim_travel::Medium;
use rtsim_world::{NodeData, World};

const PLAYER: (u64, u64) = (0xAB, 0xCD);
const STEP: u64 = 50;

fn coast(seed: u64) -> Coastal {
    synth::coastal(seed, ShipTunables::default())
}

/// A coast, a fake plugin and a player waiting at dock 0 (merchants arrive in view, stay docked
/// and leave again), stepped as the host does: the player reports every 1,000 ticks, the engine
/// runs, its commands go to the plugin, and what the plugin reports comes back. Everything sent
/// and everything applied is kept.
struct Scene {
    c: Coastal,
    fake: FakeShips,
    dock: (i32, i32),
    /// Every observation applied, with the tick it was applied at.
    journal: Vec<(Tick, Obs)>,
    /// Every command the engine sent.
    sent: Vec<Cmd>,
}

impl Scene {
    fn new(seed: u64) -> Scene {
        Scene::on(coast(seed))
    }

    fn on(c: Coastal) -> Scene {
        let mut fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
        let dock = (c.ships.docks[0].x, c.ships.docks[0].z);
        fake.set_player(PLAYER, f64::from(dock.0), f64::from(dock.1));
        Scene {
            c,
            fake,
            dock,
            journal: Vec::new(),
            sent: Vec::new(),
        }
    }

    /// The world and the ships read back from their pages, as a restarted engine has them, on a
    /// fresh plugin (the bodies of the last session are gone).
    fn restored(world: &[u8], ships: &[u8], seed: u64) -> Scene {
        let (c, _) = restore(world, ships, seed);
        Scene::on(c)
    }

    /// Steps to `to` or until `stop` holds after a step. Whether it did.
    fn pump_until(&mut self, to: Tick, mut stop: impl FnMut(&Scene) -> bool) -> bool {
        let mut t = self.c.ships.now();
        while t < to {
            if t.0.is_multiple_of(1_000) {
                let pos = Pos::new(0, self.dock.0, self.dock.1);
                self.c
                    .ships
                    .observe_player(&mut self.c.world, PLAYER, pos, t);
            }
            t = (t + STEP).min(to);
            self.c
                .ships
                .run_until(&mut self.c.world, &mut NoInfrastructure, t);
            let cmds = self.c.ships.take_commands();
            let mut obs = self.fake.apply(&cmds, t);
            self.sent.extend(cmds);
            obs.extend(self.fake.step(t));
            for o in obs {
                self.c.ships.ingest(&mut self.c.world, &o, &self.c.mask, t);
                self.journal.push((t, o));
            }
            if stop(self) {
                return true;
            }
        }
        false
    }

    /// An observation the plugin reports now, applied and journaled.
    fn report(&mut self, o: Obs) {
        let t = self.c.ships.now();
        self.c.ships.ingest(&mut self.c.world, &o, &self.c.mask, t);
        self.journal.push((t, o));
    }
}

/// An engine read back from its pages (the world and the ships), on the synthetic coast.
fn restore(world: &[u8], ships: &[u8], seed: u64) -> (Coastal, ()) {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    let c = Coastal {
        world: World::load(synth::world_config(seed), goods, book, world).expect("world loads"),
        ships: Ships::load(ShipTunables::default(), ships).expect("ships load"),
        mask: synth::coast(),
        haven: 0,
        navy: None,
    };
    (c, ())
}

/// The ship's crew, hull and epoch.
fn row(s: &Ships, agent: u64) -> (u32, u32, u32) {
    let x = s.ship(Ship::id_of_agent(agent).unwrap()).unwrap();
    (x.crew, x.hull_milli, x.epoch)
}

fn id_of(agent: u64) -> ShipId {
    Ship::id_of_agent(agent).unwrap()
}

/// Applies the journal to `c` the way the host does while replaying: the engine runs to each
/// record's tick, then the record is applied (a copy through `ingest_copy`). Returns the commands
/// the engine queued meanwhile.
fn replay(c: &mut Coastal, journal: &[(Tick, Obs)], copy: bool) -> Vec<Cmd> {
    let mut out = Vec::new();
    for (t, o) in journal {
        let at = (*t).max(c.ships.now()).max(c.world.now());
        c.ships.run_until(&mut c.world, &mut NoInfrastructure, at);
        out.extend(c.ships.take_commands());
        if copy {
            c.ships.ingest_copy(&mut c.world, o, &c.mask, at);
        } else {
            c.ships.ingest(&mut c.world, o, &c.mask, at);
        }
        out.extend(c.ships.take_commands());
    }
    out
}

/// The session that went on after a save: a crewed merchant materialized in view and a player
/// damaged it (one sailor lost, hull 800).
struct Crash {
    seed: u64,
    world: Vec<u8>,
    ships: Vec<u8>,
    /// What the plugin reported after the save, in order.
    journal: Vec<(Tick, Obs)>,
    m: boundary::Materialize,
    /// The ship's crew, hull and epoch in the session that went on (no crash).
    live: (u32, u32, u32),
    /// Its next epoch there.
    live_next: u32,
    /// Where the pages stood.
    saved_row: (u32, u32, u32),
}

/// Plays a session on seed `seed` with a save after one day, then a crewed merchant's
/// materialization and damage. With `to_demat` it goes on until the plugin reports the body
/// dematerialized (its epoch ended in the journaled window). `None` when no such merchant came
/// into view.
fn crash_scene(seed: u64, to_demat: bool) -> Option<Crash> {
    let mut sc = Scene::new(seed);
    sc.pump_until(Tick::from_days(1), |_| false);
    let (world, ships) = (sc.c.world.save(), sc.c.ships.save());
    sc.journal.clear();
    sc.sent.clear();
    let found = |s: &Scene| {
        s.sent.iter().find_map(|c| match c {
            Cmd::Materialize(m) if m.role == Role::Merchant && m.crew >= 2 => Some(m.clone()),
            _ => None,
        })
    };
    if !sc.pump_until(Tick::from_days(7), |s| found(s).is_some()) {
        return None;
    }
    let m = found(&sc).expect("found");
    let saved_row = {
        let r = restore(&world, &ships, seed).0;
        row(&r.ships, m.agent)
    };
    // A player damages it: the plugin's body and the engine's record agree.
    let b = sc.fake.bodies.get_mut(&m.agent).expect("in view");
    b.crew -= 1;
    b.hull_milli = 800;
    sc.report(Obs::Damaged(boundary::Damaged {
        agent: m.agent,
        epoch: m.epoch,
        by: By::Player,
        hull_milli: 800,
        crew_lost: 1,
        player: PLAYER,
        ship: 0,
    }));
    if to_demat {
        let ended = |s: &Scene| {
            s.journal.iter().any(
                |(_, o)| matches!(o, Obs::Dematerialized(d) if d.agent == m.agent && d.epoch == m.epoch),
            )
        };
        if !sc.pump_until(Tick::from_days(14), ended) {
            return None;
        }
    }
    Some(Crash {
        seed,
        world,
        ships,
        journal: sc.journal.clone(),
        live: row(&sc.c.ships, m.agent),
        live_next: sc.c.ships.next_epoch(id_of(m.agent)),
        saved_row,
        m,
    })
}

/// The first seed of a few whose scene is judged.
fn judged(to_demat: bool) -> Crash {
    (1..=6)
        .find_map(|seed| crash_scene(seed, to_demat))
        .expect("a crewed merchant came into view in one of six seeds")
}

/// What a session that went on leaves, and what a crash replays: scratch :2625's audit on the
/// engine. A merchant materialized after the save was damaged. Without the crash it has lost a
/// sailor and its hull; after the crash and the replay the pages' ship, which never saw the
/// materialization, must have both too (it dropped them: crew 4, hull 1000, epoch 0 on the base,
/// docs/replay-gate.md §9.5).
#[test]
fn damage_to_a_ship_seen_after_a_save_is_counted_after_a_crash() {
    let cr = judged(false);
    let m = &cr.m;
    // The control: the session that went on counted it.
    assert_eq!(
        cr.live,
        (m.crew - 1, 800, m.epoch),
        "seed {}: the damage in the session that went on",
        cr.seed
    );
    // The pages predate the materialization: the same ship, whole, with an older epoch.
    assert!(cr.saved_row.2 < m.epoch, "{:?}", cr.saved_row);
    assert_eq!((cr.saved_row.0, cr.saved_row.1), (m.crew, 1000));

    // The crash: pages read back, the journal replayed.
    let mut r = Scene::restored(&cr.world, &cr.ships, cr.seed);
    r.c.ships.begin_replay(true);
    let sent = replay(&mut r.c, &cr.journal, false);
    r.c.ships.replay_done(&mut r.c.world);
    assert_eq!(
        row(&r.c.ships, m.agent),
        cr.live,
        "seed {}: after the replay as in the session that went on",
        cr.seed
    );
    assert!(sent.is_empty(), "nothing is sent while replaying: {sent:?}");
    let counts = r.c.ships.replay_gate_counts();
    assert_eq!(counts.never, 0, "{counts:?}");
    r.c.ships.check(&r.c.world).expect("conserved");

    // The second control: an engine built fresh keeps no body of the last session (RP-31). It
    // loses the damage but not the epoch, which its floor holds.
    let mut f = Scene::new(cr.seed);
    f.c.ships.begin_replay(false);
    replay(&mut f.c, &cr.journal, false);
    f.c.ships.replay_done(&mut f.c.world);
    let (crew, hull, epoch) = row(&f.c.ships, m.agent);
    assert_eq!(
        (hull, epoch),
        (1000, m.epoch),
        "seed {}: a fresh engine has no body to damage, but its floor holds the epoch",
        cr.seed
    );
    assert!(crew >= m.crew - 1);
}

/// The epoch test (§5.2, §5.3): a replayed SHIP_MATERIALIZED makes the ship's epoch the body's
/// and the replay end leaves it abstract; nothing was sent while replaying and no "never" kind
/// reached the gate. Control: the engine that never crashed holds the same epoch.
#[test]
fn a_replayed_materialization_sets_the_epoch_and_sends_nothing() {
    let cr = judged(false);
    let m = &cr.m;
    let mut r = Scene::restored(&cr.world, &cr.ships, cr.seed);
    assert!(r.c.ships.ship(id_of(m.agent)).unwrap().epoch < m.epoch);
    r.c.ships.begin_replay(true);
    assert!(r.c.ships.replaying());
    let sent = replay(&mut r.c, &cr.journal, false);
    assert!(sent.is_empty(), "{sent:?}");
    // The honour: during the replay the body is the ship's, with its epoch.
    let x = r.c.ships.ship(id_of(m.agent)).unwrap();
    assert!(x.embodied, "the replayed body is held until the replay end");
    assert_eq!(x.epoch, m.epoch);
    r.c.ships.replay_done(&mut r.c.world);
    assert!(!r.c.ships.replaying());
    let x = r.c.ships.ship(id_of(m.agent)).unwrap();
    assert!(!x.embodied, "every body is gone at the replay end");
    assert_eq!(x.epoch, m.epoch, "and the epoch stays");
    assert_eq!(r.c.ships.next_epoch(id_of(m.agent)), cr.live_next);
    let c = r.c.ships.replay_gate_counts();
    assert_eq!(c.never, 0, "{c:?}");
    // Nothing is queued at the end either.
    assert!(r.c.ships.take_commands().is_empty());
}

/// An ending epoch is never reused (§5.3, RP-7, common rule 5): a body whose epoch ended in the
/// replayed window (the plugin reported it dematerialized), then the player stays and the same
/// agent materializes again after the replay end: its epoch is above every epoch the journal
/// named. Controls: the session that went on gets the same next epoch, and a fresh engine holds
/// it with its floor.
#[test]
fn an_ending_epoch_is_never_reused() {
    let cr = judged(true);
    let m = &cr.m;
    let ended = cr.journal.iter().any(
        |(_, o)| matches!(o, Obs::Dematerialized(d) if d.agent == m.agent && d.epoch == m.epoch),
    );
    assert!(ended, "the window ends the body's epoch");
    let mut named: BTreeMap<u64, u32> = BTreeMap::new();
    for (_, o) in &cr.journal {
        if let Some((agent, epoch)) = o.ship_epoch() {
            let e = named.entry(agent).or_default();
            *e = (*e).max(epoch);
        }
    }
    assert_eq!(named[&m.agent], m.epoch);
    assert_eq!(cr.live_next, m.epoch + 1, "the session that went on");

    for from_pages in [true, false] {
        let mut r = if from_pages {
            Scene::restored(&cr.world, &cr.ships, cr.seed)
        } else {
            Scene::new(cr.seed)
        };
        r.c.ships.begin_replay(from_pages);
        let sent = replay(&mut r.c, &cr.journal, false);
        assert!(sent.is_empty());
        r.c.ships.replay_done(&mut r.c.world);
        assert_eq!(
            r.c.ships.next_epoch(id_of(m.agent)),
            m.epoch + 1,
            "from pages {from_pages}"
        );
        // The player is still there: ships come into view again.
        let mut r = Scene::on(r.c);
        let to = r.c.ships.now() + 8 * TICKS_PER_DAY;
        r.pump_until(to, |_| false);
        let mut materializations = 0;
        for c in &r.sent {
            let Cmd::Materialize(n) = c else { continue };
            if let Some(&top) = named.get(&n.agent) {
                assert!(
                    n.epoch > top,
                    "from pages {from_pages}: agent {:x} materialized at epoch {} after the journal named {top}",
                    n.agent,
                    n.epoch
                );
                materializations += 1;
            }
        }
        assert!(
            materializations > 0,
            "from pages {from_pages}: a ship of the journal came into view again"
        );
    }
}

/// One record of every kind that carries an epoch (RP-57's table for the ships), naming
/// `agent`'s epoch `epoch`.
fn epoch_kinds(agent: u64, epoch: u32) -> Vec<Obs> {
    let pos = (0.0, 64.0, 0.0);
    vec![
        Obs::State(boundary::State {
            agent,
            epoch,
            flags: 0,
            dimension: 0,
            hull_milli: 1000,
            crew: 3,
            pos,
        }),
        Obs::Materialized(boundary::Materialized {
            agent,
            epoch,
            dimension: 0,
            entity: (1, 2),
        }),
        Obs::SpawnRejected(boundary::SpawnRejected {
            agent,
            epoch,
            reason: 9,
            cmd_seq: 1,
        }),
        Obs::Dematerialized(boundary::Dematerialized {
            agent,
            epoch,
            reason: 3,
            dimension: 0,
            hull_milli: 1000,
            crew: 3,
            pos,
        }),
        Obs::Damaged(boundary::Damaged {
            agent,
            epoch,
            by: By::Player,
            hull_milli: 900,
            crew_lost: 0,
            player: PLAYER,
            ship: 0,
        }),
        Obs::Sunk(boundary::Sunk {
            agent,
            epoch,
            by: By::Player,
            dimension: 0,
            player: PLAYER,
            ship: 0,
            op: 1 << 63 | 1,
            pos,
        }),
        Obs::Boarded(boundary::Boarded {
            agent,
            epoch,
            by: By::Player,
            player: PLAYER,
            ship: 0,
        }),
        Obs::Captured(boundary::Captured {
            agent,
            epoch,
            by: By::Ship,
            crew_taken: 0,
            player: (0, 0),
            ship: 8,
            op: 1 << 63 | 2,
        }),
        Obs::CargoLooted(boundary::CargoLooted {
            agent,
            epoch,
            by: By::Player,
            player: PLAYER,
            ship: 0,
            op: 1 << 63 | 3,
            items: Vec::new(),
        }),
    ]
}

/// RP-57's floor for the ships: every observation kind that names an epoch raises the agent's
/// floor, whether it is replayed into a fresh engine, replayed into one read back from pages, or
/// a live copy (REASSERT): the next epoch is above the named one, whatever the record does after.
/// Control: the same record applied live raises no floor (a live record names an epoch the
/// ship holds). A kind that names no body (the fight, a port, a haven) raises none either.
#[test]
fn every_kind_that_names_an_epoch_raises_the_floor() {
    let seed = 3;
    let base = coast(seed);
    let id = base.ships.service_ships(0).next().expect("a ship");
    let agent = Ship::agent(id);
    let kinds = epoch_kinds(agent, 5);
    assert_eq!(
        kinds.iter().filter(|o| o.ship_epoch().is_some()).count(),
        kinds.len()
    );
    for o in &kinds {
        let name = format!("{o:?}");
        let name = name.split('(').next().unwrap().to_string();
        // Replayed into a fresh engine.
        let mut c = coast(seed);
        c.ships.begin_replay(false);
        let t = c.ships.now();
        c.ships.ingest(&mut c.world, o, &c.mask, t);
        assert_eq!(c.ships.next_epoch(id), 6, "{name}: fresh, replayed");
        c.ships.replay_done(&mut c.world);
        assert_eq!(c.ships.next_epoch(id), 6, "{name}: fresh, after the replay");
        // Replayed into an engine read back from its pages.
        let mut c = coast(seed);
        c.ships = Ships::load(ShipTunables::default(), &c.ships.save()).unwrap();
        c.ships.begin_replay(true);
        let t = c.ships.now();
        c.ships.ingest(&mut c.world, o, &c.mask, t);
        assert_eq!(c.ships.next_epoch(id), 6, "{name}: pages, replayed");
        c.ships.replay_done(&mut c.world);
        assert_eq!(c.ships.next_epoch(id), 6, "{name}: pages, after the replay");
        // A live copy.
        let mut c = coast(seed);
        let t = c.ships.now();
        c.ships.ingest_copy(&mut c.world, o, &c.mask, t);
        assert_eq!(c.ships.next_epoch(id), 6, "{name}: live copy");
        // The control: the same record live.
        let mut c = coast(seed);
        let t = c.ships.now();
        c.ships.ingest(&mut c.world, o, &c.mask, t);
        assert_eq!(
            c.ships.next_epoch(id),
            1,
            "{name}: a live record raises no floor"
        );
    }
    // The kinds without a body raise none, replayed or copied.
    let mut c = coast(seed);
    c.ships.begin_replay(false);
    let t = c.ships.now();
    for o in [
        Obs::Fight(boundary::Fight {
            attacker: agent,
            defender: agent,
            outcome: 1,
            attacker_crew: (3, 2),
            defender_crew: (3, 2),
            attacker_hull_milli: 900,
            defender_hull_milli: 900,
        }),
        Obs::PortFound(boundary::PortFound {
            village: 0x1234,
            medium: rtsim_travel::Medium::Sea,
            dimension: 0,
            structure: 0,
            dock: (0, 63, 0),
            berths: 1,
        }),
        Obs::HavenFound(boundary::HavenFound {
            haven: 0x99,
            media: 1,
            dimension: 0,
            structure: 0,
            dock: (0, 63, 0),
        }),
    ] {
        c.ships.ingest_copy(&mut c.world, &o, &c.mask, t);
    }
    assert_eq!(c.ships.next_epoch(id), 1);
}

/// A copy embodies nothing (§5.7, RP-97): a copy of a SHIP_MATERIALIZED while replaying raises the
/// floor and no more; the same record as a replayed one is honoured. Control: that one.
#[test]
fn a_copy_of_a_materialization_embodies_nothing() {
    let cr = judged(false);
    let m = &cr.m;
    let mat = cr
        .journal
        .iter()
        .find(
            |(_, o)| matches!(o, Obs::Materialized(r) if r.agent == m.agent && r.epoch == m.epoch),
        )
        .cloned()
        .expect("journaled");
    let before = cr.saved_row.2;
    for copy in [true, false] {
        let mut r = Scene::restored(&cr.world, &cr.ships, cr.seed);
        r.c.ships.begin_replay(true);
        let at = mat.0;
        r.c.ships
            .run_until(&mut r.c.world, &mut NoInfrastructure, at);
        if copy {
            r.c.ships.ingest_copy(&mut r.c.world, &mat.1, &r.c.mask, at);
        } else {
            r.c.ships.ingest(&mut r.c.world, &mat.1, &r.c.mask, at);
        }
        let x = r.c.ships.ship(id_of(m.agent)).unwrap();
        assert_eq!(x.embodied, !copy, "copy {copy}");
        assert_eq!(x.epoch, if copy { before } else { m.epoch }, "copy {copy}");
        assert_eq!(
            r.c.ships.next_epoch(id_of(m.agent)),
            m.epoch + 1,
            "copy {copy}"
        );
        r.c.ships.replay_done(&mut r.c.world);
        assert_eq!(row(&r.c.ships, m.agent).2, m.epoch, "copy {copy}: after");
    }
}

/// Nothing embodies while replaying (§6, RP-91): a player stays at the dock for days under the
/// replay and no ship is materialized; the early returns are counted and no "never" kind reached
/// the gate. After the replay end the same player sees ships come in. Control: the same days
/// without a replay materialize ships.
#[test]
fn no_ship_embodies_while_the_replay_runs() {
    let seed = 3;
    let mut s = Scene::new(seed);
    s.pump_until(Tick::from_days(1), |_| false);
    let (world, ships) = (s.c.world.save(), s.c.ships.save());
    let days = 4 * TICKS_PER_DAY;

    // The control: no replay.
    let mut ctl = Scene::restored(&world, &ships, seed);
    let to = ctl.c.ships.now() + days;
    ctl.pump_until(to, |_| false);
    let m0 = ctl
        .sent
        .iter()
        .filter(|c| matches!(c, Cmd::Materialize(_)))
        .count();
    assert!(m0 > 0, "ships come into view of a player at a dock");
    assert_eq!(ctl.c.ships.replay_gate_counts(), boundary_counts_default());

    // The replay.
    let mut r = Scene::restored(&world, &ships, seed);
    let before = r.c.ships.stats.materialized;
    r.c.ships.begin_replay(true);
    r.pump_until(to, |_| false);
    assert!(
        r.sent.is_empty(),
        "no command is sent while replaying: {:?}",
        r.sent.iter().map(Cmd::name).collect::<Vec<_>>()
    );
    let c = r.c.ships.replay_gate_counts();
    assert_eq!(c.never, 0, "{c:?}");
    assert!(c.deferred > 0, "the sightings returned early: {c:?}");
    assert_eq!(r.c.ships.stats.materialized, before);
    r.c.ships.check(&r.c.world).expect("conserved");

    // The replay ends; the player is still there; ships come in again.
    r.c.ships.replay_done(&mut r.c.world);
    let mut r = Scene::on(r.c);
    let to = r.c.ships.now() + days;
    r.pump_until(to, |_| false);
    assert!(
        r.sent.iter().any(|c| matches!(c, Cmd::Materialize(_))),
        "after the replay end ships embody again"
    );
    r.c.ships.check(&r.c.world).expect("conserved");
}

fn boundary_counts_default() -> rtsim_ships::GateCounts {
    rtsim_ships::GateCounts::default()
}

/// A kept ship's NextLeg events while replaying drop their commands and keep the voyage's times
/// (§9.5): a ship embodied at the save is still believed embodied until the replay end, sails its
/// legs, and its timetable and hold commands are dropped by the gate (counted); the same days
/// without a replay send them. The NextLeg chain is the same in both.
#[test]
fn a_kept_ships_next_leg_while_replaying_drops_its_commands() {
    let seed = 3;
    let mut s = Scene::new(seed);
    let embodied = |s: &Scene| s.c.ships.ships.iter().any(|(_, x)| x.embodied);
    assert!(
        s.pump_until(Tick::from_days(6), embodied),
        "a ship was embodied"
    );
    let (world, ships) = (s.c.world.save(), s.c.ships.save());
    let to = s.c.ships.now() + TICKS_PER_DAY;

    // The control: the engine goes on believing the ship embodied (no plugin to say otherwise),
    // and sends its legs. Only the ships embodied at the save count: the control's player also
    // sees new ships come in, which the replay defers.
    let kept: Vec<u64> = {
        let c = restore(&world, &ships, seed).0;
        c.ships
            .ships
            .iter()
            .filter(|(_, x)| x.embodied)
            .map(|(id, _)| Ship::agent(id))
            .collect()
    };
    assert!(!kept.is_empty());
    let mut ctl = restore(&world, &ships, seed).0;
    let mut sent = Vec::new();
    let mut t = ctl.ships.now();
    while t < to {
        t = (t + STEP).min(to);
        ctl.ships
            .run_until(&mut ctl.world, &mut NoInfrastructure, t);
        sent.extend(ctl.ships.take_commands());
    }
    let legs = sent
        .iter()
        .filter(|c| match c {
            Cmd::Timetable(t) => kept.contains(&t.agent),
            Cmd::SetCargo(s) => kept.contains(&s.agent),
            Cmd::Depart(d) => kept.contains(&d.agent),
            _ => false,
        })
        .count();
    assert!(legs > 0, "a kept ship sends its next leg: {}", sent.len());

    // The replay.
    let mut r = restore(&world, &ships, seed).0;
    r.ships.begin_replay(true);
    let mut sent2 = Vec::new();
    let mut t = r.ships.now();
    while t < to {
        t = (t + STEP).min(to);
        r.ships.run_until(&mut r.world, &mut NoInfrastructure, t);
        sent2.extend(r.ships.take_commands());
    }
    assert!(
        sent2.is_empty(),
        "{:?}",
        sent2.iter().map(Cmd::name).collect::<Vec<_>>()
    );
    let c = r.ships.replay_gate_counts();
    assert_eq!(c.never, 0);
    assert_eq!(
        c.dropped, legs as u64,
        "the kept ships' legs were dropped and counted: {c:?}"
    );
    assert_eq!(
        r.ships.pending_by_kind().get("next_leg").copied(),
        Some(kept.len()),
        "the voyage goes on: one NextLeg due for each kept ship"
    );
    r.ships.check(&r.world).expect("conserved");
    r.ships.replay_done(&mut r.world);
    r.ships.check(&r.world).expect("conserved after the end");
}

/// A command queued in the pages is a body of an earlier session's (§4.7, RP-90): the replay's
/// start drops it. Control: the engine read back without a replay still holds it.
#[test]
fn a_command_queued_in_the_pages_is_dropped_at_the_replay_start() {
    let seed = 3;
    let mut s = Scene::new(seed);
    let dock = s.dock;
    // Run with a player and leave the commands untaken.
    let mut t = Tick::ZERO;
    let mut queued = 0;
    while t < Tick::from_days(6) && queued == 0 {
        if t.0.is_multiple_of(1_000) {
            s.c.ships
                .observe_player(&mut s.c.world, PLAYER, Pos::new(0, dock.0, dock.1), t);
        }
        t += STEP;
        s.c.ships
            .run_until(&mut s.c.world, &mut NoInfrastructure, t);
        queued = s.c.ships.commands_queued();
    }
    assert!(queued > 0, "the engine queued commands");
    let (world, ships) = (s.c.world.save(), s.c.ships.save());

    let mut ctl = restore(&world, &ships, seed).0;
    assert_eq!(
        ctl.ships.take_commands().len(),
        queued,
        "the pages hold them"
    );

    let mut r = restore(&world, &ships, seed).0;
    r.ships.begin_replay(true);
    assert!(r.ships.take_commands().is_empty(), "dropped at the start");
    r.ships.replay_done(&mut r.world);
    assert!(r.ships.take_commands().is_empty());
}

/// §6.1: a pirate hunt's coast raid books on industry's state and the port's watcher, which a
/// replay has not settled, so a hunt begun while replaying books none and counts the deferral
/// (RP-95); the hunt goes on. Control: the same sea without a replay raids. After the replay end
/// the raids come back.
#[test]
fn a_coast_raid_decided_while_replaying_is_deferred() {
    let tun = ShipTunables::default();
    let days = 120;
    let run = |seed: u64, replaying: bool| {
        let mut s = synth::sea_havens(seed, tun.clone());
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        if replaying {
            s.ships.begin_replay(true);
        }
        synth::drive(
            &mut s.world,
            &mut s.ships,
            &mut fake,
            &mut NoInfrastructure,
            &s.mask,
            Tick::from_days(days),
            TICKS_PER_DAY / 4,
        );
        s.ships.check(&s.world).expect("conserved");
        s
    };
    let mut judged = 0;
    for seed in 1..=6 {
        let ctl = run(seed, false);
        if ctl.ships.stats.port_raids + ctl.ships.stats.coast_raids == 0 {
            continue;
        }
        let r = run(seed, true);
        let c = r.ships.replay_gate_counts();
        assert_eq!(
            (r.ships.stats.port_raids, r.ships.stats.coast_raids),
            (0, 0),
            "seed {seed}: no raid is booked while replaying"
        );
        assert!(
            c.deferred > 0,
            "seed {seed}: the deferrals are counted: {c:?}"
        );
        assert_eq!(c.never, 0);
        assert!(r.ships.stats.hunts > 0, "seed {seed}: the hunts go on");
        judged += 1;
    }
    assert!(judged > 0, "a sea raided in the control");
}

/// Four ports and a haven with one pirate in open water, the network not built: a world whose
/// save came before its first network (the pages hold the docks, the haven and its pirate).
fn unbuilt(seed: u64) -> (World, Ships, AllWater) {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    let mut data = Vec::new();
    for i in 0..4i32 {
        data.push(NodeData {
            key: 1_000 + i as u64,
            kind: NodeKind::Village(Culture::Human),
            pos: Pos::new(0, (i % 2) * 1_200 + 37 * i, (i / 2) * 1_200 + 53 * i),
            pop: 12,
            resources: [
                (rtsim_world::synth::FARMLAND, 40 * MILLI),
                (rtsim_world::synth::CATTLE, 4 * MILLI),
            ]
            .into_iter()
            .collect(),
        });
    }
    data.push(NodeData {
        key: 9_999,
        kind: NodeKind::Haven(Culture::Human),
        pos: Pos::new(0, 600, 600),
        pop: 4,
        resources: Default::default(),
    });
    let mut w = World::build(synth::world_config(seed), goods, book, data.clone());
    let mask = AllWater(32);
    let mut s = Ships::new(seed, ShipTunables::default(), Tick::ZERO);
    for (i, d) in data.iter().enumerate().take(4) {
        s.add_port(
            &mut w,
            i,
            Medium::Sea,
            (d.pos.x, 63, d.pos.z),
            4,
            &mask,
            Tick::ZERO,
        );
    }
    s.add_haven(&mut w, 4, 1, (600, 63, 600), 1, Tick::ZERO);
    (w, s, mask)
}

/// The honour lands on the live run's body only (RP-93): ships are allocated in order and never
/// reused, so one the replay allocated (a network built in the replayed window) need not be the
/// ship the live run gave that index. A SHIP_MATERIALIZED for it raises the floor and no more; the
/// floor reaches the ship at the replay end and its first body is above it. Control: the same
/// record for a ship the pages held (the haven's pirate) is honoured.
#[test]
fn the_honour_lands_on_a_ship_the_pages_held_not_one_the_replay_allocated() {
    let (mut w, s, mask) = unbuilt(5);
    let pirate = s.havens[0].ships[0];
    let mut s = Ships::load(ShipTunables::default(), &s.save()).expect("loads");
    s.begin_replay(true);
    // The network is built inside the replayed window (the host's first simulate point).
    s.build(&mut w, &mask, Tick::ZERO);
    let service_ship = s.service_ships(0).next().expect("a service's ship");
    assert!(s.ship(service_ship).is_some());
    let named = |id: ShipId, epoch: u32| {
        Obs::Materialized(boundary::Materialized {
            agent: Ship::agent(id),
            epoch,
            dimension: 0,
            entity: (1, 2),
        })
    };
    s.ingest(&mut w, &named(service_ship, 3), &mask, Tick::ZERO);
    let x = s.ship(service_ship).unwrap();
    assert!(!x.embodied, "not honoured: the replay allocated it");
    assert_eq!(x.epoch, 0);
    assert_eq!(s.next_epoch(service_ship), 4, "the floor holds the epoch");
    // The control: the pirate the pages held takes its body.
    s.ingest(&mut w, &named(pirate, 3), &mask, Tick::ZERO);
    let x = s.ship(pirate).unwrap();
    assert!(x.embodied && x.epoch == 3, "honoured: {x:?}");
    s.replay_done(&mut w);
    assert_eq!(s.ship(service_ship).unwrap().epoch, 3, "the floor folds");
    assert!(!s.ship(service_ship).unwrap().embodied);
    assert_eq!(s.next_epoch(service_ship), 4);
    assert_eq!(s.next_epoch(pirate), 4);
}

/// A record naming an epoch at the top of the range cannot make the next body epoch 0, the
/// plugin's "no body" (a corrupt record: the increment wraps and is held at 1).
#[test]
fn a_corrupt_epoch_does_not_wrap_the_next_epoch_to_zero() {
    let mut c = coast(3);
    let id = c.ships.service_ships(0).next().expect("a ship");
    c.ships.begin_replay(false);
    let o = Obs::Damaged(boundary::Damaged {
        agent: Ship::agent(id),
        epoch: u32::MAX,
        by: By::Player,
        hull_milli: 900,
        crew_lost: 0,
        player: PLAYER,
        ship: 0,
    });
    let t = c.ships.now();
    c.ships.ingest(&mut c.world, &o, &c.mask, t);
    assert_eq!(c.ships.next_epoch(id), 1, "held at 1, not 0");
}
