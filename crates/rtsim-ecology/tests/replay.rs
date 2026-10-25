// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragons' replay gate (docs/replay-gate.md §9.2, RP-70 to RP-79): what a session's replay
//! of the last session's records may and may not do to an engine restored from its pages.
//!
//! - nothing embodies while the replay runs, and the books of what it would have embodied stand
//!   (§6, §6.1): a long replay with a player at the lair, a raid on a watched district, a watched
//!   tank;
//! - bodies stay embodied through the replay, so the records of the last session's bodies count
//!   against them (§5.1): a death after the save is a death;
//! - a replayed DRAGON_MATERIALIZED embodies the dragon as the last session had it (§5.2), and no
//!   epoch is used twice (§5.3, RP-57): the table of every observation kind;
//! - a DEPART answered NOT_EMBODIED takes no health, position or hunger (§5.8);
//! - a dragon adopted in the replayed window is adopted again by its live report (RP-13).

use rtsim_ecology::boundary::{self, AttackerKind, Cmd, Obs, SpawnKind};
use rtsim_ecology::dragon::{DragonId, LairKind, State, agent_of};
use rtsim_ecology::engine::{Dragons, NewDragon};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::species::{Gender, Species};
use rtsim_ecology::synth;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};

const SEED: u64 = 5;
const PLAYER: (u64, u64) = (5, 6);
const EGG: (u64, u64) = (0xE66, 0x2);

// ---- the session's replay, as the host drives it --------------------------------------------------

/// A session's replay begins (the host's SESSION_START): `from_pages` for an engine read back from
/// its pages.
fn begin(d: &mut Dragons, w: &mut World, at: Tick, from_pages: bool) {
    let _ = (w, at);
    d.begin_replay(from_pages);
}

/// The replay has caught up (the host's replay end).
fn end(d: &mut Dragons, w: &mut World, at: Tick) {
    d.replay_done(w, at);
}

/// What the gate has counted: (dropped, never, deferred).
fn gate(d: &Dragons) -> (u64, u64, u64) {
    let c = d.replay_gate_counts();
    (c.dropped, c.never, c.deferred)
}

// ---- fixtures -------------------------------------------------------------------------------------

fn lair_pos() -> Pos {
    Pos::new(0, 4000, 4000)
}

fn adult(name: &str, lair: u32) -> NewDragon {
    NewDragon {
        name: name.into(),
        species: Species::Fire,
        gender: Gender::Female,
        variant: 1,
        age_ticks: 110 * TICKS_PER_DAY,
        lair: Some(lair),
        fullness: 100,
        health: None,
        seed: None,
        at: None,
    }
}

/// A world with one lair and its well-fed dragon at rest: a dragon that stays home for days.
struct Rig {
    w: World,
    d: Dragons,
    x: DragonId,
}

fn rig() -> Rig {
    let (mut w, _) = synth::world(SEED);
    let mut d = Dragons::new(SEED, DragonTunables::default(), Tick::ZERO);
    let l = d.add_lair(
        &mut w,
        9,
        "Lair",
        Species::Fire,
        LairKind::Roost,
        lair_pos(),
        100,
        Tick(0),
    );
    let x = d.add_dragon(&mut w, adult("Scorch", l), Tick(0));
    Rig { w, d, x }
}

/// The world and the dragons read back from their saves, as a restarted engine would.
fn restart(w: &World, d: &Dragons) -> (World, Dragons) {
    from_pages(&w.save(), &d.save())
}

fn from_pages(world: &[u8], dragons: &[u8]) -> (World, Dragons) {
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let w = World::load(WorldConfig::new(SEED), goods, book, world).expect("the world loads");
    let d = Dragons::load(DragonTunables::default(), dragons).expect("the dragons load");
    (w, d)
}

/// The rig read back from its pages.
fn restarted(r: Rig) -> Rig {
    let (w, d) = restart(&r.w, &r.d);
    Rig { w, d, x: r.x }
}

