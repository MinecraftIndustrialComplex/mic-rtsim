// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dragon home bodies (EQUILIBRIUM.md slice S5, docs/overnight-choices-equilibrium.md EQC100 to
//! EQC149): a dragon roosting at its lair is a vanilla entity the plugin keeps (a *home body*,
//! saved by vanilla), and a puppet when it is away.
//!
//! The engine's belief is [`Dragon::body`](crate::dragon::Dragon): the entity it thinks stands for
//! the dragon, loaded or saved. It is set only from the plugin's answers (H3), so every path here
//! that wants a body orders it and waits for MATERIALIZED with the home bit. The lair census
//! ([`Dragons::lair_census`]) reconciles the books with the world each time a lair comes in:
//! removals first, then adoptions, then spawns (EQC112), so a removed body and a spawned one never
//! stand together. Nothing here runs while a session's replay runs (EQC123): the replay is the last
//! session's answers, and the lair is decided again from the next census.

use super::*;

/// What a census decided for one dragon of the lair, applied after the lines are ordered.
enum Plan {
    /// Adopt the body listed (index into the census), after any removals.
    Adopt(DragonId, usize),
    /// The dragon is away and its lair is watched: end the trip and adopt the body.
    Recall(DragonId, usize),
    /// An owed dragon at an unwatched lair gets a body spawned.
    Spawn(DragonId),
    /// A miss; the dragon may be written off.
    Miss(DragonId),
}

impl Dragons {
    /// The dragons of lair `l` that may hold a home body, at most `home_bodies_max`: those that
    /// hold one (or are being given one) first, then the oldest (EQC106). A third occupant, the
    /// young of a pair, stays a puppet while two stand.
    pub(crate) fn home_candidates(&self, l: LairId, at: Tick) -> Vec<DragonId> {
        let mut v: Vec<DragonId> = self.lairs[l as usize].occupants.clone();
        v.retain(|d| self.dragons.contains(*d));
        v.sort_by_key(|d| {
            let x = &self.dragons[*d];
            let held = x.body.is_some() || self.pending_home.contains(d);
            (
                std::cmp::Reverse(held),
                std::cmp::Reverse(x.age(at)),
                d.to_bits(),
            )
        });
        v.truncate(self.tun.home_bodies_max);
        v
    }

    /// Dragons of lair `l` with a home body, or being given one.
    pub(crate) fn homed_count(&self, l: LairId) -> usize {
        self.lairs[l as usize]
            .occupants
            .iter()
            .filter(|d| {
                self.dragons
                    .get(**d)
                    .is_some_and(|x| x.body.is_some() || self.pending_home.contains(d))
            })
            .count()
    }

