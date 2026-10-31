// SPDX-License-Identifier: AGPL-3.0-or-later
//! Villages renew (EQUILIBRIUM.md §2.4.3 and §2.4.4, slice S6, Ben's EQ-1 (a);
//! docs/overnight-choices-equilibrium.md EQC150 to EQC183).
//!
//! Every adopted village has a [`VillageRow`] when [`crate::WorldConfig::renewal`] is on: the count
//! the sim believes is there (`members`, 0 allowed, where the market keeps a floor of 1), its K, the
//! bookings Java has not answered (`owed`), a pressure from players' kills and a settlers' wait.
//!
//! - **The census moves the count both ways** ([`World::census_village`]): a complete census (Java
//!   saw every chunk of the village's reach with its entities) sets it, an incomplete one only
//!   raises it (EQC150).
//! - **Settlers** ([`Ev::Settlers`]): a village the engine has seen alive, holding fewer than two
//!   with at least two beds, gets a pair after `min(192, 16 x 2^P)` days (EQC159, EQC160).
//! - **Growth while unloaded** ([`Ev::Breed`]): a human village grows toward its beds by scheduled
//!   steps of the logistic; Java places the bodies at the next unwatched load (EQC163).
//! - **Bookings**: every arrival is a SPAWN_RESIDENT op, and the bridge's answer
//!   ([`World::resident_result`]) settles the booking (EQC162).

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::renewal::Pressure;
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_travel::caravan::Body;

use crate::ribbits::{SpawnResident, identity_op};
use crate::world::{Cause, Command, Ev, World};

/// Fewer than this many in a village and vanilla cannot breed any more (EQC159).
const MIN_PAIR: u32 = 2;
/// How far the settlers come from, in blocks (EQC161).
const SOURCE_BLOCKS: f64 = 192.0;
/// Villages within this many blocks are the neighbours settlers come away from (EQC161).
const NEIGHBOURS_WITHIN: u64 = 3_000;
/// A centroid nearer than this is no direction (EQC161).
const AWAY_MIN: f64 = 150.0;

/// The villages' RNG system: the settlers' bearing and the growth steps' waits.
pub const SYS_VILLAGE: SystemId = SystemId::named("world.village");

/// A village's renewal row. About 90 bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VillageRow {
    /// What the sim believes is there (the market's pop is this, floored at 1).
    pub members: u32,
    /// Capacity: a human village's beds (clamped), a Ribbit village's houses (EQC154, EQC166).
    pub k: u16,
    /// The adoption guess, and the residents of the houses the bridge has placed since: a Ribbit
    /// village's K until a census has seen it alive.
    pub k0: u16,
    /// Bookings Java has not answered, and the tick of the oldest (`u64::MAX`: none).
    pub owed: u16,
    pub owed_at: u64,
    /// A complete census has counted a villager here (EQC153).
    pub seen: bool,
    /// The last complete reading (`u32::MAX`: none yet).
    pub seen_r: u32,
    /// The largest complete reading, and a Ribbit village's size after a wipe.
    pub peak: u16,
    /// A Ribbit village's houses booked and not yet lived in.
    pub pending_houses: u16,
    pub pressure: Pressure,
    /// When the settlers' wait ends (`u64::MAX`: none running) and its stamp.
    pub wait_due: u64,
    pub wait_stamp: u32,
    /// When the next growth step is due (`u64::MAX`: none booked) and its stamp.
    pub breed_due: u64,
    pub breed_stamp: u32,
    /// The village reads loaded until this tick (EQC164).
    pub loaded_until: u64,
    /// Arrival batches and growth draws so far: op identity and the draws' blocks.
    pub serial: u32,
}