fn run(r: &mut Rig, at: Tick) -> Vec<Cmd> {
    r.d.run_until(&mut r.w, &mut NoInfrastructure, at);
    r.d.take_commands()
}

fn ingest(r: &mut Rig, o: &Obs, at: Tick) {
    r.d.ingest(&mut r.w, &mut NoInfrastructure, o, at);
}

/// A player's interest area at the lair from `at` for a long while.
fn stay(r: &mut Rig, at: Tick) {
    r.d.player_stays(&mut r.w, PLAYER, lair_pos(), at, at + 150_000);
}

fn materializations(cmds: &[Cmd]) -> Vec<boundary::Materialize> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::Materialize(m) => Some(*m),
            _ => None,
        })
        .collect()
}

fn adoptions(cmds: &[Cmd]) -> Vec<boundary::Adopt> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::Adopt(a) => Some(*a),
            _ => None,
        })
        .collect()
}

/// The epoch of the dragon's next materialization with a player at its lair from `at`.
fn next_epoch(r: &mut Rig, at: Tick) -> u32 {
    stay(r, at);
    let agent = agent_of(r.x);
    let cmds = run(r, at + 10);
    let ms: Vec<_> = materializations(&cmds)
        .into_iter()
        .filter(|m| m.agent == agent)
        .collect();
    assert_eq!(ms.len(), 1, "one materialization at the lair: {cmds:?}");
    ms[0].epoch
}

fn materialized(agent: u64, epoch: u32) -> Obs {
    Obs::Materialized(boundary::Materialized {
        agent,
        epoch,
        dimension: 0,
        entity: (0xB0D1, u64::from(epoch)),
    })
}

fn dematerialized(
    agent: u64,
    epoch: u32,
    reason: u32,
    health: f32,
    pos: (f64, f64, f64),
) -> Obs {
    Obs::Dematerialized(boundary::Dematerialized {
        agent,
        epoch,
        reason,
        dimension: 0,
        health,
        max_health: 500.0,
        hunger: 55,
        age_ticks: (110 * TICKS_PER_DAY) as i32,
        pos,
    })
}

fn hurt(agent: u64, epoch: u32, health_after: f32) -> Obs {
    Obs::Hurt(boundary::Hurt {
        agent,
        epoch,
        attacker_kind: AttackerKind::Player,
        attacker: PLAYER,
        hits: 3,
        amount: 140.0,
        health_after,
    })
}

fn died(agent: u64, epoch: u32) -> Obs {
    Obs::Died(boundary::Died {
        agent,
        epoch,
        killer_kind: AttackerKind::Player,
        killer: PLAYER,
        dimension: 0,
        pos: (120.0, 80.0, -40.0),
    })
}

fn untagged() -> Obs {
    Obs::Untagged(boundary::Untagged {
        entity: EGG,
        spawn: SpawnKind::SpawnEgg,
        species: Species::Ice,
        dimension: 0,
        flags: 0,
        age_ticks: (60 * TICKS_PER_DAY) as i32,
        hunger: 70,
        gender: Gender::Male,
        variant: 2,
        home: (0, 0, 0),
        lair: 0,
        pos: (120.0, 80.0, -40.0),
    })
}

// ---- test 1: a death after the save counts (§5.1) ---------------------------------------------------