    /// A lair came in (EQC112): reconcile its books with the bodies the plugin found.
    pub(crate) fn lair_census(&mut self, w: &mut World, c: &boundary::LairCensus, at: Tick) {
        let Some(l) = self.lair_of_key(c.lair) else {
            return;
        };
        if self.outbox.replaying() {
            // The last session's recount only marks its lair out (RP-64 for lairs): the lines
            // it answered with are decided again from the first census of this session.
            self.lairs_in.remove(&l);
            return;
        }
        if self.lairs_in.get(&l).is_some_and(|&e| c.epoch < e) {
            return;
        }
        self.lairs_in.insert(l, c.epoch);
        self.stats.lair_censuses += 1;
        let watched = c.flags & boundary::CENSUS_WATCHED != 0;
        let lp = self.lairs[l as usize].pos;
        let occupants = self.lairs[l as usize].occupants.clone();
        let dist = |s: &Self, b: &boundary::CensusBody| -> f64 {
            let p = s.canon(c.dimension, b.pos.0, b.pos.2);
            flight::dist(
                s.topo(lp.frame),
                (i64::from(p.x), i64::from(p.z)),
                (i64::from(lp.x), i64::from(lp.z)),
            )
        };

        // Which bodies are whose (EQC112), by UUID first (EQC310): the body a dragon of the lair
        // believes is its own; then the lair's own lists (left behind: removed; expected after a
        // reset, or missing: the lair's own, a stray that may come home). An agent names its
        // dragon only for a dragon of this lair owed a body, of the body's species: an agent is
        // not unique across engine lives (a rebuilt engine, and a crash's replay, hand its index
        // out again), so a body whose agent is anything else is a stray.
        let mut owned: BTreeMap<DragonId, Vec<usize>> = BTreeMap::new();
        let mut strays: Vec<usize> = Vec::new();
        let mut removals: Vec<(Uuid, u32)> = Vec::new();
        for (i, b) in c.bodies.iter().enumerate() {
            let u = b.entity;
            let by_body = occupants
                .iter()
                .copied()
                .find(|o| self.dragons.get(*o).is_some_and(|x| x.body == Some(u)));
            if let Some(d) = by_body {
                owned.entry(d).or_default().push(i);
                continue;
            }
            let lair = &self.lairs[l as usize];
            if lair.left_behind.contains(&u) {
                removals.push((u, boundary::DISCARD_AWAY));
                continue;
            }
            if lair.expect.contains(&u) || lair.missing.contains(&u) {
                strays.push(i);
                continue;
            }
            let by_agent = (b.agent != 0)
                .then(|| dragon_of(b.agent))
                .flatten()
                .filter(|d| {
                    occupants.contains(d)
                        && self.dragons.get(*d).is_some_and(|x| {
                            matches!(x.state, State::Roosting)
                                && x.body.is_none()
                                && x.species == b.species
                        })
                });
            match by_agent {
                Some(d) => owned.entry(d).or_default().push(i),
                None => strays.push(i),
            }
        }
        let candidates = self.home_candidates(l, at);

        // What each dragon does.
        let mut plans: Vec<Plan> = Vec::new();
        for &d in &occupants {
            let Some(x) = self.dragons.get(d) else {
                continue;
            };
            let (state, believed) = (x.state, x.body);
            let away = matches!(state, State::Flying(_) | State::Hunting { .. });
            let embodied = x.is_embodied();
            let mut list = owned.remove(&d).unwrap_or_default();
            // One body per dragon (H1): a loaded home body in control is the one; else the
            // closest to the lair.
            list.sort_by(|&a, &b| {
                let (ba, bb) = (&c.bodies[a], &c.bodies[b]);
                let in_control = |x: &boundary::CensusBody| embodied && believed == Some(x.entity);
                in_control(bb)
                    .cmp(&in_control(ba))
                    .then(dist(self, ba).total_cmp(&dist(self, bb)))
                    .then(a.cmp(&b))
            });
            let keep = list.first().copied();
            for &extra in list.iter().skip(1) {
                removals.push((c.bodies[extra].entity, boundary::DISCARD_SECOND_BODY));
            }
            match keep {
                Some(i) if !candidates.contains(&d) => {
                    // A third body of a pair (H2): it is not this dragon's to keep.
                    removals.push((c.bodies[i].entity, boundary::DISCARD_LAIR_FULL));
                    if believed == Some(c.bodies[i].entity) {
                        self.dragons.get_mut(d).expect("live").body = None;
                    }
                }
                Some(i) if matches!(state, State::Roosting) => plans.push(Plan::Adopt(d, i)),
                Some(i) if away => {
                    if watched {
                        plans.push(Plan::Recall(d, i));
                    } else {
                        removals.push((c.bodies[i].entity, boundary::DISCARD_AWAY));
                        self.dragons.get_mut(d).expect("live").body = None;
                    }
                }
                Some(i) => {
                    // Embodied: only the body it believes is matched to it (the one in control).
                    // Any other would be a second body of a dragon embodied elsewhere, a puppet
                    // away or a home body in control: removed, watched or not (H1, EQC311).
                    let e = c.bodies[i].entity;
                    if believed == Some(e) {
                        let x = self.dragons.get_mut(d).expect("live");
                        if x.miss.0 != 0 {
                            x.miss = (0, Tick(0));
                        }
                    } else {
                        removals.push((e, boundary::DISCARD_SECOND_BODY));
                    }
                }
                None => match state {
                    State::Roosting if believed.is_some() => plans.push(Plan::Miss(d)),
                    State::Roosting if !watched && candidates.contains(&d) => {
                        plans.push(Plan::Spawn(d));
                    }
                    _ if away && believed.is_some() => {
                        // Its old body is gone: the dragon is alive and away, no loss.
                        self.dragons.get_mut(d).expect("live").body = None;
                    }
                    _ => {}
                },
            }
        }

        // The bodies the lair remembers as left behind that are not listed are gone; the ones
        // listed are removed above and leave the list at the next census that does not list them.
        {
            let listed: BTreeSet<Uuid> = c.bodies.iter().map(|b| b.entity).collect();
            let lair = &mut self.lairs[l as usize];
            if lair.left_behind.iter().any(|u| !listed.contains(u)) {
                lair.left_behind.retain(|u| listed.contains(u));
                self.site_dirty.insert(l);
            }
        }

        // Strays (EQC113, EQC313): the lair's own by UUID (expected after a reset, or missing)
        // while it has room; any other only into a lair this census found empty, as the untagged
        // path did before S5 (a lair that got a newcomer while it was seen from afar keeps it,
        // and Ice and Fire's generated dragon found later is removed); the rest removed. The
        // lair's own first, then nearest first. A written-off body that turns up at a lair that
        // has refilled stays standing: a stray whose death still presses its old lair (S3's F3
        // and F4).
        let empty = occupants.is_empty();
        let own = |s: &Self, i: usize| {
            let lair = &s.lairs[l as usize];
            let u = c.bodies[i].entity;
            lair.expect.contains(&u) || lair.missing.contains(&u)
        };
        strays.sort_by(|&a, &b| {
            own(self, b)
                .cmp(&own(self, a))
                .then(dist(self, &c.bodies[a]).total_cmp(&dist(self, &c.bodies[b])))
                .then(a.cmp(&b))
        });
        let mut room = self.tun.home_bodies_max.saturating_sub(occupants.len());
        let mut adoptable: Vec<usize> = Vec::new();
        for i in strays {
            let u = c.bodies[i].entity;
            let back = self.lairs[l as usize].missing.contains(&u);
            if room > 0 && (empty || own(self, i)) {
                room -= 1;
                adoptable.push(i);
            } else if !back {
                removals.push((u, boundary::DISCARD_LAIR_FULL));
            }
        }

        // The lines: removals, then adoptions, then spawns (EQC112).
        for (entity, reason) in removals {
            self.outbox
                .push(Cmd::Discard(boundary::Discard { entity, reason }));
            self.stats.discarded += 1;
            self.stats.home_removed += 1;
        }
        let mut writeoffs: Vec<DragonId> = Vec::new();
        for p in &plans {
            match *p {
                Plan::Adopt(d, i) => self.adopt_home(w, d, c.bodies[i].entity, at),
                Plan::Recall(d, i) => {
                    self.stats.home_recalled += 1;
                    self.note(
                        at,
                        "recall",
                        Some(d),
                        "its lair came in with a player near: its trip ends, its body stands"
                            .into(),
                    );
                    self.adopt_home(w, d, c.bodies[i].entity, at);
                }
                Plan::Miss(d) => {
                    let (n, span) = (
                        self.tun.miss_writeoff,
                        self.tun.miss_span_days * TICKS_PER_DAY,
                    );
                    let x = self.dragons.get_mut(d).expect("live");
                    if x.miss.0 == 0 {
                        x.miss = (1, at);
                    } else {
                        x.miss.0 = x.miss.0.saturating_add(1);
                    }
                    if x.miss.0 >= n && at.0.saturating_sub(x.miss.1.0) >= span {
                        writeoffs.push(d);
                    }
                }
                Plan::Spawn(_) => {}
            }
        }
        // Dragons found, their misses start over.
        for p in &plans {
            if let Plan::Adopt(d, _) | Plan::Recall(d, _) = *p
                && self.dragons[d].miss.0 != 0
            {
                self.dragons.get_mut(d).expect("live").miss = (0, Tick(0));
            }
        }
        for i in adoptable {
            self.adopt_stray(w, l, &c.bodies[i], at);
        }
        for p in &plans {
            if let Plan::Spawn(d) = *p {
                self.home_spawn(w, d, at);
            }
        }
        for d in writeoffs {
            self.write_off(w, d, at);
        }

        // After a reset: the bodies the ledger's row names that this census did not find.
        self.expect_misses(l, c, at);
        // A census that leaves nothing missing has accounted for the gap the row reported.
        let lair = &self.lairs[l as usize];
        if lair.gap
            && lair.expect.is_empty()
            && !lair.occupants.iter().any(|d| {
                self.dragons
                    .get(*d)
                    .is_some_and(|x| x.miss.0 != 0 && x.body.is_some())
            })
        {
            self.lairs[l as usize].gap = false;
            self.site_dirty.insert(l);
        }
        self.site_dirty.insert(l);
    }

