// SPDX-License-Identifier: AGPL-3.0-or-later
//! The bulk-synchronous parallel step (SCALE.md §5, PLAN.md §4.7): the regions due in a step are
//! processed in parallel, and every effect that crosses regions is a message applied at a later
//! step in a fixed order, so the result is the same at any worker count.
//!
//! # One step, over the window `(from, to]`
//!
//! 1. **Gather** (caller's thread): every wheel entry due by `to` and every message due by `to`,
//!    grouped by region and page, in key order.
//! 2. **Load** (caller's thread): the pages of those regions ([`PagedTable`] loads; prefetch should
//!    have made most of them resident already).
//! 3. **Process** (in parallel on the [`Executor`]): each page is moved into a job, and each due
//!    region handles its work in time order, interleaving its own breakpoints with its messages.
//!    A breakpoint that falls inside the window after a message or another breakpoint fires in the
//!    same step, at its own tick. A region reads and writes only itself; anything it does to
//!    another region it sends as a message ([`Post::send`]).
//! 4. **Merge** (caller's thread): pages go back, next breakpoints are booked in key order, and
//!    sent messages are sorted by (destination, sender, send tick, sequence) and booked for
//!    delivery no earlier than the next step.
//!
//! # Why the worker count cannot matter
//!
//! What a region does in a step depends only on its own state and its own work list, and its work
//! list is sorted by (time, kind, sender, send tick, sequence): breakpoints before messages at the
//! same tick, messages by region id then by message sequence (SCALE.md §5). How regions are split
//! into jobs, and which job finishes first, changes nothing: outputs are put back in key order or
//! sorted. The determinism gate (this module's tests, and `rtsim-cli bench bsp`) runs the same world
//! at 1 and N workers and compares state hashes.
//!
//! The step length is part of the world, not of the machine: a message is never applied in the step
//! that sent it, so a longer step delays some messages more. Two runs agree when they use the same
//! steps.
//!
//! # Not in the parallel phase
//!
//! Reads and events from outside the engine ([`PagedTable::read`], [`PagedTable::apply`]) run on the
//! caller's thread between steps, as before. External inputs that should be ordered with the step's
//! messages go in through [`Bsp::post`].

use crate::clock::Tick;
use crate::codec::{Decode, Encode, Reader, Writer};
use crate::exec::{Executor, scope};
use crate::lazy::LazyStats;
use crate::page::{PageError, PageKey, PageStore, Result, SaveBatch};
use crate::paging::{Page, PagedModel, PagedTable};
use crate::rng::{StreamKey, SystemId};
use crate::wheel::TimingWheel;

/// A region model that takes part in the parallel step.
pub trait Actor: PagedModel + Send {
    /// What regions send each other. Saved while in flight.
    type Msg: Send + Encode + Decode;

    /// The booked breakpoint fired at `at`; the region is caught up and settled to `at`.
    fn on_wake(&mut self, at: Tick, post: &mut Post<'_, Self::Msg>);

    /// A message from region `from` arrived at `at`; the region is settled to `at`.
    fn on_message(&mut self, at: Tick, from: u64, msg: Self::Msg, post: &mut Post<'_, Self::Msg>);
}

/// A message in flight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope<Msg> {
    pub to: u64,
    /// The sending region, or [`EXTERNAL`] for [`Bsp::post`].
    pub from: u64,
    pub sent_at: Tick,
    /// The sender's sequence within the step: with `from` and `sent_at`, unique.
    pub n: u32,
    /// The earliest time it may be applied. It is applied at the first step boundary at or after
    /// this, and never in the step that sent it.
    pub deliver_at: Tick,
    pub msg: Msg,
}

/// The sender id of messages from outside the engine.
pub const EXTERNAL: u64 = u64::MAX;

/// Work items (a region's wake, or one message) a job gets at least: below this, handing work to
/// another thread costs more than it saves, and a step with less runs on the caller. PLACEHOLDER,
/// from the first `bench bsp` run.
pub const MIN_JOB_WEIGHT: usize = 64;

impl<Msg> Envelope<Msg> {
    fn order_key(&self) -> (u64, u64, Tick, u32) {
        (self.to, self.from, self.sent_at, self.n)
    }
}

/// Where a region sends messages while it handles its work.
pub struct Post<'a, Msg> {
    from: u64,
    now: Tick,
    n: &'a mut u32,
    out: &'a mut Vec<Envelope<Msg>>,
}