/// A spawn-egg dragon adopted (embodied) and saved; a player kills it after the save; the server
/// then dies, and the session that restores the pages replays the death. With `crash` false the
/// control: the pages are taken after the death. The dragons alive after the restart and replay.
fn alive_after_a_death(crash: bool) -> usize {
    let (mut w, _) = synth::world(SEED);
    let mut d = Dragons::new(SEED, DragonTunables::default(), Tick::ZERO);
    d.ingest(&mut w, &mut NoInfrastructure, &untagged(), Tick(100));
    let a = adoptions(&d.take_commands())
        .pop()
        .expect("the spawn-egg dragon is adopted");
    assert_eq!(d.dragons.len(), 1);
    let at_the_save = (w.save(), d.save());
    d.ingest(&mut w, &mut NoInfrastructure, &died(a.agent, a.epoch), Tick(900));
    assert_eq!(d.dragons.len(), 0, "the session that went on: it is dead");
    if crash {
        let (mut w2, mut d2) = from_pages(&at_the_save.0, &at_the_save.1);
        assert_eq!(d2.dragons.len(), 1, "the pages predate the death");
        begin(&mut d2, &mut w2, Tick(100), true);
        d2.ingest(
            &mut w2,
            &mut NoInfrastructure,
            &died(a.agent, a.epoch),
            Tick(900),
        );
        end(&mut d2, &mut w2, Tick(1_000));
        d2.dragons.len()
    } else {
        let (mut w2, mut d2) = restart(&w, &d);
        begin(&mut d2, &mut w2, Tick(900), true);
        end(&mut d2, &mut w2, Tick(1_000));
        d2.dragons.len()
    }
}

#[test]
fn a_dragon_killed_after_a_save_is_dead_after_the_replay() {
    assert_eq!(alive_after_a_death(true), 0, "the replayed death counts");
}

#[test]
fn a_dragon_killed_before_a_save_is_dead_after_a_restart() {
    assert_eq!(alive_after_a_death(false), 0, "the control: no crash");
}

// ---- test 2: no epoch is used twice (§5.3, RP-7) ----------------------------------------------------

/// The last session materialized the dragon after the pages (epoch 1) and captured it again; the
/// next body of this session is not epoch 1 again. Today (the base): 1 again.
#[test]
fn a_replayed_body_does_not_give_its_epoch_to_the_next_one() {
    let mut r = restarted(rig());
    let agent = agent_of(r.x);
    begin(&mut r.d, &mut r.w, Tick(400), true);
    ingest(&mut r, &materialized(agent, 1), Tick(500));
    let at = (4000.0, 100.0, 4000.0);
    ingest(&mut r, &dematerialized(agent, 1, 3, 400.0, at), Tick(700));
    end(&mut r.d, &mut r.w, Tick(900));
    assert_eq!(next_epoch(&mut r, Tick(1_000)), 2, "above the replayed 1");
}

/// The control: nothing happened in the window, the first body is epoch 1.
#[test]
fn without_a_replayed_body_the_first_epoch_is_one() {
    let mut r = restarted(rig());
    begin(&mut r.d, &mut r.w, Tick(400), true);
    end(&mut r.d, &mut r.w, Tick(900));
    assert_eq!(next_epoch(&mut r, Tick(1_000)), 1);
}

/// An engine that is not from pages (RP-31) does not honour a replayed MATERIALIZED, which it
/// did not decide; the epoch is still taken.
#[test]
fn a_replayed_body_on_a_fresh_engine_only_raises_the_floor() {
    let mut r = rig();
    let agent = agent_of(r.x);
    begin(&mut r.d, &mut r.w, Tick(400), false);
    ingest(&mut r, &materialized(agent, 4), Tick(500));
    assert!(
        !matches!(r.d.dragon(r.x).unwrap().state, State::Embodied { .. }),
        "a fresh engine embodies nothing for a record"
    );
    end(&mut r.d, &mut r.w, Tick(900));
    assert_eq!(next_epoch(&mut r, Tick(1_000)), 5);
}

/// Every way the last session's body can end in the replayed window (captured, refused, killed
/// is the dragon's end and has its own test): the next body is above it.
#[test]
fn an_epoch_that_ended_in_the_replayed_window_is_never_reused() {
    let at = (4000.0, 100.0, 4000.0);
    let endings: Vec<(&str, Box<dyn Fn(u64) -> Obs>)> = vec![
        (
            "captured",
            Box::new(move |a| dematerialized(a, 1, 3, 400.0, at)),
        ),
        (
            "left tracking",
            Box::new(move |a| dematerialized(a, 1, 4, 400.0, at)),
        ),
        (
            "refused",
            Box::new(|a| {
                Obs::SpawnRejected(boundary::SpawnRejected {
                    agent: a,
                    epoch: 1,
                    reason: 1,
                    cmd_seq: 3,
                })
            }),
        ),
    ];
    for (what, ending) in endings {
        let mut r = restarted(rig());
        let agent = agent_of(r.x);
        begin(&mut r.d, &mut r.w, Tick(400), true);
        ingest(&mut r, &materialized(agent, 1), Tick(500));
        ingest(&mut r, &ending(agent), Tick(700));
        end(&mut r.d, &mut r.w, Tick(900));
        let e = next_epoch(&mut r, Tick(1_000));
        assert!(e > 1, "{what}: the next body is epoch {e}");
    }
}