    /// The lair's census epoch ended: its footprint is no longer all loaded (EQC112).
    pub(crate) fn lair_out(&mut self, o: &boundary::LairOut) {
        let Some(l) = self.lair_of_key(o.lair) else {
            return;
        };
        if self.lairs_in.get(&l) == Some(&o.epoch) {
            self.lairs_in.remove(&l);
        }
    }

    /// Adopts a stray body as an occupant of lair `l` (EQC113): a dragon is made from the body's
    /// own species, age, hunger, gender and variant, as an untagged lair dragon was.
    fn adopt_stray(&mut self, w: &mut World, l: LairId, b: &boundary::CensusBody, at: Tick) {
        let lair = &mut self.lairs[l as usize];
        lair.missing.retain(|u| *u != b.entity);
        lair.expect.retain(|u| *u != b.entity);
        // Its lair found late with no ledger row waits the cap (MZB-1012); its own dragon
        // adopted during that wait shows its history was lost, not its dragon: no pressure.
        if lair.found_unknown && lair.occupants.is_empty() {
            lair.pressure = Pressure::default();
        }
        let pos = self.canon(
            u32::from(self.lairs[l as usize].pos.frame),
            b.pos.0,
            b.pos.2,
        );
        let d = self.add_dragon(
            w,
            NewDragon {
                name: format!("{} dragon {:x}", b.species.name(), b.entity.1 & 0xffff),
                species: b.species,
                gender: b.gender,
                variant: (b.variant % 4) as u8,
                age_ticks: b.age_ticks.max(0) as u64,
                lair: Some(l),
                fullness: b.hunger,
                health: (b.health > 0.0).then_some(b.health),
                seed: Some(fnv1a64(
                    &[b.entity.0.to_le_bytes(), b.entity.1.to_le_bytes()].concat(),
                )),
                at: Some((pos, b.pos.1 as i32)),
            },
            at,
        );
        self.stats.strays_adopted += 1;
        self.adopted.insert(b.entity, d);
        self.adopt_home(w, d, b.entity, at);
    }