impl<Msg> Post<'_, Msg> {
    /// Sends `msg` to region `to`, to be applied at `deliver_at` or at the first step boundary
    /// after this step, whichever is later.
    pub fn send(&mut self, to: u64, deliver_at: Tick, msg: Msg) {
        self.out.push(Envelope {
            to,
            from: self.from,
            sent_at: self.now,
            n: *self.n,
            deliver_at,
            msg,
        });
        *self.n += 1;
    }

    /// The region handling this work.
    pub fn from(&self) -> u64 {
        self.from
    }

    /// The tick of the work being handled.
    pub fn now(&self) -> Tick {
        self.now
    }
}

/// Counters for the profiler and the bench.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BspStats {
    pub steps: u64,
    /// Regions that had work.
    pub regions: u64,
    pub wakes: u64,
    pub messages: u64,
    pub sent: u64,
    /// Messages to regions that do not exist, handed back in [`StepReport::dead_letters`].
    pub dead_letters: u64,
    /// Regions that were due only through stale wheel entries.
    pub stale: u64,
    /// Jobs handed to the executor.
    pub jobs: u64,
}

/// What a step hands back.
#[derive(Debug)]
pub struct StepReport<Msg> {
    /// Regions that had work this step.
    pub regions: usize,
    /// Messages whose destination does not exist, in the fixed order. The caller decides what
    /// happens to what they carried (conservation).
    pub dead_letters: Vec<Envelope<Msg>>,
}

struct RegionWork<Msg> {
    key: u64,
    /// (effective time, envelope), sorted.
    mail: Vec<(Tick, Envelope<Msg>)>,
}

struct PageWork<M: Actor> {
    slab: u32,
    page: Page<M>,
    before: usize,
    regions: Vec<RegionWork<M::Msg>>,
}

struct ChunkOut<M: Actor> {
    pages: Vec<(u32, Page<M>, usize)>,
    bookings: Vec<(u64, Tick)>,
    sent: Vec<Envelope<M::Msg>>,
    dead: Vec<Envelope<M::Msg>>,
    stats: BspStats,
    lazy: LazyStats,
}

#[derive(Clone, Copy)]
struct Ctx {
    world_seed: u64,
    system: SystemId,
    per: u64,
    start: Tick,
    to: Tick,
}

fn process<M: Actor>(chunk: Vec<PageWork<M>>, ctx: Ctx) -> ChunkOut<M> {
    let mut out = ChunkOut {
        pages: Vec::with_capacity(chunk.len()),
        bookings: Vec::new(),
        sent: Vec::new(),
        dead: Vec::new(),
        stats: BspStats::default(),
        lazy: LazyStats::default(),
    };
    for PageWork {
        slab,
        mut page,
        before,
        regions,
    } in chunk
    {
        for RegionWork { key, mail } in regions {
            let s = (key % ctx.per) as usize;
            let Some(slot) = page.slots[s].as_mut() else {
                out.stats.dead_letters += mail.len() as u64;
                out.dead.extend(mail.into_iter().map(|(_, e)| e));
                continue;
            };
            let stream = StreamKey::new(ctx.world_seed, ctx.system, key);
            let mut changed = false;
            let mut n = 0u32;
            let mut mail = mail.into_iter().peekable();
            loop {
                let wake = slot.wake.filter(|&w| w <= ctx.to);
                let next_mail = mail.peek().map(|(at, _)| *at);
                let take_wake = match (wake, next_mail) {
                    (Some(w), Some(m)) => w <= m,
                    (Some(_), None) => true,
                    (None, Some(_)) => false,
                    (None, None) => break,
                };
                let heap0 = slot.cell.model().heap_bytes();
                if take_wake {
                    let at = wake
                        .expect("checked")
                        .max(ctx.start)
                        .max(slot.cell.anchor());
                    slot.wake = None;
                    out.lazy.slot_commits += slot.cell.catch_up(at, stream);
                    slot.cell.settle_to(at);
                    let mut post = Post {
                        from: key,
                        now: at,
                        n: &mut n,
                        out: &mut out.sent,
                    };
                    slot.cell.model_mut().on_wake(at, &mut post);
                    out.stats.wakes += 1;
                    out.lazy.breakpoints += 1;
                } else {
                    let (at, env) = mail.next().expect("checked");
                    let at = at.max(slot.cell.anchor());
                    out.lazy.slot_commits += slot.cell.catch_up(at, stream);
                    slot.cell.settle_to(at);
                    let mut post = Post {
                        from: key,
                        now: at,
                        n: &mut n,
                        out: &mut out.sent,
                    };
                    slot.cell
                        .model_mut()
                        .on_message(at, env.from, env.msg, &mut post);
                    out.stats.messages += 1;
                    out.lazy.events += 1;
                }
                page.heap = page.heap + slot.cell.model().heap_bytes() - heap0;
                slot.wake = slot.cell.model().next_breakpoint(slot.cell.anchor());
                changed = true;
            }
            if changed {
                page.dirty = true;
                out.stats.regions += 1;
                if let Some(w) = slot.wake {
                    out.bookings.push((key, w));
                }
            } else {
                out.stats.stale += 1;
            }
        }
        out.pages.push((slab, page, before));
    }
    out
}

