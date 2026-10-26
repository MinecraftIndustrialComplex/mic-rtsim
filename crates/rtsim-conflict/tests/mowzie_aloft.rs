// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine half of MOWZIE.md's slice M11 (ledger 24, §4.5 "When it has a body"): the grove's
//! chief goes with its band as a stack of its own behind an away CHIEF_BODY, comes home by a
//! HAND_BACK, is paid as a puppet, and is booked once whatever ends it. Played against the emulated
//! plugin (`synth::CampRun`) on the shipped cultures.

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{Cultures, PartyKind};
use rtsim_conflict::engine::{CHIEF_SLOT, Conflict, Party, Phase, SlotState, Target, body_id};
use rtsim_conflict::synth::{self, CampRun};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_settle::Pos;

// ---- set-up -------------------------------------------------------------------------------------

/// A grove that never plunders, so the only parties are the ones a test orders.
fn quiet_grove(seed: u64) -> CampRun {
    let mut cultures = Cultures::builtin();
    for k in &mut cultures.list {
        k.def.plunder_mean_days = None;
    }
    CampRun::with_cultures(seed, cultures, "umvuthana", 9)
}

fn ok(r: &mut CampRun) {
    let bad = r.check();
    assert!(bad.is_empty(), "the invariants break: {bad:#?}");
    assert!(r.failures.is_empty(), "{:#?}", r.failures);
}

/// The camp comes in and its first lines are answered.
fn come_in(r: &mut CampRun) {
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
}

/// The chief's slot: its state and generation.
fn chief(r: &CampRun) -> Option<(SlotState, u32)> {
    r.c.camp(r.f)
        .and_then(|c| c.chief())
        .map(|s| (s.state, s.generation))
}

/// The camp comes in and waits out its first heir: a seated Umvuthi, its generation.
fn seat_chief(r: &mut CampRun) -> u32 {
    come_in(r);
    let due = r.c.camp(r.f).and_then(|c| c.heir).expect("an heir").due;
    r.run(due + 100);
    let (state, generation) = chief(r).expect("seated");
    assert_eq!(state, SlotState::Body, "seated");
    generation
}

/// A player's payment of 9 gold nuggets to the seated Umvuthi at home: the payer joins the camp's
/// memory (MZB-418), so a CHIEF_BODY that carries it is told apart from one that carries nothing.
fn pay_at_home(r: &mut CampRun, generation: u32) {
    let g =
        r.w.goods
            .find("gold_nugget")
            .expect("a good of the synthetic world");
    let id = rtsim_conflict::engine::item_of(g);
    r.c.define(2, id, "gold_nugget");
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut *r.infra,
        &Obs::ChiefPaid(boundary::ChiefPaid {
            faction: r.key,
            slot: CHIEF_SLOT,
            generation,
            flags: 0,
            player: synth::PLAYER,
            op: 0x5111,
            items: vec![boundary::Item { item: id, count: 9 }],
        }),
        at,
    );
    let traded = &r.c.camp(r.f).expect("a camp").traded;
    assert_eq!(traded, &vec![synth::PLAYER], "the payer is remembered");
}

/// An Invasion led by the seated Umvuthi, paid once at home, ordered now on the refinery and out on
/// the road past its muster: the party.
fn led_invasion_out(seed: u64) -> (CampRun, Handle<Party>) {
    let mut r = quiet_grove(seed);
    let generation = seat_chief(&mut r);
    pay_at_home(&mut r, generation);
    let at = r.c.now();
    let target = Target::District(synth::REFINERY, synth::REFINERY_POS);
    let p =
        r.c.order_party(&mut r.w, r.key, PartyKind::Invasion, target, None, at)
            .expect("an invasion");
    r.flush(at);
    assert!(r.c.parties[p].chief, "the chief leads");
    let muster = r.c.parties[p].until;
    r.run(muster + 200);
    assert_eq!(r.c.parties[p].phase, Phase::Out, "on the road");
    (r, p)
}

/// The engine's body id of the grove's chief.
fn chief_body(r: &CampRun) -> u32 {
    let ci = r.c.factions[r.f as usize].culture;
    let name =
        r.c.cultures
            .get(ci)
            .def
            .chief_body
            .clone()
            .expect("a chief body");
    body_id(&r.c.cultures, &name)
}

fn here(r: &CampRun, p: Handle<Party>) -> Pos {
    let at = r.c.now();
    let (x, z) = r.c.party_pos(p, at);
    Pos::new(synth::CAMP_POS.frame, x, z)
}