    /// Orders the dragon's body `u` adopted as a home body, synced from the engine's closed forms
    /// (EQC108), and makes the dragon embodied at a new epoch. The engine books the body when the
    /// plugin answers (H3), unless it already believed it.
    pub(crate) fn adopt_home(&mut self, w: &mut World, d: DragonId, u: Uuid, at: Tick) {
        let epoch = self.next_epoch(d);
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let (age, hunger, health) = (
            x.age(at).min(i32::MAX as u64) as i32,
            x.fullness_at(&self.tun, at),
            x.health_at(&sp, at),
        );
        let Some(l) = x.lair else {
            return;
        };
        let (lp, ly) = (self.lairs[l as usize].pos, self.lairs[l as usize].y);
        self.outbox.push(Cmd::Adopt(boundary::Adopt {
            entity: u,
            agent: agent_of(d),
            epoch,
            flags: boundary::ADOPT_HAS_HOME | boundary::ADOPT_HOME_BODY,
            home: (lp.x, ly, lp.z),
            age_ticks: age,
            hunger,
            health,
        }));
        self.by_entity.insert(u, d);
        self.pending_home.insert(d);
        self.bump(d);
        self.cancel_leg(w, d);
        let x = self.dragons.get_mut(d).expect("live");
        x.epoch = epoch;
        x.set_health(health, at);
        if x.body != Some(u) {
            x.body = None;
        }
        x.rest = Some((lp, ly));
        x.state = State::Embodied {
            epoch,
            since: at,
            depart_at: Tick(u64::MAX),
            purpose: Purpose::Home,
            harass: false,
        };
        self.stats.home_adopts += 1;
        self.track(d);
        self.site_dirty.insert(l);
    }

