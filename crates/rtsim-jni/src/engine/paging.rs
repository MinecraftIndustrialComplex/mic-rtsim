// SPDX-License-Identifier: AGPL-3.0-or-later
//! The paged modules behind the boundary (docs/choices-paging-followups.md PF12 to PF21): the
//! herds, the dragons, the conflict, the ships and the phase 1 world page their rows through the
//! engine's own store, under a budget each.
//!
//! - **The store is shared.** The engine keeps it as an `Arc<Mutex<Option<PageStore>>>`, which is
//!   a `rows::PageSource`: every module reads its pages through it on a miss, on the step thread.
//! - **One meta-page convention** (PF12): every module takes its meta page's key from the caller,
//!   and the engine passes the module's own page, `(table, 0)` (herds 0x4800, dragons 0x4400,
//!   conflict 0x4300, ships 0x5300; the phase 1 world's is its snapshot table's, 0x5001 id 0).
//!   The module's row and index tables sit at its table plus 0x10 and up, so the per-table
//!   deletions of the engine's own pages never touch them.
//! - **A save** writes each module's meta page and its pages changed since the last save into the
//!   save's batch on the step thread ([`Tickets`]); the IO lane commits it and hands the tickets
//!   back through a channel ([`Outcome`]), which the next step drains: `saved` once the commit is
//!   durable, `save_failed` otherwise. A commit built before the step had seen every failed one
//!   is not made (it would rest on pages that never reached the disk) and counts as failed too
//!   (PF14).
//! - **The budget**: `enforce_budget` after every step; when a module is over its budget with
//!   every resident page changed since the last save (`needs_save`) and no save is in flight, an
//!   extra save at that step boundary, committed as a pending generation (P29): a crash rolls it
//!   back at the next start and the journal replays it (PF15).
//! - **State hashes** are whole passes (they read every row), so only an engine opened in TEST
//!   mode writes them, in a page of their own next to the meta page, `(table, 1)` (PF16).

use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use rtsim_conflict::{Conflict, ConflictTickets};
use rtsim_ecology::herds::{Herds, HerdsTicket};
use rtsim_ecology::{Dragons, DragonsTickets};
use rtsim_kernel::codec::{Reader, Writer};
use rtsim_kernel::page::{PageError, PageKey, PageStore, SaveBatch};
use rtsim_kernel::paging::SaveTicket;
use rtsim_kernel::rows::PageSource;
use rtsim_ships::{ShipTickets, Ships};

use super::State;

/// The engine's page store, shared with the paged modules (`None` once closed).
pub type Store = Arc<Mutex<Option<PageStore>>>;

/// Row bytes each paged module keeps resident (PF19). PLACEHOLDER 4 MiB (the markets sweep's
/// budget): five modules, 20 MiB of rows at most beside the resident indexes and wheels.
pub const DEFAULT_ROW_BUDGET: usize = 4 << 20;

pub const HASH_MAGIC: u32 = u32::from_le_bytes(*b"RTSH");

/// The page beside a module's meta page that holds its state hash (TEST mode only).
pub fn hash_key(meta: PageKey) -> PageKey {
    PageKey::new(meta.table(), meta.id() + 1)
}

/// A module's tables: its own and the row and index tables at plus 0x10 and up.
pub(super) fn tables(base: u16) -> RangeInclusive<u16> {
    base..=base + 0x1f
}

/// What a module needs to page: the module crates' paged API.
pub(super) trait Paged {
    type Ticket: Send + 'static;
    fn save_paged(&mut self, batch: &mut SaveBatch, meta_key: PageKey) -> Self::Ticket;
    fn saved(&mut self, t: Self::Ticket);
    fn save_failed(&mut self, t: Self::Ticket);
    fn enforce_budget(&mut self) -> usize;
    fn needs_save(&self) -> bool;
    fn state_hash(&self) -> u64;
}

macro_rules! paged {
    ($t:ty, $ticket:ty) => {
        impl Paged for $t {
            type Ticket = $ticket;
            fn save_paged(&mut self, batch: &mut SaveBatch, meta_key: PageKey) -> $ticket {
                <$t>::save_paged(self, batch, meta_key)
            }
            fn saved(&mut self, t: $ticket) {
                <$t>::saved(self, t)
            }
            fn save_failed(&mut self, t: $ticket) {
                <$t>::save_failed(self, t)
            }
            fn enforce_budget(&mut self) -> usize {
                <$t>::enforce_budget(self)
            }
            fn needs_save(&self) -> bool {
                <$t>::needs_save(self)
            }
            fn state_hash(&self) -> u64 {
                <$t>::state_hash(self)
            }
        }
    };
}