/// The parallel step's own state: messages in flight.
pub struct Bsp<M: Actor> {
    mail: TimingWheel<Envelope<M::Msg>>,
    stats: BspStats,
    external_n: u32,
}

impl<M: Actor> Bsp<M> {
    pub fn new(now: Tick) -> Self {
        Bsp {
            mail: TimingWheel::new(now),
            stats: BspStats::default(),
            external_n: 0,
        }
    }

    pub fn stats(&self) -> BspStats {
        self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = BspStats::default();
    }

    /// Messages in flight.
    pub fn in_flight(&self) -> usize {
        self.mail.len()
    }

    /// Every message in flight, in no particular order (conservation checks).
    pub fn iter_mail(&self) -> impl Iterator<Item = &Envelope<M::Msg>> {
        self.mail.iter().map(|(_, e)| e)
    }

    /// A message from outside the engine, applied at `deliver_at` or the next step boundary.
    pub fn post(&mut self, to: u64, deliver_at: Tick, msg: M::Msg) {
        let now = self.mail.now();
        let env = Envelope {
            to,
            from: crate::bsp::EXTERNAL,
            sent_at: now,
            n: self.external_n,
            deliver_at,
            msg,
        };
        self.external_n = self.external_n.wrapping_add(1);
        self.mail.schedule(deliver_at.max(Tick(now.0 + 1)), env);
    }