    /// Orders a body spawned at the lair for an owed dragon, sleeping, as a home body (EQC116).
    /// Nothing while a replay runs (it embodies: docs/replay-gate.md §6).
    pub(crate) fn home_spawn(&mut self, w: &mut World, d: DragonId, at: Tick) {
        if self.outbox.defer() {
            return;
        }
        let Some(l) = self.dragons[d].lair else {
            return;
        };
        let gated = self.oil_gated(d);
        let (lp, ly) = (self.lairs[l as usize].pos, self.lairs[l as usize].y);
        let epoch = self.next_epoch(d);
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let health = x.health_at(&sp, at);
        let mut flags = boundary::MAT_HAS_HOME | boundary::MAT_SLEEPING | boundary::MAT_HOME_BODY;
        let oil = if gated {
            flags |= boundary::MAT_OIL;
            x.oil_level(&self.tun.oil, at)
        } else {
            0
        };
        let here = (f64::from(lp.x), f64::from(ly), f64::from(lp.z));
        self.outbox.push(Cmd::Materialize(boundary::Materialize {
            agent: agent_of(d),
            epoch,
            flags,
            species: x.species,
            dimension: u32::from(lp.frame),
            age_ticks: x.age(at).min(i32::MAX as u64) as i32,
            pos: here,
            hunger: x.fullness_at(&self.tun, at),
            gender: x.gender,
            variant: u32::from(x.variant),
            health,
            home: (lp.x, ly, lp.z),
            target: here,
            target_player: (0, 0),
            oil,
        }));
        self.stats.materialized += 1;
        self.stats.home_spawns += 1;
        self.pending_home.insert(d);
        self.bump(d);
        self.cancel_leg(w, d);
        let x = self.dragons.get_mut(d).expect("live");
        x.epoch = epoch;
        x.set_health(health, at);
        x.state = State::Embodied {
            epoch,
            since: at,
            depart_at: Tick(u64::MAX),
            purpose: Purpose::Home,
            harass: false,
        };
        x.rest = Some((lp, ly));
        self.track(d);
        self.note(
            at,
            "materialize",
            Some(d),
            "its owed home body is spawned".into(),
        );
    }

    /// The plugin answered an adoption, a spawn or a hand-back with the home body (EQC122). A home
    /// body is the Ice and Fire AI's while it is loaded (EQC117), so whatever the puppet it was
    /// had booked goes (a hand-back's puppet may have been passing or harassing): no departure,
    /// purpose home, the plan bumped so a departure already booked is stale (EQC315). A new body
    /// starts with no misses (EQC320).
    pub(crate) fn home_answered(&mut self, d: DragonId, entity: Uuid) {
        self.pending_home.remove(&d);
        self.bump(d);
        let x = self.dragons.get_mut(d).expect("live");
        x.body = Some(entity);
        if let State::Embodied { epoch, since, .. } = x.state {
            x.state = State::Embodied {
                epoch,
                since,
                depart_at: Tick(u64::MAX),
                purpose: Purpose::Home,
                harass: false,
            };
        }
        let l = x.lair;
        self.adopted.insert(entity, d);
        if let Some(l) = l {
            self.site_dirty.insert(l);
        }
    }

    /// Writes a dragon off whose home body no census found (EQC114): its body is missing, it is
    /// lost to the sim, a player's loss when rtsim ran through and Other after a gap.
    fn write_off(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        let (Some(l), Some(u)) = (x.lair, x.body) else {
            return;
        };
        let other = self.lairs[l as usize].gap;
        let actor = x.actor;
        self.stats.home_missing += 1;
        self.note(
            at,
            "missing",
            Some(d),
            format!(
                "its home body was not found by {} censuses: written off ({})",
                self.tun.miss_writeoff,
                if other { "other" } else { "a player's loss" }
            ),
        );
        // Clear first: a missing body is no body left behind.
        self.dragons.get_mut(d).expect("live").body = None;
        if !other {
            self.press(d, at);
        }
        if w.knowledge.notable(actor).is_some() {
            w.knowledge.die(actor, at);
        }
        self.remove(w, d, at);
        self.lairs[l as usize].lose_body(u);
        if other
            && self.lairs[l as usize].occupants.is_empty()
            && self.lairs[l as usize].vacant_since.is_some()
        {
            // No pressure, so the wait is the cap's (§2.3).
            let cap = self.tun.renewal().cap_ticks();
            self.start_wait_for(l, cap, at);
        }
        self.track_lair(l);
    }