/// The scripted player meets the band where it is now, reported to the engine as the bridge
/// reports a player: the commands that took.
fn meet(r: &mut CampRun, p: Handle<Party>) -> Vec<Cmd> {
    let at = r.c.now();
    let pos = here(r, p);
    let mark = r.cmds.len();
    r.c.observe_player(&mut r.w, synth::PLAYER, pos, at);
    // The meeting is an event of the engine's wheel, due now.
    r.run(at + 2);
    r.cmds[mark..].iter().map(|c| c.1.clone()).collect()
}

/// The commands sent from `mark` on.
fn since(r: &CampRun, mark: usize) -> Vec<Cmd> {
    r.cmds[mark..].iter().map(|c| c.1.clone()).collect()
}

/// The away CHIEF_BODYs of `agent` in `cmds`, checked, with the MATERIALIZE each leads. Each is the
/// party's (the camp's key, health 0, flags 0, the camp's memory) and goes just ahead of the
/// MATERIALIZE of its agent and epoch, which has BAND_CHIEF and, last, the stack `{ chief body, 0,
/// MEMBER_CHIEF }`; and a MATERIALIZE of the agent has a chief stack exactly when a CHIEF_BODY leads
/// it. Returns the MATERIALIZEs led.
fn chief_pairs(r: &CampRun, cmds: &[Cmd], agent: u64) -> Vec<boundary::BandMaterialize> {
    let chief = boundary::Stack {
        body: chief_body(r),
        count: 0,
        flags: boundary::MEMBER_CHIEF,
    };
    let traded = r.c.camp(r.f).map(|c| c.traded.clone()).unwrap_or_default();
    let mut out = Vec::new();
    for (i, c) in cmds.iter().enumerate() {
        match c {
            Cmd::ChiefBody(b) if b.agent == agent => {
                let Some(Cmd::Materialize(m)) = cmds.get(i + 1) else {
                    panic!("an away CHIEF_BODY not just ahead of a MATERIALIZE: {cmds:#?}");
                };
                assert_eq!(
                    (m.agent, u64::from(m.epoch)),
                    (b.agent, b.epoch),
                    "one band, one epoch"
                );
                assert_eq!(b.faction, r.key);
                assert_eq!(
                    (b.health, b.flags),
                    (0.0, 0),
                    "health 0, the body's own; no flags"
                );
                assert_eq!(b.traded, traded, "the camp's memory");
                assert_ne!(m.flags & boundary::BAND_CHIEF, 0, "BAND_CHIEF");
                assert_eq!(m.members.last(), Some(&chief), "the chief's stack, last");
                out.push(m.clone());
            }
            Cmd::Materialize(m) if m.agent == agent => {
                let led = i > 0 && matches!(&cmds[i - 1], Cmd::ChiefBody(b) if b.agent == agent);
                let stacks = m
                    .members
                    .iter()
                    .filter(|s| s.flags & boundary::MEMBER_CHIEF != 0)
                    .count();
                assert_eq!(
                    stacks,
                    usize::from(led),
                    "a chief stack only behind a CHIEF_BODY"
                );
            }
            _ => {}
        }
    }
    out
}

/// The members a MATERIALIZE carries that are units: every stack but the chief's.
fn units_in(m: &boundary::BandMaterialize) -> u32 {
    m.members
        .iter()
        .filter(|s| s.flags & boundary::MEMBER_CHIEF == 0)
        .map(|s| s.count)
        .sum()
}

fn placed(r: &CampRun, p: Handle<Party>) -> u32 {
    r.c.parties[p].spawned.iter().map(|e| e.1).sum()
}

/// The band arrives at its target a tick from now with the scripted player standing there, set
/// through the world as the bridge reports a player who has not met it on the road: a watched
/// fight. Returns the tick of the arrival.
fn player_at_the_target(r: &mut CampRun, p: Handle<Party>) -> Tick {
    let arrive = *r.c.parties[p].times.last().expect("a route");
    r.run(Tick(arrive.0 - 1));
    let at = r.c.now();
    let player = r.player;
    r.w.set_player_pos(player, synth::REFINERY_POS, at);
    arrive
}

// ---- A1: the chief's stack and its CHIEF_BODY ---------------------------------------------------