/// The last session's body, embodied by the replayed MATERIALIZED, takes the records that follow:
/// a player's wound lowers its health and is a grudge, as it was in the session that went on.
#[test]
fn a_replayed_body_takes_the_wounds_that_follow() {
    let mut r = restarted(rig());
    let agent = agent_of(r.x);
    let full = r.d.dragon(r.x).unwrap().health;
    begin(&mut r.d, &mut r.w, Tick(400), true);
    ingest(&mut r, &materialized(agent, 1), Tick(500));
    ingest(&mut r, &hurt(agent, 1, 200.0), Tick(600));
    end(&mut r.d, &mut r.w, Tick(900));
    let (health, actor) = {
        let x = r.d.dragon(r.x).unwrap();
        (x.health, x.actor)
    };
    assert!(full > 250.0, "a healthy dragon to start with: {full}");
    assert_eq!(health, 200.0, "the wound counts");
    let player = r.d.player(&mut r.w, PLAYER);
    let grudge = r.w.knowledge.sentiment(actor, player, Tick(900));
    assert!(
        grudge <= r.d.tun.grudge_below,
        "the wound is a grudge ({grudge})"
    );
}

/// A copy of a record from an ended session (S4p's REASSERT) embodies nothing, and its epoch is
/// taken all the same, live or not.
#[test]
fn a_copied_body_raises_the_floor_and_embodies_nothing() {
    let mut r = rig();
    let agent = agent_of(r.x);
    let at = (4000.0, 100.0, 4000.0);
    r.d.ingest_copy(
        &mut r.w,
        &mut NoInfrastructure,
        &materialized(agent, 5),
        Tick(100),
    );
    r.d.ingest_copy(
        &mut r.w,
        &mut NoInfrastructure,
        &dematerialized(agent, 5, 3, 300.0, at),
        Tick(200),
    );
    assert!(
        !matches!(r.d.dragon(r.x).unwrap().state, State::Embodied { .. }),
        "a copy embodies nothing"
    );
    assert_eq!(next_epoch(&mut r, Tick(1_000)), 6, "above the copy's 5");
}

// ---- RP-57: every observation kind with its answer --------------------------------------------------