impl VillageRow {
    /// A row at its adoption guess.
    pub fn at_guess(pop: u32) -> Self {
        let k = u16::try_from(pop).unwrap_or(u16::MAX);
        VillageRow {
            members: pop,
            k,
            k0: k,
            owed: 0,
            owed_at: u64::MAX,
            seen: false,
            seen_r: u32::MAX,
            peak: 0,
            pending_houses: 0,
            pressure: Pressure::default(),
            wait_due: u64::MAX,
            wait_stamp: 0,
            breed_due: u64::MAX,
            breed_stamp: 0,
            loaded_until: 0,
            serial: 0,
        }
    }
}

/// Where a booked resident comes from and how it arrives (SPAWN_RESIDENT's flags, EQC169).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrive {
    /// A grown house's resident, at the house's slot (today's order).
    House,
    /// A Ribbit refill: at the village's meeting point (EQC165).
    Refill,
    /// A human village's growth while it was unloaded: at the meeting point (EQC163).
    Growth,
    /// A settler from beyond the edge, coming from this point if the village is watched (EQC161).
    Settler { x: i32, z: i32 },
}

impl World {
    /// The renewal row of village node `i`, when the world keeps rows.
    pub fn village_row(&self, i: usize) -> Option<&VillageRow> {
        self.villages.get(&i)
    }

    /// What the sim believes a village holds: its row's count, else its settlement's pop.
    pub fn village_members(&self, i: usize) -> u32 {
        match self.villages.get(&i) {
            Some(r) => r.members,
            None => self.settlements.get(self.nodes[i].settlement).map_or(0, |s| s.pop),
        }
    }

    /// Makes a village's row at its adoption guess (a no-op for anything but a village, and with
    /// renewal off).
    pub(crate) fn make_village_row(&mut self, i: usize) {
        if !self.cfg.renewal || !matches!(self.nodes[i].kind, NodeKind::Village(_)) {
            return;
        }
        let pop = self.nodes[i].local.pop;
        self.villages.insert(i, VillageRow::at_guess(pop));
    }

    /// A census of village `i` (EQC150): `residents` counted, `beds` found, `complete` when Java saw
    /// every chunk of the village's reach with its entities.
    pub fn census_village(&mut self, i: usize, residents: u32, beds: u32, complete: bool, at: Tick) {
        let Some(row) = self.villages.get(&i).copied() else {
            return;
        };
        let ribbit = self.is_ribbit(i);
        // A census that reached the engine says the village was loaded when it was taken (EQC164).
        let loaded_until = at.0.saturating_add(self.cfg.village_loaded_ticks);
        if complete {
            self.release_stale_owed(i, at);
            let owed = u32::from(self.villages[&i].owed);
            let k_max = self.cfg.village_k_max;
            let row = self.villages.get_mut(&i).expect("a row");
            row.seen_r = residents;
            row.seen |= residents > 0;
            row.peak = row.peak.max(u16::try_from(residents).unwrap_or(u16::MAX));
            let k_before = row.k;
            if !ribbit {
                row.k = u16::try_from(beds.min(k_max)).unwrap_or(u16::MAX);
            }
            row.loaded_until = loaded_until;
            self.recompute_k(i);
            if self.villages[&i].k != k_before {
                self.breed_cancel(i);
            }
            // A human village's bookings are in its count; a Ribbit village's are not, until the
            // bridge answers them (EQC162).
            let target = if ribbit { residents } else { residents + owed };
            self.set_members(i, target, at);
        } else {
            let owed = u32::from(row.owed);
            let target = if ribbit { residents } else { residents + owed };
            self.villages.get_mut(&i).expect("a row").loaded_until = loaded_until;
            if target > row.members {
                self.set_members(i, target, at);
            }
        }
        self.reconsider(i, at);
    }

    fn is_ribbit(&self, i: usize) -> bool {
        matches!(self.nodes[i].kind, NodeKind::Village(Culture::Ribbit))
    }

