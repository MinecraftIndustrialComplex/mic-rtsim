// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where parallel work runs (SCALE.md §5, PLAN.md §2): the engine owns no threads, so it asks an
//! [`Executor`] the caller hands it.
//!
//! The trait is the smallest form of structured parallelism, scope and spawn: a [`Scope`] collects
//! jobs that may borrow from the caller's stack, and [`Executor::run`] returns only when every job
//! has finished. Nothing the engine computes depends on how many workers there are or which job
//! runs where; the parallel step (`crate::bsp`) makes its results identical at any worker count.
//!
//! # Implementations
//!
//! - [`Inline`]: one worker, the caller. Jobs run in spawn order. For tests, tools and the
//!   reference run of the determinism gate.
//! - [`ThreadPool`]: plain `std` threads the *caller* creates and owns, for the CLI, the bench and
//!   tests. It exists only until the native host is wired in.
//! - **mic-native-rt** (the native host's plugin crate, `mods/mic-native`): the engine's plugin
//!   implements the trait over the host's pools, so the step runs on the background pool and never
//!   brings threads of its own. `rt::scope` already has the contract this trait needs (jobs borrow
//!   anything that outlives the scope; it returns when all have finished; a worker of the pool
//!   helps while it waits):
//!
//! ```ignore
//! use mic_native_rt::{self as rt, Pool};
//! use rtsim_kernel::exec::{Executor, Job};
//!
//! struct HostExecutor(Pool); // Pool::Background during play, Pool::Foreground on a loading screen
//!
//! impl Executor for HostExecutor {
//!     fn workers(&self) -> usize {
//!         rt::governor::pool_info(self.0).map_or(1, |i| i.threads as usize)
//!     }
//!     fn run<'env>(&self, jobs: Vec<Job<'env>>) {
//!         let r = rt::scope(self.0, |s| -> rt::Result<()> {
//!             for job in jobs {
//!                 s.spawn(job)?;
//!             }
//!             Ok(())
//!         });
//!         // rt catches a job's panic and reports Error::PANIC; re-raise it on the step's thread,
//!         // where the plugin's export boundary catches it (PLAN.md §2).
//!         if !matches!(r, Ok(Ok(()))) {
//!             panic!("a step job failed: {r:?}");
//!         }
//!     }
//! }
//! ```
//!
//! The step job itself runs on the background pool, so its `rt::scope(Pool::Background, ..)` is a
//! background worker waiting on its own pool, which mic-native allows (no priority inversion).
//! Job tags for the per-job hooks are not needed: the engine keeps no per-session thread-locals.

use std::any::Any;
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

/// One unit of parallel work. It may borrow anything that outlives the scope it was spawned into.
pub type Job<'env> = Box<dyn FnOnce() + Send + 'env>;

/// Runs batches of jobs, possibly in parallel. Implementations must run every job exactly once
/// and return only when all have finished, even when one panics; a job's panic is then resumed on
/// the caller.
pub trait Executor: Sync {
    /// How many jobs may run at once. Advice for splitting work; results never depend on it.
    fn workers(&self) -> usize;

    /// Runs every job and waits for all of them.
    fn run<'env>(&self, jobs: Vec<Job<'env>>);
}

/// Jobs collected for one [`scope`].
pub struct Scope<'env> {
    jobs: Vec<Job<'env>>,
}

impl<'env> Scope<'env> {
    /// Adds a job. It starts when the scope's body has returned.
    pub fn spawn(&mut self, f: impl FnOnce() + Send + 'env) {
        self.jobs.push(Box::new(f));
    }

    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

/// Calls `body` to spawn jobs, then runs them all on `exec` and waits. Returns `body`'s result.
pub fn scope<'env, E, R>(exec: &E, body: impl FnOnce(&mut Scope<'env>) -> R) -> R
where
    E: Executor + ?Sized,
{
    let mut s = Scope { jobs: Vec::new() };
    let r = body(&mut s);
    exec.run(s.jobs);
    r
}

/// One worker: the caller, in spawn order.
#[derive(Clone, Copy, Debug, Default)]
pub struct Inline;

impl Executor for Inline {
    fn workers(&self) -> usize {
        1
    }

    fn run<'env>(&self, jobs: Vec<Job<'env>>) {
        for job in jobs {
            job();
        }
    }
}

/// Locks, ignoring poison: a job's panic is caught before it can poison anything here, and the
/// pool must keep its guarantee (wait for every job) whatever happened.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Batch {
    pending: AtomicUsize,
    panic: Mutex<Option<Box<dyn Any + Send>>>,
    done: Mutex<bool>,
    cv: Condvar,
}

struct Task {
    job: Job<'static>,
    batch: Arc<Batch>,
}

impl Task {
    fn run(self) {
        let Task { job, batch } = self;
        if let Err(p) = catch_unwind(AssertUnwindSafe(job)) {
            let mut first = lock(&batch.panic);
            if first.is_none() {
                *first = Some(p);
            }
        }
        if batch.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
            *lock(&batch.done) = true;
            batch.cv.notify_all();
        }
    }
}