/// One record of every kind, naming `agent` and `epoch` where the kind names an epoch; whether
/// the kind raises the epoch floor (the others carry no epoch).
fn every_kind(agent: u64, epoch: u32) -> Vec<(&'static str, Obs, bool)> {
    use boundary::{
        Ate, Breaths, HoardRobbed, LairChest, LairFound, NestCensus, NestFound, NestHeart,
        NestLoss, OilChunkRec, OilChunks, OilConfig,
    };
    let at = (4000.0, 100.0, 4000.0);
    vec![
        (
            "State",
            Obs::State(boundary::State {
                agent,
                epoch,
                flags: 0,
                dimension: 0,
                health: 400.0,
                max_health: 500.0,
                hunger: 80,
                age_ticks: 0,
                pos: at,
            }),
            true,
        ),
        ("Materialized", materialized(agent, epoch), true),
        (
            "SpawnRejected",
            Obs::SpawnRejected(boundary::SpawnRejected {
                agent,
                epoch,
                reason: 1,
                cmd_seq: 1,
            }),
            true,
        ),
        (
            "Dematerialized",
            dematerialized(agent, epoch, 3, 400.0, at),
            true,
        ),
        ("Hurt", hurt(agent, epoch, 300.0), true),
        ("Died", died(agent, epoch), true),
        (
            "Ate",
            Obs::Ate(Ate {
                agent,
                epoch,
                prey: 1,
                count: 1,
                village: 0,
                pos: at,
            }),
            true,
        ),
        (
            "Breaths",
            Obs::Breaths(Breaths {
                agent,
                epoch,
                count: 2,
            }),
            true,
        ),
        (
            "LairFound",
            Obs::LairFound(LairFound {
                lair: 0xAA,
                late: false,
                dimension: 0,
                species: Species::Fire,
                kind: LairKind::Roost,
                min: (2000, 60, 2000),
                max: (2060, 140, 2060),
                home: (2030, 100, 2030),
            }),
            false,
        ),
        (
            "LairChest",
            Obs::LairChest(LairChest {
                lair: 9,
                dimension: 0,
                pos: (4000, 100, 4000),
            }),
            false,
        ),
        (
            "HoardRobbed",
            Obs::HoardRobbed(HoardRobbed {
                lair: 9,
                op: 1 << 63 | 7,
                player: PLAYER,
                items: Vec::new(),
            }),
            false,
        ),
        ("Untagged", untagged(), false),
        (
            "OilChunks",
            Obs::OilChunks(OilChunks {
                dimension: 0,
                source: 2,
                chunks: vec![OilChunkRec {
                    cx: 3,
                    cz: 4,
                    amount: 1000,
                    flags: 0,
                }],
            }),
            false,
        ),
        ("OilConfig", Obs::OilConfig(OilConfig { flags: 0 }), false),
        (
            "NestFound",
            Obs::NestFound(NestFound {
                nest: 5,
                placed: true,
                dimension: 0,
                heart: (0, 0, 0),
            }),
            false,
        ),
        (
            "NestCensus",
            Obs::NestCensus(NestCensus {
                nest: 5,
                worms: 2,
                heart_missing: false,
            }),
            false,
        ),
        (
            "NestLoss",
            Obs::NestLoss(NestLoss {
                nest: 5,
                count: 1,
                cause: 1,
                player: (0, 0),
            }),
            false,
        ),
        (
            "NestHeart",
            Obs::NestHeart(NestHeart {
                nest: 5,
                player: PLAYER,
            }),
            false,
        ),
    ]
}

/// RP-57's table for the dragons (docs/replay-gate.md §4.5, RP-76): eight kinds name an epoch and
/// raise the dragon's floor, replayed whatever they do after (a body not held drops them); ten
/// carry none. The next body's epoch follows.
#[test]
fn every_replayed_kind_raises_the_epoch_floor_or_carries_no_epoch() {
    let agent = agent_of(rig().x);
    let kinds = every_kind(agent, 7);
    assert_eq!(kinds.len(), 18, "every kind of Obs");
    assert_eq!(kinds.iter().filter(|k| k.2).count(), 8);
    for (name, o, raises) in kinds {
        assert_eq!(
            o.agent_epoch(),
            raises.then_some((agent, 7)),
            "{name}: the kind's own answer"
        );
        let mut r = restarted(rig());
        assert_eq!(r.x, rig().x, "the same handle");
        begin(&mut r.d, &mut r.w, Tick(400), true);
        ingest(&mut r, &o, Tick(500));
        end(&mut r.d, &mut r.w, Tick(900));
        let e = next_epoch(&mut r, Tick(1_000));
        assert_eq!(e, if raises { 8 } else { 1 }, "{name}: the next epoch");
    }
}

// ---- test 3: the DEPART trap (§5.8) -----------------------------------------------------------------

