// SPDX-License-Identifier: AGPL-3.0-or-later
//! Notables (PLAN.md §5.7, C24, C25): individuals with a history a player could observe, such as
//! the trader of a caravan a player robbed. A notable's memories are 8-byte slots (a report handle
//! and a salience), its feelings are its holder's sentiment table, and its temperament is derived
//! from its seed, never stored.
//!
//! **Age and death** are one event each: the death is booked when the notable is created, and
//! nothing ages per tick (mcrtsim aged every NPC every tick; not copied).
//!
//! **Heirs inherit grudges:** when a notable dies its strongest memories and feelings pass to its
//! heir, or to its home settlement when the heir is gone, so a grudge outlives the one who held it.

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::Writer;
use rtsim_kernel::handle::Handle;

use crate::actor::ActorId;
use crate::report::ReportId;
use crate::temperament::Temperament;

/// One memory: report handle (6), salience (u8), spare (u8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct MemorySlot(pub [u8; 8]);

const _: () = assert!(std::mem::size_of::<MemorySlot>() == 8);

impl MemorySlot {
    pub fn new(report: ReportId, salience: u8) -> Self {
        let mut b = [0u8; 8];
        b[0..6].copy_from_slice(&report.to_bytes());
        b[6] = salience;
        MemorySlot(b)
    }

    pub fn report(&self) -> ReportId {
        Handle::from_bytes(self.0[0..6].try_into().unwrap())
    }

    pub fn salience(&self) -> u8 {
        self.0[6]
    }

    fn set_salience(&mut self, s: u8) {
        self.0[6] = s;
    }
}

/// A notable's own row. Its board and sentiment live in its holder entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notable {
    pub seed: u64,
    pub home: ActorId,
    /// Who inherits: a notable or a settlement holder. One 6-byte handle (PLAN.md §5.7).
    pub heir: ActorId,
    pub born: Tick,
    pub dies: Tick,
    pub(crate) memories: Vec<MemorySlot>,
    pub(crate) slots: u8,
}

impl Notable {
    pub fn new(seed: u64, home: ActorId, heir: ActorId, born: Tick, dies: Tick, slots: u8) -> Self {
        Self {
            seed,
            home,
            heir,
            born,
            dies,
            memories: Vec::new(),
            slots,
        }
    }

    pub fn temperament(&self) -> Temperament {
        Temperament::from_seed(self.seed)
    }

    /// Age in ticks at `at`; a life stage is derived from it when read, never stepped.
    pub fn age(&self, at: Tick) -> u64 {
        at.since(self.born)
    }

    pub fn memories(&self) -> &[MemorySlot] {
        &self.memories
    }

    /// Remembers a report. An existing memory keeps the higher salience. When full, the least
    /// salient memory goes (the oldest of equals), and a new one less salient than all is not kept.
    pub fn remember(&mut self, report: ReportId, salience: u8) {
        if salience == 0 || self.slots == 0 {
            return;
        }
        if let Some(m) = self.memories.iter_mut().find(|m| m.report() == report) {
            if salience > m.salience() {
                m.set_salience(salience);
            }
            return;
        }
        if self.memories.len() >= usize::from(self.slots) {
            // Earliest index among the least salient: memories are appended, so that is the
            // oldest of them.
            let (i, least) = self
                .memories
                .iter()
                .enumerate()
                .min_by_key(|(i, m)| (m.salience(), *i))
                .map(|(i, m)| (i, m.salience()))
                .expect("full means non-empty");
            if salience <= least {
                return;
            }
            self.memories.remove(i);
        }
        self.memories.push(MemorySlot::new(report, salience));
    }

    pub fn forget(&mut self, report: ReportId) {
        self.memories.retain(|m| m.report() != report);
    }

    /// The `n` most salient memories, most salient first (earliest first among equals).
    pub fn strongest_memories(&self, n: usize) -> Vec<MemorySlot> {
        let mut v: Vec<(usize, MemorySlot)> = self.memories.iter().copied().enumerate().collect();
        v.sort_by(|a, b| b.1.salience().cmp(&a.1.salience()).then(a.0.cmp(&b.0)));
        v.into_iter().take(n).map(|(_, m)| m).collect()
    }

    pub fn encode(&self, w: &mut Writer) {
        w.u64(self.seed);
        w.u64(self.home.to_bits());
        w.u64(self.heir.to_bits());
        w.u64(self.born.0);
        w.u64(self.dies.0);
        w.u8(self.slots);
        w.var_u64(self.memories.len() as u64);
        for m in &self.memories {
            w.raw(&m.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(i: u32) -> ReportId {
        Handle::from_parts(i, 1)
    }

    #[test]
    fn memories_keep_the_most_salient() {
        let mut n = Notable::new(1, Handle::NULL, Handle::NULL, Tick(0), Tick(10), 3);
        n.remember(r(1), 10);
        n.remember(r(2), 50);
        n.remember(r(3), 30);
        n.remember(r(4), 5); // weaker than everything: not kept
        assert_eq!(n.memories().len(), 3);
        n.remember(r(5), 40); // evicts r(1), the least salient
        let kept: Vec<_> = n.memories().iter().map(|m| m.report()).collect();
        assert_eq!(kept, vec![r(2), r(3), r(5)]);
        assert_eq!(
            n.strongest_memories(2)
                .iter()
                .map(|m| m.report())
                .collect::<Vec<_>>(),
            vec![r(2), r(5)]
        );
        n.remember(r(3), 60);
        assert_eq!(n.strongest_memories(1)[0].report(), r(3));
    }
}