    /// The bodies a reset's ledger row named that a census does not find (EQC119).
    fn expect_misses(&mut self, l: LairId, c: &boundary::LairCensus, at: Tick) {
        if self.lairs[l as usize].expect.is_empty() {
            if self.lairs[l as usize].miss.0 != 0 {
                self.lairs[l as usize].miss = (0, Tick(0));
            }
            return;
        }
        let (n, span) = (
            self.tun.miss_writeoff,
            self.tun.miss_span_days * TICKS_PER_DAY,
        );
        let listed: BTreeSet<Uuid> = c.bodies.iter().map(|b| b.entity).collect();
        let lair = &mut self.lairs[l as usize];
        lair.expect.retain(|u| !listed.contains(u));
        if lair.miss.0 == 0 {
            lair.miss = (1, at);
        } else {
            lair.miss.0 = lair.miss.0.saturating_add(1);
        }
        if lair.miss.0 >= n && at.0.saturating_sub(lair.miss.1.0) >= span {
            // Given up: the lair's wait runs on to the cap, no pressure (Other).
            lair.expect.clear();
            lair.miss = (0, Tick(0));
        }
    }

    /// A home body went past the leash, or changed dimension: a puppet now (EQC109).
    pub(crate) fn left_home(&mut self, h: &boundary::DragonLeftHome, at: Tick) {
        let Some(d) = self.embodied(h.agent, h.epoch) else {
            return;
        };
        let here = self.canon(h.dimension, h.pos.0, h.pos.2);
        let x = self.dragons.get_mut(d).expect("live");
        x.body = None;
        x.rest = Some((here, h.pos.1 as i32));
        let l = x.lair;
        self.pending_home.remove(&d);
        self.stats.home_left += 1;
        if let Some(l) = l {
            self.site_dirty.insert(l);
        }
        self.note(
            at,
            "leash",
            Some(d),
            "left its lair: a puppet from here".into(),
        );
    }

    /// A home body whose lair went out, or whose chunk unloaded, stays in the world and vanilla
    /// saves it (reason 8, EQC117): the dragon is abstract again, roosting, its body believed.
    pub(crate) fn home_saved(
        &mut self,
        w: &mut World,
        d: DragonId,
        m: &boundary::Dematerialized,
        at: Tick,
    ) {
        let full = self.tun.full;
        {
            let x = self.dragons.get_mut(d).expect("live");
            x.set_health(m.health, at);
            x.set_fullness(m.hunger.clamp(0, full), at);
        }
        self.by_entity.retain(|_, v| *v != d);
        self.pending_home.remove(&d);
        self.stats.home_saved += 1;
        self.note(
            at,
            "saved",
            Some(d),
            "its home body stays; vanilla saves it".into(),
        );
        self.roost(w, d, at);
    }