/// An embodied dragon told to leave (hungry): its agent, epoch and where it was, and the rig.
fn leaving() -> (Rig, u64, u32) {
    let mut r = rig();
    let agent = agent_of(r.x);
    stay(&mut r, Tick(100));
    let m = materializations(&run(&mut r, Tick(110)))
        .pop()
        .expect("it materializes at home");
    ingest(&mut r, &materialized(m.agent, m.epoch), Tick(120));
    ingest(
        &mut r,
        &Obs::State(boundary::State {
            agent,
            epoch: m.epoch,
            flags: 0,
            dimension: 0,
            health: 350.0,
            max_health: 500.0,
            hunger: 0,
            age_ticks: (110 * TICKS_PER_DAY) as i32,
            pos: (4100.0, 120.0, 4100.0),
        }),
        Tick(200),
    );
    let departs: Vec<_> = r
        .d
        .take_commands()
        .into_iter()
        .filter(|c| matches!(c, Cmd::Depart(_)))
        .collect();
    assert_eq!(departs.len(), 1, "a starving dragon is told to leave");
    assert!(matches!(
        r.d.dragon(r.x).unwrap().state,
        State::Leaving { .. }
    ));
    (r, agent, m.epoch)
}

fn near(r: &Rig, at: Tick, x: i32, z: i32) -> bool {
    let (p, _) = r.d.dragon(r.x).unwrap().position(&r.d.topo, &r.d.lairs, at);
    (p.x - x).abs() <= 64 && (p.z - z).abs() <= 64
}

/// The plugin holds no body of the dragon (the session before ended, the DEPART replayed or
/// copied): it answers NOT_EMBODIED with an agent and an epoch and zeros. The dragon is where the
/// engine had it, with the health it had. Today (the base): health 0 at (0, 0, 0).
#[test]
fn a_depart_answered_not_embodied_keeps_the_dragon_where_it_was() {
    let (mut r, agent, epoch) = leaving();
    let at = Tick(300);
    let zeros = dematerialized(agent, epoch, boundary::DEMAT_NOT_EMBODIED, 0.0, (0.0, 0.0, 0.0));
    ingest(&mut r, &zeros, at);
    let x = r.d.dragon(r.x).unwrap();
    assert_eq!(x.health, 350.0, "health as the engine had it");
    assert!(
        near(&r, at, 4100, 4100),
        "where the engine had it: {:?}",
        x.position(&r.d.topo, &r.d.lairs, at)
    );
    assert!(
        matches!(x.state, State::Flying(_) | State::Roosting),
        "abstract again: {:?}",
        x.state
    );
    r.d.check(&r.w, None).unwrap();
}

/// The control: a DEMATERIALIZED of a held dragon takes its health and its position.
#[test]
fn a_dematerialized_of_a_held_dragon_takes_its_health_and_position() {
    let (mut r, agent, epoch) = leaving();
    let at = Tick(300);
    ingest(
        &mut r,
        &dematerialized(agent, epoch, 1, 275.0, (4200.0, 130.0, 4300.0)),
        at,
    );
    let x = r.d.dragon(r.x).unwrap();
    assert_eq!(x.health, 275.0);
    assert!(near(&r, at, 4200, 4300), "{:?}", x.position(&r.d.topo, &r.d.lairs, at));
}

// ---- test 4: the dragon adopted in the replayed window (RP-13) --------------------------------------