paged!(Herds, HerdsTicket);
paged!(Dragons, DragonsTickets);
paged!(Conflict, ConflictTickets);
paged!(Ships, ShipTickets);
paged!(rtsim_world::World, SaveTicket);

/// What a module link keeps for its pages.
#[derive(Default)]
pub(super) struct Pager {
    /// The next save writes the module even while it is switched off: it was switched off since
    /// the last save, or a save that wrote it failed.
    pub(super) save_due: bool,
    /// The module's tables hold pages this build does not use (dropped, or rows without their
    /// meta page): `start` deletes them before anything runs, so no fresh table reads them.
    pub(super) purge: bool,
}

/// The pages of the module's own table a save leaves live; the rest of that table is deleted
/// with the commit. `Keep`: the save did not write the module, its pages stay as they are.
pub(super) enum Live {
    Keep,
    Pages(Vec<PageKey>),
}

impl Live {
    /// Pages of `table` the store holds that this save leaves behind. Reads only that table's
    /// index blocks.
    pub(super) fn deletions(
        &self,
        store: &PageStore,
        table: u16,
    ) -> Result<Vec<PageKey>, PageError> {
        match self {
            Live::Keep => Ok(Vec::new()),
            Live::Pages(live) => Ok(store
                .table_ids(table)?
                .into_iter()
                .map(|id| PageKey::new(table, id))
                .filter(|k| !live.contains(k))
                .collect()),
        }
    }
}

/// One module's part of a save: its pages into `batch` unless it is switched off with nothing
/// due (U10: its pages stay as they were).
pub(super) fn save<E: Paged>(
    engine: &mut E,
    pager: &mut Pager,
    enabled: bool,
    meta: PageKey,
    batch: &mut SaveBatch,
    test: bool,
) -> (Option<E::Ticket>, Live) {
    if !enabled && !pager.save_due {
        return (None, Live::Keep);
    }
    pager.save_due = false;
    let t = engine.save_paged(batch, meta);
    let mut live = vec![meta];
    if test {
        let mut w = Writer::with_capacity(12);
        w.u32(HASH_MAGIC);
        w.u64(engine.state_hash());
        batch.put(hash_key(meta), w.as_bytes());
        live.push(hash_key(meta));
    }
    (Some(t), Live::Pages(live))
}

/// The ticket back: the commit is durable (`ok`), or it failed and the module's next save
/// writes it again, switched off or not.
pub(super) fn done<E: Paged>(engine: &mut E, pager: &mut Pager, t: Option<E::Ticket>, ok: bool) {
    if let Some(t) = t {
        if ok {
            engine.saved(t);
        } else {
            engine.save_failed(t);
            pager.save_due = true;
        }
    }
}

/// The state hash the save with the meta page at `meta` wrote, if it wrote one (TEST mode).
pub(super) fn saved_hash(source: &dyn PageSource, meta: PageKey) -> Option<u64> {
    let b = source.read(hash_key(meta)).ok()??;
    let mut r = Reader::new(&b);
    (r.u32().ok()? == HASH_MAGIC).then(|| r.u64().ok())?
}

/// Whether the store holds any page of `tables`.
pub(super) fn holds_pages(source: &dyn PageSource, tables: RangeInclusive<u16>) -> bool {
    tables.into_iter().any(|t| {
        source
            .table_ids(t)
            .map(|ids| !ids.is_empty())
            .unwrap_or(true)
    })
}

/// Every page of `tables` (a purge at start).
pub(super) fn purge_keys(
    store: &PageStore,
    tables: &RangeInclusive<u16>,
) -> Result<Vec<PageKey>, PageError> {
    let mut out = Vec::new();
    for t in tables.clone() {
        out.extend(
            store
                .table_ids(t)?
                .into_iter()
                .map(|id| PageKey::new(t, id)),
        );
    }
    Ok(out)
}

/// The paged modules' tickets of one save.
#[derive(Default)]
pub(super) struct Tickets {
    pub(super) herds: Option<HerdsTicket>,
    pub(super) dragons: Option<DragonsTickets>,
    pub(super) conflict: Option<ConflictTickets>,
    pub(super) ships: Option<ShipTickets>,
    pub(super) world: Option<SaveTicket>,
}

/// A save's result, back to the step thread.
pub(super) struct Outcome {
    tickets: Tickets,
    ok: bool,
}

/// A save on its way to the IO lane. Dropped unfinished (the lane refused the job, the engine
/// died first, or the commit was not made), it reports a failure: the tickets always come back.
pub(super) struct Job {
    tickets: Option<Tickets>,
    tx: Sender<Outcome>,
    failures: Arc<AtomicU64>,
    /// Failed saves the step had seen when it built this one.
    pub(super) built_after: u64,
}