/// A1 (MZB-801, MZB-803): each first MATERIALIZE with bodies of an epoch of a band its Umvuthi
/// leads goes just behind an away CHIEF_BODY (the band's agent and epoch, health 0, flags 0, the
/// camp's memory) and carries BAND_CHIEF and, last, the chief alone, `{ umvuthi, 0, MEMBER_CHIEF
/// }`: met on the road (`seen`), at its target (`start_fight`), at a later wave after Java refused
/// the first, and on its way home. The chief is in no fill: `spawned` and the waves count the
/// units only. A walk-in and a second wave of the epoch send neither. Without the change no
/// CHIEF_BODY and no chief stack goes (the pairs found are none).
#[test]
fn the_chief_body_goes_ahead_of_the_chiefs_stack() {
    // 1. On the road, then walked into its fight.
    let (mut r, p) = led_invasion_out(81);
    let agent = Party::agent(p);
    let waves = r.c.stats.waves;
    let cmds = meet(&mut r, p);
    let pairs = chief_pairs(&r, &cmds, agent);
    assert_eq!(pairs.len(), 1, "met on the road: one pair");
    assert_eq!(pairs[0].epoch, r.c.parties[p].epoch);
    assert_eq!(
        units_in(&pairs[0]),
        placed(&r, p),
        "the chief is in no fill"
    );
    assert_eq!(
        r.c.stats.waves,
        waves + 1,
        "one wave, the chief not counted apart"
    );
    // The player follows the band to its target: it walks into its fight in the same bodies.
    let arrive = *r.c.parties[p].times.last().expect("a route");
    let mark = r.cmds.len();
    while r.c.now() < arrive + 5 {
        let at = r.c.now();
        let pos = here(&r, p);
        r.c.observe_player(&mut r.w, synth::PLAYER, pos, at);
        let to = at + 50;
        r.run(to);
    }
    assert_eq!(r.c.parties[p].phase, Phase::Fighting, "a fight");
    let walked = since(&r, mark);
    assert!(
        walked
            .iter()
            .any(|c| matches!(c, Cmd::Materialize(m) if m.agent == agent)),
        "the walked-in fight's marker went"
    );
    assert!(
        chief_pairs(&r, &walked, agent).is_empty(),
        "a walk-in sends no chief"
    );
    ok(&mut r);

    // 2. At its target, unmet on the road; 3. the fight's later waves, one after Java refused the
    // first. With a quota of 2 the scripted player's kills make room for a second wave.
    for refused in [false, true] {
        let (mut r, p) = led_invasion_out(82);
        r.c.tun.spawn_quota = 2;
        r.plugin.fight = Some(synth::Fight {
            kill_share: 1.0,
            after: 10,
            kill_chief: false,
            only_at_base: false,
        });
        let agent = Party::agent(p);
        let arrive = player_at_the_target(&mut r, p);
        let mark = r.cmds.len();
        let first = if refused {
            let cmds = r.decide(arrive + 1);
            let pairs = chief_pairs(&r, &cmds, agent);
            assert_eq!(pairs.len(), 1, "the fight's first wave");
            let m = &pairs[0];
            let at = r.c.now();
            r.c.ingest(
                &mut r.w,
                &mut NoInfrastructure,
                &Obs::SpawnRejected(boundary::BandSpawnRejected {
                    agent,
                    epoch: m.epoch,
                    wave: 0,
                    reason: 1,
                    cmd_seq: 0,
                }),
                at,
            );
            assert_eq!(placed(&r, p), 0, "refused: nothing placed");
            m.clone()
        } else {
            r.run(arrive + 1);
            let cmds = since(&r, mark);
            let pairs = chief_pairs(&r, &cmds, agent);
            assert_eq!(pairs.len(), 1, "the fight's first wave");
            assert_eq!(units_in(&pairs[0]), 2, "the quota's 2");
            assert_eq!(placed(&r, p), 2, "the chief is in no fill");
            pairs[0].clone()
        };
        assert_eq!(r.c.parties[p].phase, Phase::Fighting, "refused {refused}");
        let epoch = first.epoch;
        // The next wave: the scripted player's kills put it a wave interval after them.
        let mark = r.cmds.len();
        let limit = r.c.now() + 2 * r.c.tun.wave_interval_ticks;
        let materialized = |r: &CampRun| {
            r.cmds[mark..]
                .iter()
                .any(|(_, c)| matches!(c, Cmd::Materialize(m) if m.agent == agent))
        };
        while !materialized(&r) && r.c.now() < limit {
            let to = r.c.now() + 10;
            r.run(to);
        }
        let later = since(&r, mark);
        let pairs = chief_pairs(&r, &later, agent);
        let waves: Vec<&boundary::BandMaterialize> = later
            .iter()
            .filter_map(|c| match c {
                Cmd::Materialize(m) if m.agent == agent => Some(m),
                _ => None,
            })
            .collect();
        assert_eq!(waves.len(), 1, "refused {refused}: one later wave");
        assert_eq!(waves[0].epoch, epoch, "refused {refused}: the same epoch");
        assert_ne!(
            waves[0].flags & boundary::BAND_CHIEF,
            0,
            "BAND_CHIEF on every wave"
        );
        if refused {
            assert_eq!(
                pairs.len(),
                1,
                "after the refusal the chief goes with the next wave"
            );
            assert_eq!(units_in(waves[0]), 2, "the quota's 2");
        } else {
            assert!(
                pairs.is_empty(),
                "a second wave of the epoch sends no chief"
            );
            assert_eq!(units_in(waves[0]), 2, "the second wave's room");
        }
    }

    // 4. On its way home, unmet before.
    let (mut r, p) = led_invasion_out(83);
    let agent = Party::agent(p);
    let arrive = *r.c.parties[p].times.last().expect("a route");
    r.run(arrive + 400);
    assert_eq!(r.c.parties[p].phase, Phase::Home, "walking home");
    assert!(!r.c.parties[p].embodied, "unmet");
    let cmds = meet(&mut r, p);
    let pairs = chief_pairs(&r, &cmds, agent);
    assert_eq!(pairs.len(), 1, "met on its way home: one pair");
    assert_eq!(units_in(&pairs[0]), placed(&r, p));
    ok(&mut r);
}