/// A dragon adopted by the replayed UNTAGGED has its ADOPT dropped (the entity is not there), so
/// the entity is untagged again when it joins, and DragonBridge reports it again: the live report
/// re-adopts the same dragon at a new epoch. The plugin's tag is not in the pages
/// (docs/replay-gate.md §5.4, RP-13; DragonBridge `reportUntagged`, Z9).
#[test]
fn a_replayed_untagged_dragon_is_adopted_again_by_its_live_report() {
    let (mut w, _) = synth::world(SEED);
    let mut d = Dragons::new(SEED, DragonTunables::default(), Tick::ZERO);
    begin(&mut d, &mut w, Tick(0), true);
    d.ingest(&mut w, &mut NoInfrastructure, &untagged(), Tick(100));
    assert_eq!(d.dragons.len(), 1, "the engine adopted it");
    let sent = adoptions(&d.take_commands());
    assert!(sent.is_empty(), "no ADOPT while replaying: {sent:?}");
    assert!(gate(&d).0 >= 1, "the gate dropped it: {:?}", gate(&d));
    let first_epoch = d.dragons.iter().next().unwrap().1.epoch;
    end(&mut d, &mut w, Tick(500));
    d.ingest(&mut w, &mut NoInfrastructure, &untagged(), Tick(600));
    let again = adoptions(&d.take_commands());
    assert_eq!(again.len(), 1, "the live report re-adopts it");
    assert_eq!(d.dragons.len(), 1, "the same dragon, never a second");
    assert_eq!(again[0].entity, EGG);
    assert!(
        again[0].epoch > first_epoch,
        "a new epoch ({} after {first_epoch})",
        again[0].epoch
    );
    // Reported a third time while embodied in this session: nothing more.
    d.ingest(&mut w, &mut NoInfrastructure, &untagged(), Tick(700));
    assert!(adoptions(&d.take_commands()).is_empty());
}

/// The control: a live UNTAGGED adopts at once, at epoch 1.
#[test]
fn a_live_untagged_dragon_is_adopted_at_once() {
    let (mut w, _) = synth::world(SEED);
    let mut d = Dragons::new(SEED, DragonTunables::default(), Tick::ZERO);
    d.ingest(&mut w, &mut NoInfrastructure, &untagged(), Tick(100));
    let sent = adoptions(&d.take_commands());
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].epoch, 1);
    assert_eq!(d.dragons.len(), 1);
}

// ---- test 5: nothing embodies while a long replay runs ----------------------------------------------

/// A player stays at the lair for the whole replayed window of `days`; the dragon is home. The
/// materializations the window decided, the gate's counts at the end, and the rig.
fn long_replay(replaying: bool, days: u64) -> (Vec<boundary::Materialize>, (u64, u64, u64), Rig) {
    let mut r = restarted(rig());
    if replaying {
        begin(&mut r.d, &mut r.w, Tick(0), true);
    }
    stay(&mut r, Tick(100));
    let mut found = Vec::new();
    let mut t = Tick(100);
    while t.0 < days * TICKS_PER_DAY {
        t += 1_000;
        found.extend(materializations(&run(&mut r, t)));
    }
    let counts = gate(&r.d);
    (found, counts, r)
}

/// The window's player stands at the lair (the saved interest area, a replayed position): the
/// dragon is home, and a sighting books a body. While the replay runs none does; the sightings
/// are held (deferred) and made again at the replay end, when the host routes the positions
/// again. Today (the base): the dragon is embodied in the middle of the replay.
#[test]
fn no_dragon_materializes_while_a_long_replay_runs_with_a_player_near() {
    let (found, counts, mut r) = long_replay(true, 2);
    assert!(found.is_empty(), "materialized in the replay: {found:?}");
    assert_eq!(counts.1, 0, "no 'never' command was decided: {counts:?}");
    assert!(counts.2 >= 1, "the sighting was deferred: {counts:?}");
    r.d.check(&r.w, None).unwrap();
    // The replay ends; the host routes the player's position again: the dragon is seen at once.
    let at = Tick(2 * TICKS_PER_DAY + 100);
    end(&mut r.d, &mut r.w, at);
    let (p, _) = r.d.dragon(r.x).unwrap().position(&r.d.topo, &r.d.lairs, at);
    r.d.player_stays(&mut r.w, PLAYER, p, at, at + 150_000);
    let cmds = run(&mut r, at + 10);
    let ms = materializations(&cmds);
    assert_eq!(ms.len(), 1, "seen after the replay end: {cmds:?}");
    assert_eq!(ms[0].epoch, 1);
}

/// The control: the same window with no replay running embodies the dragon at the lair at once.
#[test]
fn a_dragon_materializes_when_no_replay_runs_with_a_player_near() {
    let (found, counts, _) = long_replay(false, 2);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].epoch, 1);
    assert_eq!(counts, (0, 0, 0), "a live engine defers nothing");
}