    /// A Ribbit village's K (EQC166): its adoption count plus the houses the bridge has placed
    /// (`k0` holds both: [`World::house_placed`] adds a house's residents to it) until a complete
    /// census has seen it alive, then the largest complete reading plus the residents of houses
    /// booked and not yet lived in. A house the bridge could not place is room for nobody. A human
    /// village's K is the census's beds.
    pub(crate) fn recompute_k(&mut self, i: usize) {
        if !self.is_ribbit(i) {
            return;
        }
        let Some(row) = self.villages.get_mut(&i) else {
            return;
        };
        let k = if row.seen {
            u32::from(row.peak) + u32::from(row.pending_houses)
        } else {
            u32::from(row.k0)
        };
        row.k = u16::try_from(k).unwrap_or(u16::MAX);
    }

    /// A booking older than `owed_max_days` is released at a confirmed census: a lost order must
    /// not hold a count up for ever (EQC162). The village's booked houses are forgotten with its
    /// resident bookings; a house the bridge places after that is nobody's booking, and K counts
    /// it when a census finds its resident.
    fn release_stale_owed(&mut self, i: usize, at: Tick) {
        let limit = self.cfg.owed_max_days * TICKS_PER_DAY;
        let Some(row) = self.villages.get_mut(&i) else {
            return;
        };
        if row.owed == 0 || row.owed_at == u64::MAX || at.0.saturating_sub(row.owed_at) <= limit {
            return;
        }
        self.stats.lapsed += u64::from(row.owed);
        row.owed = 0;
        row.owed_at = u64::MAX;
        row.pending_houses = 0;
        self.resident_ops.retain(|_, (n, _)| *n != i);
        // The booked houses go with them: an answer that came for one later (NOT_FREE, or HOUSE_PLACED with
        // its resident's order) would take from the bookings made since, and let an extra refill pass K.
        self.houses_pending.retain(|_, n| *n != i);
        self.recompute_k(i);
    }

    /// Sets what a village holds (EQC155): the one place its count changes. The market keeps a floor
    /// of 1 and re-reads its flows.
    pub fn set_members(&mut self, i: usize, members: u32, at: Tick) {
        let Some(row) = self.villages.get_mut(&i) else {
            return;
        };
        if row.members == members {
            return;
        }
        row.members = members;
        self.breed_cancel(i);
        self.set_population(i, members.max(1), at);
        self.market_event(i, at, Cause::Population);
    }

    /// The bridge answered a resident op (EQC162). False when the world holds no such op.
    pub fn resident_result(&mut self, op: u64, placed: bool, at: Tick) -> bool {
        if let Some(&node) = self.houses_pending.get(&op) {
            // A booked house the bridge could not place (NOT_FREE, FAILED): its resident's booking
            // lapses, the house stays booked (`houses` is what the clock counts).
            if placed || !self.villages.contains_key(&node) {
                return false;
            }
            self.houses_pending.remove(&op);
            let per = u16::try_from(self.cfg.growth_pop_per_house).unwrap_or(1);
            let row = self.villages.get_mut(&node).expect("a row");
            row.owed = row.owed.saturating_sub(per);
            row.pending_houses = row.pending_houses.saturating_sub(per);
            if row.owed == 0 {
                row.owed_at = u64::MAX;
            }
            self.stats.lapsed += u64::from(per);
            self.recompute_k(node);
            self.reconsider(node, at);
            return true;
        }
        let Some((node, house)) = self.resident_ops.remove(&op) else {
            return false;
        };
        let ribbit = self.is_ribbit(node);
        let Some(row) = self.villages.get_mut(&node) else {
            return true;
        };
        row.owed = row.owed.saturating_sub(1);
        if row.owed == 0 {
            row.owed_at = u64::MAX;
        }
        if house {
            row.pending_houses = row.pending_houses.saturating_sub(1);
        }
        let members = row.members;
        if placed {
            if ribbit {
                // Pop on spawn (EQC165): the resident exists now.
                let row = self.villages.get_mut(&node).expect("a row");
                let m = members + 1;
                row.peak = row.peak.max(u16::try_from(m).unwrap_or(u16::MAX));
                self.recompute_k(node);
                self.set_members(node, m, at);
            }
        } else {
            self.stats.lapsed += 1;
            if !ribbit {
                // The abstract booking lapses: the bridge would not put a villager there (EQC179).
                self.set_members(node, members.saturating_sub(1), at);
            }
            self.recompute_k(node);
        }
        self.reconsider(node, at);
        true
    }