impl Job {
    pub(super) fn new(
        tickets: Tickets,
        tx: Sender<Outcome>,
        failures: Arc<AtomicU64>,
        built_after: u64,
    ) -> Job {
        Job {
            tickets: Some(tickets),
            tx,
            failures,
            built_after,
        }
    }

    /// Whether a save failed that this one was built without knowing of: its pages would rest
    /// on pages that never reached the disk, so it must not be committed.
    pub(super) fn stale(&self) -> bool {
        self.failures.load(Ordering::Acquire) > self.built_after
    }

    /// The commit is durable.
    pub(super) fn succeeded(mut self) {
        if let Some(tickets) = self.tickets.take() {
            let _ = self.tx.send(Outcome { tickets, ok: true });
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if let Some(tickets) = self.tickets.take() {
            self.failures.fetch_add(1, Ordering::AcqRel);
            let _ = self.tx.send(Outcome { tickets, ok: false });
        }
    }
}

/// Hands every save result that came back to the modules (the start of a step, and before a
/// save).
pub(super) fn drain(st: &mut State) {
    while let Ok(o) = st.paging.rx.try_recv() {
        st.paging.in_flight = st.paging.in_flight.saturating_sub(1);
        if !o.ok {
            st.paging.failures_seen += 1;
        }
        let Tickets {
            herds,
            dragons,
            conflict,
            ships,
            world,
        } = o.tickets;
        done(&mut st.herds.engine, &mut st.herds.pager, herds, o.ok);
        done(&mut st.dragons.engine, &mut st.dragons.pager, dragons, o.ok);
        done(
            &mut st.conflict.engine,
            &mut st.conflict.pager,
            conflict,
            o.ok,
        );
        done(&mut st.ships.engine, &mut st.ships.pager, ships, o.ok);
        super::phase1::world_done(st, world, o.ok);
    }
}

/// Evicts saved row pages down to each module's budget (a step boundary). Returns whether a
/// module a save would write is still over its budget with every resident page changed since
/// the last save (a switched-off module with nothing due is not written, so it never counts).
pub(super) fn enforce_budget(st: &mut State) -> bool {
    fn over<E: Paged>(engine: &mut E, pager: &Pager, enabled: bool) -> bool {
        engine.enforce_budget();
        (enabled || pager.save_due) && engine.needs_save()
    }
    let h = over(&mut st.herds.engine, &st.herds.pager, st.herds.enabled);
    let d = over(
        &mut st.dragons.engine,
        &st.dragons.pager,
        st.dragons.enabled,
    );
    let c = over(
        &mut st.conflict.engine,
        &st.conflict.pager,
        st.conflict.enabled,
    );
    let s = over(&mut st.ships.engine, &st.ships.pager, st.ships.enabled);
    // The world's pages go with every snapshot, which a save takes when they are over.
    st.phase1.world.enforce_budget();
    h || d || c || s || st.phase1.world.needs_save()
}

/// The step's side of the save channel.
pub(super) struct Channel {
    pub(super) tx: Sender<Outcome>,
    rx: std::sync::mpsc::Receiver<Outcome>,
    /// Saves handed to the IO lane whose result has not come back.
    pub(super) in_flight: u32,
    /// Failed saves whose result came back.
    pub(super) failures_seen: u64,
}

impl Channel {
    pub(super) fn new() -> Channel {
        let (tx, rx) = std::sync::mpsc::channel();
        Channel {
            tx,
            rx,
            in_flight: 0,
            failures_seen: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PF14: a save that never finished (the IO lane refused it, or its commit was not made)
    /// still hands its tickets back, as failed; a save built before the step saw that failure is
    /// stale, one built after it is not.
    #[test]
    fn a_failed_job_reports_back_and_makes_later_ones_stale() {
        let failures = Arc::new(AtomicU64::new(0));
        let (tx, rx) = std::sync::mpsc::channel();
        let a = Job::new(Tickets::default(), tx.clone(), failures.clone(), 0);
        let b = Job::new(Tickets::default(), tx.clone(), failures.clone(), 0);
        assert!(!a.stale() && !b.stale());
        drop(a);
        assert!(!rx.try_recv().expect("a's tickets came back").ok);
        assert!(b.stale(), "b was built before a's failure came back");
        drop(b);
        assert!(!rx.try_recv().expect("b's tickets came back").ok);
        let c = Job::new(Tickets::default(), tx, failures.clone(), 2);
        assert!(!c.stale(), "c was built after both came back");
        c.succeeded();
        assert!(rx.try_recv().expect("c's tickets came back").ok);
        assert_eq!(failures.load(Ordering::Acquire), 2);
    }
}