/// A1's controls: the Frostmaw, whose chief is its band's one body, sends no CHIEF_BODY and keeps
/// MEMBER_CHIEF on its first stack; a grove's raid, which its Umvuthi does not lead, sends neither.
#[test]
fn a_chief_that_is_a_band_body_and_a_band_without_its_chief_send_no_chief_body() {
    let mut r = CampRun::new(67, "frostmaw", 1);
    come_in(&mut r);
    r.watch = true;
    let p = r.raid().expect("a raid goes");
    r.camp_out();
    let agent = Party::agent(p);
    let limit = r.c.now() + 4 * TICKS_PER_DAY;
    let first = loop {
        let m = r.cmds.iter().find_map(|(_, c)| match c {
            Cmd::Materialize(m) if m.agent == agent => Some(m.clone()),
            _ => None,
        });
        if let Some(m) = m {
            break m;
        }
        assert!(r.c.now() < limit, "the Frostmaw is seen");
        let to = r.c.now() + 100;
        r.run(to);
    };
    assert!(
        !r.cmds.iter().any(|(_, c)| matches!(c, Cmd::ChiefBody(_))),
        "no CHIEF_BODY"
    );
    assert_ne!(first.flags & boundary::BAND_CHIEF, 0);
    assert_eq!(
        first.members[0].flags,
        boundary::MEMBER_CHIEF,
        "its first stack is the chief"
    );

    let mut r = quiet_grove(84);
    seat_chief(&mut r);
    let p = r.raid().expect("a raid");
    assert!(!r.c.parties[p].chief, "the Umvuthi does not lead a raid");
    let muster = r.c.parties[p].until;
    r.run(muster + 200);
    let cmds = meet(&mut r, p);
    let agent = Party::agent(p);
    let m = cmds
        .iter()
        .find_map(|c| match c {
            Cmd::Materialize(m) if m.agent == agent => Some(m.clone()),
            _ => None,
        })
        .expect("the raid is seen");
    assert!(
        !cmds.iter().any(|c| matches!(c, Cmd::ChiefBody(_))),
        "no CHIEF_BODY"
    );
    assert_eq!(m.flags & boundary::BAND_CHIEF, 0, "no BAND_CHIEF");
    assert!(
        m.members
            .iter()
            .all(|s| s.flags & boundary::MEMBER_CHIEF == 0)
    );
    ok(&mut r);
}

// ---- A2: a chief never appears alone ------------------------------------------------------------