#[derive(Default)]
struct Queue {
    tasks: VecDeque<Task>,
    shutdown: bool,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    cv: Condvar,
}

/// A fixed set of `std` threads that the caller creates and owns (the engine owns none). The
/// calling thread of [`Executor::run`] works too, so `ThreadPool::new(n)` starts `n - 1` threads
/// and runs at most `n` jobs at once.
pub struct ThreadPool {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl ThreadPool {
    /// A pool of `workers` (at least 1; 1 means everything runs on the caller).
    pub fn new(workers: usize) -> ThreadPool {
        let shared = Arc::new(Shared::default());
        let threads = (1..workers.max(1))
            .map(|i| {
                let shared = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name(format!("rtsim-worker-{i}"))
                    .spawn(move || worker(&shared))
                    .expect("spawn a pool thread")
            })
            .collect();
        ThreadPool { shared, threads }
    }
}

fn worker(shared: &Shared) {
    loop {
        let task = {
            let mut q = lock(&shared.queue);
            loop {
                if let Some(t) = q.tasks.pop_front() {
                    break t;
                }
                if q.shutdown {
                    return;
                }
                q = shared.cv.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        task.run();
    }
}

impl Executor for ThreadPool {
    fn workers(&self) -> usize {
        self.threads.len() + 1
    }

    fn run<'env>(&self, jobs: Vec<Job<'env>>) {
        if jobs.len() <= 1 || self.threads.is_empty() {
            return Inline.run(jobs);
        }
        let batch = Arc::new(Batch {
            pending: AtomicUsize::new(jobs.len()),
            panic: Mutex::new(None),
            done: Mutex::new(false),
            cv: Condvar::new(),
        });
        {
            let mut q = lock(&self.shared.queue);
            for job in jobs {
                // SAFETY: only the lifetime is erased. This function does not return, and does not
                // unwind, until `pending` reaches zero, that is until every job of this batch has
                // run and been dropped (Task::run consumes it, and catches its panic). Nothing in
                // between can panic: locks ignore poison and the queue only moves boxes. So no job
                // outlives the borrows its 'env lifetime promised.
                let job: Job<'static> =
                    unsafe { std::mem::transmute::<Job<'env>, Job<'static>>(job) };
                q.tasks.push_back(Task {
                    job,
                    batch: Arc::clone(&batch),
                });
            }
        }
        self.shared.cv.notify_all();
        // The caller works too, until the queue is empty (it may run another batch's job; that
        // is fine, every batch waits for its own count).
        loop {
            let task = lock(&self.shared.queue).tasks.pop_front();
            match task {
                Some(t) => t.run(),
                None => break,
            }
        }
        let mut done = lock(&batch.done);
        while !*done {
            done = batch.cv.wait(done).unwrap_or_else(|e| e.into_inner());
        }
        drop(done);
        if let Some(p) = lock(&batch.panic).take() {
            resume_unwind(p);
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        lock(&self.shared.queue).shutdown = true;
        self.shared.cv.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn every_job_runs_once_and_may_borrow() {
        for exec in [&Inline as &dyn Executor, &ThreadPool::new(4)] {
            let mut out = vec![0u64; 100];
            scope(exec, |s| {
                for (i, slot) in out.iter_mut().enumerate() {
                    s.spawn(move || *slot = i as u64 * 3);
                }
            });
            assert_eq!(out, (0..100).map(|i| i * 3).collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_panic_reaches_the_caller_after_every_job_finished() {
        let pool = ThreadPool::new(3);
        let ran = AtomicU64::new(0);
        let r = catch_unwind(AssertUnwindSafe(|| {
            scope(&pool, |s| {
                for i in 0..20 {
                    let ran = &ran;
                    s.spawn(move || {
                        ran.fetch_add(1, Ordering::SeqCst);
                        if i == 7 {
                            panic!("job 7");
                        }
                    });
                }
            })
        }));
        assert!(r.is_err());
        assert_eq!(ran.load(Ordering::SeqCst), 20);
        // The pool still works afterwards.
        let n = AtomicU64::new(0);
        scope(&pool, |s| {
            for _ in 0..10 {
                let n = &n;
                s.spawn(move || {
                    n.fetch_add(1, Ordering::SeqCst);
                });
            }
        });
        assert_eq!(n.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn concurrent_callers_share_a_pool() {
        let pool = ThreadPool::new(4);
        let total = AtomicU64::new(0);
        std::thread::scope(|ts| {
            for _ in 0..4 {
                ts.spawn(|| {
                    for _ in 0..50 {
                        scope(&pool, |s| {
                            for _ in 0..8 {
                                let total = &total;
                                s.spawn(move || {
                                    total.fetch_add(1, Ordering::SeqCst);
                                });
                            }
                        });
                    }
                });
            }
        });
        assert_eq!(total.load(Ordering::SeqCst), 4 * 50 * 8);
    }
}