    /// One step over `(table.now(), to]`. See the module docs.
    pub fn step(
        &mut self,
        table: &mut PagedTable<M>,
        store: &mut PageStore,
        exec: &dyn Executor,
        to: Tick,
    ) -> Result<StepReport<M::Msg>> {
        let from = table.now();
        assert!(to >= from, "step to {to:?} before the table's now {from:?}");
        if to == from {
            // An empty window: nothing can be due in it (wakes at `from` were due last step).
            return Ok(StepReport {
                regions: 0,
                dead_letters: Vec::new(),
            });
        }
        let start = Tick(from.0 + 1);
        self.stats.steps += 1;

        // 1. Gather.
        let mut due: Vec<u64> = table.due.drain(..).map(|b| b.key).collect();
        while let Some((_, b)) = table.wheel.pop_until(to) {
            due.push(b.key);
        }
        due.sort_unstable();
        due.dedup();
        let mut mail: Vec<(Tick, Envelope<M::Msg>)> = Vec::new();
        while let Some(m) = self.mail.pop_until(to) {
            mail.push(m);
        }
        mail.sort_by(|a, b| {
            (a.1.to, a.0, a.1.from, a.1.sent_at, a.1.n).cmp(&(
                b.1.to,
                b.0,
                b.1.from,
                b.1.sent_at,
                b.1.n,
            ))
        });
        // Merge the two key-sorted lists into region work, in key order.
        let mut regions: Vec<RegionWork<M::Msg>> = Vec::with_capacity(due.len() + mail.len());
        let mut di = due.into_iter().peekable();
        let mut mi = mail.into_iter().peekable();
        loop {
            let key = match (di.peek(), mi.peek()) {
                (Some(&d), Some((_, e))) => d.min(e.to),
                (Some(&d), None) => d,
                (None, Some((_, e))) => e.to,
                (None, None) => break,
            };
            if di.peek() == Some(&key) {
                di.next();
            }
            let mut rw = RegionWork {
                key,
                mail: Vec::new(),
            };
            while mi.peek().is_some_and(|(_, e)| e.to == key) {
                rw.mail.push(mi.next().expect("peeked"));
            }
            regions.push(rw);
        }
        let n_regions = regions.len();

        // 2. Load, page by page.
        let per = table.cfg.regions_per_page;
        let mut groups: Vec<(u64, Vec<RegionWork<M::Msg>>)> = Vec::new();
        for rw in regions {
            let pid = rw.key / per;
            match groups.last_mut() {
                Some((p, v)) if *p == pid => v.push(rw),
                _ => groups.push((pid, vec![rw])),
            }
        }
        let mut dead: Vec<Envelope<M::Msg>> = Vec::new();
        let mut slabs: Vec<Option<u32>> = Vec::with_capacity(groups.len());
        for (pid, _) in &groups {
            slabs.push(table.ensure(store, *pid)?);
        }
        let mut work: Vec<PageWork<M>> = Vec::with_capacity(groups.len());
        let mut weight = 0usize;
        for ((_, rws), slab) in groups.into_iter().zip(slabs) {
            let Some(slab) = slab else {
                for rw in rws {
                    self.stats.dead_letters += rw.mail.len() as u64;
                    dead.extend(rw.mail.into_iter().map(|(_, e)| e));
                }
                continue;
            };
            let page = table.slab[slab as usize].take().expect("resident page");
            let before = page.bytes();
            weight += rws.iter().map(|r| 1 + r.mail.len()).sum::<usize>();
            work.push(PageWork {
                slab,
                page,
                before,
                regions: rws,
            });
        }

        // 3. Process. Contiguous chunks of about equal work, a few per worker.
        let ctx = Ctx {
            world_seed: table.world_seed(),
            system: table.system(),
            per,
            start,
            to,
        };
        let target = (exec.workers() * 4)
            .min(weight / MIN_JOB_WEIGHT)
            .min(work.len())
            .max(1);
        let per_chunk = weight.div_ceil(target).max(1);
        let mut chunks: Vec<Vec<PageWork<M>>> = Vec::new();
        let mut cur: Vec<PageWork<M>> = Vec::new();
        let mut cur_w = 0usize;
        for pw in work {
            cur_w += pw.regions.iter().map(|r| 1 + r.mail.len()).sum::<usize>();
            cur.push(pw);
            if cur_w >= per_chunk {
                chunks.push(std::mem::take(&mut cur));
                cur_w = 0;
            }
        }
        if !cur.is_empty() {
            chunks.push(cur);
        }
        self.stats.jobs += chunks.len() as u64;
        let mut results: Vec<Option<ChunkOut<M>>> = (0..chunks.len()).map(|_| None).collect();
        if chunks.len() == 1 {
            results[0] = Some(process(chunks.pop().expect("one chunk"), ctx));
        } else {
            scope(exec, |s| {
                for (chunk, slot) in chunks.into_iter().zip(results.iter_mut()) {
                    s.spawn(move || *slot = Some(process(chunk, ctx)));
                }
            });
        }

        // 4. Merge, in chunk order (= key order).
        let mut sent: Vec<Envelope<M::Msg>> = Vec::new();
        for r in results {
            let r = r.expect("every job ran");
            for (slab, page, before) in r.pages {
                table.slab[slab as usize] = Some(page);
                table.reaccount(slab, before);
                table.sync(slab);
            }
            for (key, at) in r.bookings {
                table.book(key, Some(at));
            }
            sent.extend(r.sent);
            dead.extend(r.dead);
            let s = r.stats;
            self.stats.regions += s.regions;
            self.stats.wakes += s.wakes;
            self.stats.messages += s.messages;
            self.stats.dead_letters += s.dead_letters;
            self.stats.stale += s.stale;
            table.lazy.slot_commits += r.lazy.slot_commits;
            table.lazy.breakpoints += r.lazy.breakpoints;
            table.lazy.events += r.lazy.events;
        }
        sent.sort_by_key(|e| e.order_key());
        self.stats.sent += sent.len() as u64;
        // The mail wheel's clock is at `to` already: the gather loop drained it there.
        let next = Tick(to.0 + 1);
        for e in sent {
            self.mail.schedule(e.deliver_at.max(next), e);
        }
        dead.sort_by_key(|e| e.order_key());
        Ok(StepReport {
            regions: n_regions,
            dead_letters: dead,
        })
    }