    /// A settled puppet at a lair that is in becomes a home body (EQC110): ordered once, from its
    /// state record.
    pub(crate) fn maybe_hand_back(&mut self, d: DragonId, s: &boundary::State) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        let (Some(l), None) = (x.lair, x.body) else {
            return;
        };
        if !matches!(x.state, State::Embodied { .. })
            || x.visitor.is_some()
            || s.flags & boundary::STATE_FLYING != 0
            || !self.lairs_in.contains_key(&l)
            || self.pending_home.contains(&d)
            || self.homed_count(l) >= self.tun.home_bodies_max
        {
            return;
        }
        let (lp, ly) = (self.lairs[l as usize].pos, self.lairs[l as usize].y);
        let here = self.canon(s.dimension, s.pos.0, s.pos.2);
        if here.frame != lp.frame
            || flight::dist(
                self.topo(lp.frame),
                (i64::from(here.x), i64::from(here.z)),
                (i64::from(lp.x), i64::from(lp.z)),
            ) > self.tun.hand_back_blocks
            || (s.pos.1 - f64::from(ly)).abs() > self.tun.hand_back_height
        {
            return;
        }
        let State::Embodied { epoch, .. } = x.state else {
            return;
        };
        self.outbox.push(Cmd::HandBack(boundary::HandBack {
            agent: agent_of(d),
            epoch,
        }));
        self.pending_home.insert(d);
        self.stats.home_hand_backs += 1;
    }

    /// A home body died that the engine had no body of the epoch for (EQC131): one it believed
    /// standing and saved, or one it wrote off as missing (a stray's death still presses its old
    /// lair, S3's F4).
    pub(crate) fn unmet_death(&mut self, w: &mut World, dd: &boundary::Died, at: Tick) {
        let Some(l) = self.lair_of_key(dd.lair) else {
            return;
        };
        let u = dd.entity;
        let mine = self.lairs[l as usize]
            .occupants
            .iter()
            .copied()
            .find(|o| self.dragons.get(*o).is_some_and(|x| x.body == Some(u)));
        if let Some(d) = mine {
            self.dragons.get_mut(d).expect("live").body = None;
            if LossCause::from_credit(dd.killer_kind.credit(), true) == LossCause::Player {
                self.press(d, at);
            }
            let killer =
                (dd.killer_kind == AttackerKind::Player).then(|| self.player(w, dd.killer));
            let pos = (
                self.canon(dd.dimension, dd.pos.0, dd.pos.2),
                dd.pos.1 as i32,
            );
            self.kill(w, d, killer, pos, at);
        } else if self.lairs[l as usize].missing.contains(&u) {
            let h = self.tun.respawn_half_life_days as f32;
            let lair = &mut self.lairs[l as usize];
            lair.missing.retain(|m| *m != u);
            lair.pressure.bump(day_num(at), 1.0, h);
            self.site_dirty.insert(l);
            self.stats.stray_deaths += 1;
        } else {
            let lair = &mut self.lairs[l as usize];
            if lair.left_behind.contains(&u) {
                lair.left_behind.retain(|m| *m != u);
                self.site_dirty.insert(l);
            }
        }
    }

    /// Checks the home bodies' invariants (EQC105's H1 and H2, and the lists' disjointness).
    pub(crate) fn check_homes(&self) -> Result<(), String> {
        let mut seen: BTreeMap<Uuid, DragonId> = BTreeMap::new();
        for (h, x) in self.dragons.iter() {
            let Some(u) = x.body else {
                continue;
            };
            if x.lair.is_none() {
                return Err(format!(
                    "dragon {} holds a home body but has no lair",
                    h.to_bits()
                ));
            }
            if let Some(o) = seen.insert(u, h) {
                return Err(format!(
                    "dragons {} and {} hold the same body {u:?}",
                    o.to_bits(),
                    h.to_bits()
                ));
            }
        }
        for (i, l) in self.lairs.iter().enumerate() {
            let held = l
                .occupants
                .iter()
                .filter(|d| self.dragons.get(**d).is_some_and(|x| x.body.is_some()))
                .count();
            if held > self.tun.home_bodies_max {
                return Err(format!("lair {i} holds {held} home bodies"));
            }
            for u in l.left_behind.iter().chain(&l.missing) {
                if seen.contains_key(u) {
                    return Err(format!("lair {i} lists the standing body {u:?} as gone"));
                }
            }
            if l.left_behind.len() > crate::dragon::LEFT_BEHIND_KEPT
                || l.missing.len() > crate::dragon::MISSING_KEPT
                || l.expect.len() > crate::dragon::MISSING_KEPT
            {
                return Err(format!("lair {i} keeps too many bodies"));
            }
        }
        for l in self.lairs_in.keys() {
            if *l as usize >= self.lairs.len() {
                return Err(format!("lair {l} is in but does not exist"));
            }
        }
        for d in &self.pending_home {
            if !self.embodied.contains(d) {
                return Err(format!(
                    "dragon {} awaits a body but is not embodied",
                    d.to_bits()
                ));
            }
        }
        Ok(())
    }
}
