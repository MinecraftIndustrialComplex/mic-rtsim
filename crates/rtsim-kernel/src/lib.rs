// SPDX-License-Identifier: AGPL-3.0-or-later
//! The mic-rtsim engine kernel (PLAN.md §5.1): everything the generic modules stand on, and nothing
//! that knows about Minecraft, JNI or threads.
//!
//! - [`handle`]: generational arenas with 6-byte handles (SCALE.md §9).
//! - [`rng`]: the counter-based RNG keyed by (world seed, system, entity, slot) (PLAN.md §4.7).
//! - [`clock`]: sim time in ticks, 24,000 per day.
//! - [`wheel`]: the hierarchical timing wheel (PLAN.md §4.3).
//! - [`ladder`]: the detail-ladder operations fold, sample, query (SCALE.md §2).
//! - [`lazy`]: lazy regions with catch-up and breakpoints (SCALE.md §4).
//! - [`conserve`]: integer conservation and error diffusion (SCALE.md §9).
//! - [`dyadic`]: random counts over any interval in O(log horizon), consistent for every reader
//!   (DECISIONS.md O17).
//! - [`codec`] and [`page`]: page encoding and the incremental, crash-safe page store (SCALE.md §8,
//!   PLAN.md §4.9).
//! - [`journal`]: operation ids and idempotent replay (A9).
//! - [`paging`]: lazy regions paged to disk under a memory budget, with prefetch (A16, SCALE.md §8).
//! - [`exec`] and [`bsp`]: the executor the caller hands in, and the bulk-synchronous parallel
//!   step over paged regions (SCALE.md §5).
//! - [`profile`]: named zones, microseconds per system.
//! - [`store`]: the agent storage facade over plain columns (P4 stays open).
//! - [`synth`]: synthetic kinds for tests and the scale bench. Scaffolding, not content.
//!
//! The kernel owns no threads (PLAN.md §2): every entry point runs on the caller's thread, and
//! parallel work runs on an [`exec::Executor`] the caller provides.

pub mod bsp;
pub mod clock;
pub mod codec;
pub mod conserve;
pub mod dyadic;
pub mod exec;
pub mod handle;
pub mod journal;
pub mod ladder;
pub mod lazy;
pub mod page;
pub mod paging;
pub mod profile;
pub mod rng;
pub mod store;
pub mod synth;
pub mod wheel;

pub use clock::{TICKS_PER_DAY, Tick};
pub use handle::{Arena, Handle};
pub use rng::{SlotRng, StreamKey, SystemId};
pub use wheel::TimingWheel;