    /// Whether the bridge's answer `placed` to `op` is one the world acts on: a resident booking's
    /// either way, a booked house's only when it could not be placed (a placed house is
    /// HOUSE_PLACED's business).
    pub fn wants_resident_result(&self, op: u64, placed: bool) -> bool {
        self.resident_ops.contains_key(&op) || (!placed && self.houses_pending.contains_key(&op))
    }

    /// A player killed a resident of village `i` (EQC158): its pressure rises by 1/K.
    pub(crate) fn village_killed(&mut self, i: usize, at: Tick) {
        let half = self.cfg.village_rule.half_life_days;
        let Some(row) = self.villages.get_mut(&i) else {
            return;
        };
        let day = (at.0 / TICKS_PER_DAY) as u32;
        row.pressure.bump(day, 1.0 / f32::from(row.k.max(1)), half);
        self.stats.kills += 1;
    }

    /// What a village counts toward the settlers' threshold: a Ribbit village's bookings are not
    /// members yet (EQC159).
    fn effective(&self, i: usize) -> u32 {
        let r = &self.villages[&i];
        r.members + if self.is_ribbit(i) { u32::from(r.owed) } else { 0 }
    }

    /// A village is short for settlers when the engine has seen it alive, it has beds for a pair
    /// and fewer than two are in it (EQC159).
    fn short_for_settlers(&self, i: usize) -> bool {
        let r = &self.villages[&i];
        r.seen && u32::from(r.k) >= MIN_PAIR && self.effective(i) < MIN_PAIR
    }

    /// After anything that moved a village's count, K or bookings: start the settlers' wait if it
    /// is short and none runs, drop a wait it no longer needs, and keep the growth step and the
    /// Ribbit clock right (EQC159).
    pub(crate) fn reconsider(&mut self, i: usize, at: Tick) {
        if !self.villages.contains_key(&i) {
            return;
        }
        let short = self.short_for_settlers(i);
        let (running, rule) = (self.villages[&i].wait_due != u64::MAX, self.cfg.village_rule);
        if short && !running {
            let day = (at.0 / TICKS_PER_DAY) as u32;
            let row = self.villages.get_mut(&i).expect("a row");
            let delay = rule.delay_ticks(row.pressure, day);
            let due = at.0 + delay;
            row.wait_stamp += 1;
            row.wait_due = due;
            let stamp = row.wait_stamp;
            self.schedule(Tick(due), Ev::Settlers { node: i, stamp });
            self.stats.emptied += 1;
            let (place, id) = (self.node_at(i, at), self.node_id(i));
            self.tape.event(
                rtsim_archive::kinds::SETTLE_EMPTIED,
                place,
                id,
                0,
                (delay / TICKS_PER_DAY) as i64,
            );
        } else if !short && running {
            let row = self.villages.get_mut(&i).expect("a row");
            row.wait_stamp += 1;
            row.wait_due = u64::MAX;
        }
        self.breed_ensure(i, at);
        if self.is_ribbit(i) {
            // Its growth clock may have a refill to run, or none left (EQC165).
            self.growth_check(i, at);
        }
    }

    /// Whether a human village is growing by the sim's steps: seen alive, a pair or more in it,
    /// beds free (EQC163).
    fn breeds(&self, i: usize) -> bool {
        let r = &self.villages[&i];
        self.cfg.renewal
            && !self.is_ribbit(i)
            && r.seen
            && r.members >= MIN_PAIR
            && r.members < u32::from(r.k)
    }