    /// Puts the messages in flight into `batch` under `key`, in a canonical order.
    pub fn save(&self, batch: &mut SaveBatch, key: PageKey) {
        batch.put(key, &self.encode());
    }

    fn encode(&self) -> Vec<u8> {
        let mut all: Vec<(Tick, &Envelope<M::Msg>)> = self.mail.iter().collect();
        all.sort_by_key(|(at, e)| (*at, e.order_key()));
        let mut w = Writer::with_capacity(16 + all.len() * 32);
        w.var_u64(self.mail.now().0);
        w.u32(self.external_n);
        w.var_u64(all.len() as u64);
        for (at, e) in all {
            w.var_u64(at.0);
            w.var_u64(e.to);
            w.var_u64(e.from);
            w.var_u64(e.sent_at.0);
            w.var_u64(u64::from(e.n));
            w.var_u64(e.deliver_at.0);
            e.msg.encode(&mut w);
        }
        w.into_bytes()
    }

    /// A CRC32 of the messages in flight, for the determinism gate.
    pub fn mail_hash(&self) -> u32 {
        crc32fast::hash(&self.encode())
    }

    /// Restores what [`Bsp::save`] wrote; an absent page means nothing in flight at tick 0.
    pub fn open(store: &mut PageStore, key: PageKey) -> Result<Self> {
        let Some(bytes) = store.get(key)? else {
            return Ok(Bsp::new(Tick::ZERO));
        };
        let bad = |e: crate::codec::CodecError| PageError::Format(format!("bsp mail: {e}"));
        let mut r = Reader::new(&bytes);
        let now = Tick(r.var_u64().map_err(bad)?);
        let mut b = Bsp::new(now);
        b.external_n = r.u32().map_err(bad)?;
        let n = r.var_u64().map_err(bad)?;
        for _ in 0..n {
            let at = Tick(r.var_u64().map_err(bad)?);
            let e = Envelope {
                to: r.var_u64().map_err(bad)?,
                from: r.var_u64().map_err(bad)?,
                sent_at: Tick(r.var_u64().map_err(bad)?),
                n: r.var_u64().map_err(bad)? as u32,
                deliver_at: Tick(r.var_u64().map_err(bad)?),
                msg: M::Msg::decode(&mut r).map_err(bad)?,
            };
            b.mail.schedule(at.max(now), e);
        }
        r.finish().map_err(bad)?;
        Ok(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TICKS_PER_DAY;
    use crate::exec::{Inline, ThreadPool};
    use crate::page::Durability;
    use crate::paging::PagingConfig;
    use crate::synth::{Cargo, Herd, TradePost};

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rtsim-bsp-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Runs a world of `regions` posts for `days`, stepping `step` ticks at a time; returns the
    /// state hash of the table and the mail, and the stats.
    fn run(
        regions: u64,
        days: u64,
        step: u64,
        exec: &dyn Executor,
        budget: usize,
        tag: &str,
    ) -> (u32, u32, BspStats, u64) {
        let dir = tmpdir(tag);
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let cfg = PagingConfig {
            regions_per_page: 16,
            budget_bytes: budget,
            ..PagingConfig::new(1, 2)
        };
        let mut t: PagedTable<TradePost> =
            PagedTable::new(9, SystemId::named("test.post"), Tick::ZERO, cfg);
        let setup = StreamKey::new(9, SystemId::named("test.setup"), 0);
        let mut grain0 = 0;
        for k in 0..regions {
            let mut rng = setup.slot(k);
            let p = TradePost {
                herd: Herd::new(rng.next_u64(), [30, 50, 20]),
                grain: 100,
                received: 0,
                next_departure: Tick(1 + rng.below(2 * TICKS_PER_DAY)),
                departures: 0,
                regions,
                seed: 9,
                work: 3,
                ledger: 0,
            };
            grain0 += p.grain;
            t.insert(&mut store, k, p, Tick::ZERO).unwrap();
        }
        let mut bsp: Bsp<TradePost> = Bsp::new(Tick::ZERO);
        let end = Tick::from_days(days);
        let mut now = Tick::ZERO;
        while now < end {
            now = Tick((now.0 + step).min(end.0));
            let r = bsp.step(&mut t, &mut store, exec, now).unwrap();
            assert!(r.dead_letters.is_empty());
            if now.0 % (TICKS_PER_DAY / 4) < step {
                let mut b = SaveBatch::new();
                let ticket = t.save_dirty(&mut b);
                store.commit(b, 0).unwrap();
                t.saved(ticket);
            }
            t.enforce_budget();
        }
        // Conservation over the stored states: grain held + in flight = initial + produced, 10
        // per region per day slot committed by its anchor.
        let (mut held, mut produced) = (0, 0);
        t.for_each(&mut store, |_, cell, _| {
            held += cell.model().grain;
            produced += 10 * cell.anchor().day() as i64;
        })
        .unwrap();
        let flying: i64 = bsp.iter_mail().map(|e| e.msg.0).sum();
        assert_eq!(held + flying, grain0 + produced, "grain is conserved");
        let h = t.state_hash(&mut store, end).unwrap();
        let stats = bsp.stats();
        std::fs::remove_dir_all(&dir).unwrap();
        (h, bsp.mail_hash(), stats, t.stats().evictions)
    }

    /// The determinism gate: 1 worker and N workers, resident or paged, give the same state.
    #[test]
    fn same_state_at_any_worker_count_and_budget() {
        // Short steps run mostly on the caller (under MIN_JOB_WEIGHT); long steps split into jobs.
        for (regions, step) in [(300u64, 200u64), (1_000, 4_800)] {
            let one = run(regions, 12, step, &Inline, usize::MAX, "inline");
            assert!(one.2.wakes > 500 && one.2.messages > 500, "{:?}", one.2);
            for (workers, budget) in [(2, usize::MAX), (7, usize::MAX), (4, 20_000)] {
                let pool = ThreadPool::new(workers);
                let n = run(
                    regions,
                    12,
                    step,
                    &pool,
                    budget,
                    &format!("pool{workers}-{budget}-{step}"),
                );
                assert_eq!(
                    (one.0, one.1),
                    (n.0, n.1),
                    "{regions} regions, step {step}, {workers} workers, budget {budget}"
                );
                assert_eq!(one.2.wakes, n.2.wakes);
                assert_eq!(one.2.messages, n.2.messages);
                if step > 1_000 {
                    assert!(
                        n.2.jobs > n.2.steps,
                        "the pool ran steps in parallel: {:?}",
                        n.2
                    );
                }
                if budget != usize::MAX {
                    assert!(n.3 > 0, "the small budget evicted pages");
                }
            }
        }
    }

    /// Breakpoints that fall inside a long window fire at their own ticks, so a world stepped
    /// one tick at a time agrees with one stepped in long windows wherever messages do not
    /// arrive within their sender's window.
    #[test]
    fn wakes_fire_at_their_ticks_inside_a_window() {
        let dir = tmpdir("window");
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut t: PagedTable<TradePost> =
            PagedTable::new(1, SystemId::named("w"), Tick::ZERO, PagingConfig::new(1, 2));
        let p = TradePost {
            herd: Herd::new(1, [1, 1, 1]),
            grain: 1000,
            received: 0,
            next_departure: Tick(10),
            departures: 0,
            regions: 1,
            seed: 1,
            work: 0,
            ledger: 0,
        };
        t.insert(&mut store, 0, p, Tick::ZERO).unwrap();
        let mut bsp: Bsp<TradePost> = Bsp::new(Tick::ZERO);
        bsp.step(&mut t, &mut store, &Inline, Tick::from_days(20))
            .unwrap();
        let mut deps = 0;
        t.for_each(&mut store, |_, c, _| deps = c.model().departures)
            .unwrap();
        // Departures every 1 to 4 days over 20 days, all in one window.
        assert!(deps >= 5, "{deps}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mail_survives_a_save() {
        let dir = tmpdir("mail");
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut bsp: Bsp<TradePost> = Bsp::new(Tick(5));
        bsp.post(3, Tick(100), Cargo(7));
        bsp.post(1, Tick(50), Cargo(2));
        let mut b = SaveBatch::new();
        bsp.save(&mut b, PageKey::new(3, 0));
        store.commit(b, 0).unwrap();
        let back: Bsp<TradePost> = Bsp::open(&mut store, PageKey::new(3, 0)).unwrap();
        assert_eq!(back.mail_hash(), bsp.mail_hash());
        assert_eq!(back.in_flight(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
