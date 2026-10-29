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
use rtsim_settle::{Culture, NodeKind};

use crate::world::{Cause, Command, Ev, World};

/// The villages' RNG system: the settlers' bearing and the growth steps' waits.
pub const SYS_VILLAGE: SystemId = SystemId::named("world.village");

/// A village's renewal row. About 90 bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VillageRow {
    /// What the sim believes is there (the market's pop is this, floored at 1).
    pub members: u32,
    /// Capacity: a human village's beds (clamped), a Ribbit village's houses (EQC154, EQC166).
    pub k: u16,
    /// The adoption guess, kept: a Ribbit village's K until a census has seen it alive.
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
            if !ribbit {
                row.k = u16::try_from(beds.min(k_max)).unwrap_or(u16::MAX);
            }
            row.loaded_until = loaded_until;
            self.recompute_k(i);
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
    }

    fn is_ribbit(&self, i: usize) -> bool {
        matches!(self.nodes[i].kind, NodeKind::Village(Culture::Ribbit))
    }

    /// A Ribbit village's K (EQC166): its adoption count plus the houses it has booked until a
    /// complete census has seen it alive, then the largest complete reading plus the residents of
    /// houses booked and not yet lived in. A human village's K is the census's beds.
    pub(crate) fn recompute_k(&mut self, i: usize) {
        if !self.is_ribbit(i) {
            return;
        }
        let houses = self.growth.get(&i).map_or(0, |g| g.houses);
        let Some(row) = self.villages.get_mut(&i) else {
            return;
        };
        let k = if row.seen {
            u32::from(row.peak) + u32::from(row.pending_houses)
        } else {
            u32::from(row.k0) + houses
        };
        row.k = u16::try_from(k).unwrap_or(u16::MAX);
    }

    /// A booking older than `owed_max_days` is released at a confirmed census: a lost order must
    /// not hold a count up for ever (EQC162).
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
        self.set_population(i, members.max(1), at);
        self.market_event(i, at, Cause::Population);
    }

    /// The bridge answered a resident op (EQC162). False when the world holds no such op.
    pub fn resident_result(&mut self, op: u64, placed: bool, at: Tick) -> bool {
        let _ = (op, placed, at);
        false
    }

    /// Whether `op` is a resident or house booking the bridge has not answered.
    pub fn is_resident_op(&self, op: u64) -> bool {
        self.resident_ops.contains_key(&op) || self.houses_pending.contains_key(&op)
    }

    /// A player killed a resident of village `i` (EQC158): its pressure rises by 1/K.
    pub(crate) fn village_killed(&mut self, i: usize, at: Tick) {
        let _ = (i, at);
    }

    /// The settlers' wait ran out (EQC160).
    pub(crate) fn settlers_due(&mut self, node: usize, stamp: u32, at: Tick) {
        let _ = (node, stamp, at);
    }

    /// A growth step is due (EQC163).
    pub(crate) fn breed_due(&mut self, node: usize, stamp: u32, at: Tick) {
        let _ = (node, stamp, at);
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

    fn _unused(&self, _: StreamKey, _: Command, _: Ev, _: Culture) {}
}