    /// Books the next growth step when the village breeds and none is booked, and drops one it no
    /// longer needs. The exponential wait is memoryless, so a fresh draw at the new count is exact.
    fn breed_ensure(&mut self, i: usize, at: Tick) {
        let want = self.breeds(i);
        let row = self.villages.get_mut(&i).expect("a row");
        if !want {
            if row.breed_due != u64::MAX {
                row.breed_stamp += 1;
                row.breed_due = u64::MAX;
            }
            return;
        }
        if row.breed_due != u64::MAX {
            return;
        }
        row.serial += 1;
        let (m, k) = (f64::from(row.members), f64::from(row.k));
        let rate = self.cfg.village_growth_per_day * m * (1.0 - m / k);
        if rate <= 0.0 {
            return;
        }
        let key = self.nodes[i].key;
        let row = self.villages.get_mut(&i).expect("a row");
        let draw = StreamKey::new(self.cfg.seed, SYS_VILLAGE, key).block(u64::from(row.serial), 1)[0];
        let u = ((draw >> 11) as f64 + 1.0) / (1u64 << 53) as f64;
        let ticks = ((-u.ln() / rate) * TICKS_PER_DAY as f64).round().max(1.0) as u64;
        let due = at.0.saturating_add(ticks);
        row.breed_stamp += 1;
        row.breed_due = due;
        let stamp = row.breed_stamp;
        self.schedule(Tick(due), Ev::Breed { node: i, stamp });
    }

    /// A village's count or K changed: the booked growth step is for the old one.
    fn breed_cancel(&mut self, i: usize) {
        if let Some(row) = self.villages.get_mut(&i)
            && row.breed_due != u64::MAX
        {
            row.breed_stamp += 1;
            row.breed_due = u64::MAX;
        }
    }

    /// The settlers' wait ran out (EQC160).
    pub(crate) fn settlers_due(&mut self, node: usize, stamp: u32, at: Tick) {
        let Some(row) = self.villages.get_mut(&node) else {
            return;
        };
        if row.wait_stamp != stamp || row.wait_due == u64::MAX {
            return;
        }
        row.wait_due = u64::MAX;
        if !self.short_for_settlers(node) {
            return;
        }
        let eff = self.effective(node);
        let k = u32::from(self.villages[&node].k);
        let n = self.cfg.settlers_pair.min(k.saturating_sub(eff));
        if n == 0 {
            return;
        }
        let ribbit = self.is_ribbit(node);
        let row = self.villages.get_mut(&node).expect("a row");
        row.serial += 1;
        let serial = row.serial;
        let members = row.members;
        let (x, z) = self.settlers_from(node, serial);
        let key = self.nodes[node].key;
        let seed = StreamKey::new(self.cfg.seed, SYS_VILLAGE, key).block(u64::from(serial), 0)[0];
        // A human village books them abstractly (EQC160): the count rises now, Java places the
        // bodies at the next unwatched load. A Ribbit village's count rises at the answers.
        let row = self.villages.get_mut(&node).expect("a row");
        row.owed = row.owed.saturating_add(n as u16);
        if row.owed_at == u64::MAX {
            row.owed_at = at.0;
        }
        if !ribbit {
            self.set_members(node, members + n, at);
        }
        let body = if ribbit { Body::RibbitResident } else { Body::VillagerResident };
        for slot in 0..n {
            let op = identity_op(seed, u64::from(slot) + 1);
            self.outbox.push(Command::SpawnResident(SpawnResident {
                op,
                village: key,
                house: 0,
                body,
                arrive: Arrive::Settler { x, z },
            }));
            self.resident_ops.insert(op, (node, false));
        }
        self.stats.settlers[usize::from(ribbit)] += u64::from(n);
        let (place, id) = (self.node_at(node, at), self.node_id(node));
        self.tape.event(
            rtsim_archive::kinds::SETTLE_SETTLERS,
            place,
            id,
            0,
            i64::from(n),
        );
        self.reconsider(node, at);
    }