/// A2 (MZB-802): an Invasion whose warriors are all dead while its chief lives sends nothing when
/// met: no MATERIALIZE and no CHIEF_BODY, its Umvuthi abstract until its band has a body to show.
/// The control, one warrior left, sends both (and fails without the change: no CHIEF_BODY).
#[test]
fn a_band_with_only_its_chief_left_sends_nothing() {
    for left in [0, 1] {
        let (mut r, p) = led_invasion_out(85);
        let agent = Party::agent(p);
        let mut keep = left;
        for u in &mut r.c.parties[p].units {
            let k = u.1.min(keep);
            keep -= k;
            u.1 = k;
        }
        assert_eq!(r.c.parties[p].alive(), left);
        assert!(r.c.parties[p].chief, "its chief lives");
        let cmds = meet(&mut r, p);
        let materializes = cmds
            .iter()
            .filter(|c| matches!(c, Cmd::Materialize(m) if m.agent == agent))
            .count();
        let pairs = chief_pairs(&r, &cmds, agent);
        if left == 0 {
            assert_eq!(materializes, 0, "no MATERIALIZE");
            assert!(
                !cmds.iter().any(|c| matches!(c, Cmd::ChiefBody(_))),
                "no CHIEF_BODY"
            );
        } else {
            assert_eq!((materializes, pairs.len()), (1, 1), "one warrior: both");
        }
    }
}

// ---- A3: the gate drops the pair ----------------------------------------------------------------

/// A3 (MZB-804): an engine with an away CHIEF_BODY and its MATERIALIZE queued (its band met on its
/// way home, the commands not taken), and a home CHIEF_BODY queued after them (the band home at a
/// grove that is in), saved to pages and loaded: the queue holds all three, and at `begin_replay`
/// the pair is dropped and the home record kept. A meeting while replaying pushes neither
/// (`deferred` rises, `never` stays 0). Without the split the away CHIEF_BODY took the home row's
/// keep and outlived its MATERIALIZE.
#[test]
fn an_away_chief_body_is_dropped_with_its_materialize() {
    let (mut r, p) = led_invasion_out(86);
    let agent = Party::agent(p);
    let arrive = *r.c.parties[p].times.last().expect("a route");
    r.run(arrive + 400);
    assert_eq!(r.c.parties[p].phase, Phase::Home, "walking home");
    // Met on its way home, near enough to the camp that it is home before the met band's bodies are
    // gone; nothing is taken from here on.
    let home = *r.c.parties[p].times.last().expect("a route home");
    r.run(Tick(home.0 - 300));
    let at = r.c.now();
    let pos = here(&r, p);
    r.c.observe_player(&mut r.w, synth::PLAYER, pos, at);
    r.c.run_until(&mut r.w, &mut NoInfrastructure, at + 2);
    assert!(r.c.parties[p].embodied, "met");
    let epoch = r.c.parties[p].epoch;
    r.c.run_until(&mut r.w, &mut NoInfrastructure, home + 5);
    assert!(!r.c.parties.contains(p), "home");
    let pages = r.c.save();
    let load = || Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).expect("loads");

    let queued = load().take_commands();
    let away = |c: &Cmd| matches!(c, Cmd::ChiefBody(b) if b.agent == agent);
    let home_body = |c: &Cmd| matches!(c, Cmd::ChiefBody(b) if b.agent == 0);
    let band = |c: &Cmd| matches!(c, Cmd::Materialize(m) if m.agent == agent);
    let i = queued
        .iter()
        .position(away)
        .expect("the away CHIEF_BODY is queued");
    assert!(band(&queued[i + 1]), "just ahead of its MATERIALIZE");
    match &queued[i] {
        Cmd::ChiefBody(b) => assert_eq!(b.epoch, u64::from(epoch)),
        _ => unreachable!(),
    }
    assert!(
        queued[i..].iter().any(home_body),
        "the home CHIEF_BODY is queued after them: {queued:#?}"
    );

    let mut c = load();
    c.begin_replay(true);
    let kept = c.take_commands();
    assert!(!kept.iter().any(away), "the away CHIEF_BODY is dropped");
    assert!(!kept.iter().any(band), "with its MATERIALIZE");
    assert_eq!(
        kept.iter().filter(|c| home_body(c)).count(),
        1,
        "the home one is kept"
    );
    assert_eq!(c.replay_gate_counts().never, 0);

    // A meeting while replaying: a band out on the road, met before the replay ends.
    let (mut r, p) = led_invasion_out(87);
    let agent = Party::agent(p);
    r.restart_to_replay();
    let gate = r.c.replay_gate_counts();
    let cmds = meet(&mut r, p);
    let after = r.c.replay_gate_counts();
    assert!(after.deferred > gate.deferred, "the meeting is deferred");
    assert_eq!(after.never, 0, "never decided");
    assert!(
        !cmds.iter().any(|c| matches!(c, Cmd::ChiefBody(_))
            || matches!(c, Cmd::Materialize(m) if m.agent == agent)),
        "neither is pushed while replaying"
    );
    assert!(!r.c.parties[p].embodied);
}