    /// Where a family comes from (EQC161): the point 192 blocks out on the side away from the
    /// other villages within 3,000 blocks, else along a bearing drawn from the village's stream.
    /// The draw is always made, so the stream does not depend on which is used.
    fn settlers_from(&self, node: usize, serial: u32) -> (i32, i32) {
        let me = self.nodes[node].pos;
        let key = self.nodes[node].key;
        let draw = StreamKey::new(self.cfg.seed, SYS_VILLAGE, key).block(u64::from(serial), 2)[0];
        let random = (draw >> 11) as f64 / (1u64 << 53) as f64 * std::f64::consts::TAU;
        let (mut cx, mut cz, mut n) = (0.0f64, 0.0f64, 0u32);
        for (j, o) in self.nodes.iter().enumerate() {
            if j == node || !matches!(o.kind, NodeKind::Village(_)) || o.pos.frame != me.frame {
                continue;
            }
            if self.pos_dist(me, o.pos).is_some_and(|d| d <= NEIGHBOURS_WITHIN) {
                // The neighbour's nearest image, so a village across a seam counts on its side.
                let (ox, oz) = self.topo(me.frame).toward(
                    (i64::from(me.x), i64::from(me.z)),
                    (i64::from(o.pos.x), i64::from(o.pos.z)),
                );
                cx += (ox - i64::from(me.x)) as f64;
                cz += (oz - i64::from(me.z)) as f64;
                n += 1;
            }
        }
        let away = if n > 0 {
            let (mx, mz) = (cx / f64::from(n), cz / f64::from(n));
            (mx.hypot(mz) >= AWAY_MIN).then(|| (-mz).atan2(-mx))
        } else {
            None
        };
        let a = away.unwrap_or(random);
        let p = self.canonical_pos(Pos::new(
            me.frame,
            me.x + (SOURCE_BLOCKS * a.cos()).round() as i32,
            me.z + (SOURCE_BLOCKS * a.sin()).round() as i32,
        ));
        (p.x, p.z)
    }

    /// A growth step is due (EQC163).
    pub(crate) fn breed_due(&mut self, node: usize, stamp: u32, at: Tick) {
        let Some(row) = self.villages.get_mut(&node) else {
            return;
        };
        if row.breed_stamp != stamp || row.breed_due == u64::MAX {
            return;
        }
        row.breed_due = u64::MAX;
        if !self.breeds(node) {
            return;
        }
        if self.villages[&node].loaded_until > at.0 {
            // Loaded: vanilla's breeding is what runs there (EQC164). Draw the next wait.
            self.breed_ensure(node, at);
            return;
        }
        let row = self.villages.get_mut(&node).expect("a row");
        row.serial += 1;
        let (serial, members) = (row.serial, row.members);
        row.owed = row.owed.saturating_add(1);
        if row.owed_at == u64::MAX {
            row.owed_at = at.0;
        }
        let key = self.nodes[node].key;
        let seed = StreamKey::new(self.cfg.seed, SYS_VILLAGE, key).block(u64::from(serial), 0)[0];
        let op = identity_op(seed, 1);
        self.outbox.push(Command::SpawnResident(SpawnResident {
            op,
            village: key,
            house: 0,
            body: Body::VillagerResident,
            arrive: Arrive::Growth,
        }));
        self.resident_ops.insert(op, (node, false));
        self.stats.grown += 1;
        let (place, id) = (self.node_at(node, at), self.node_id(node));
        self.tape
            .event(rtsim_archive::kinds::SETTLE_GREW, place, id, 0, 1);
        self.set_members(node, members + 1, at);
        self.reconsider(node, at);
    }

    /// A village's pressure now.
    pub fn village_pressure(&self, i: usize) -> f32 {
        self.villages.get(&i).map_or(0.0, |r| {
            r.pressure.now(self.day(), self.cfg.village_rule.half_life_days)
        })
    }

    pub(crate) fn day(&self) -> u32 {
        (self.now.0 / TICKS_PER_DAY) as u32
    }
}
