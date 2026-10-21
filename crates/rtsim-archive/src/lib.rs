// SPDX-License-Identifier: AGPL-3.0-or-later
//! The event archive (VIEWER.md §10, VW4): what happened in a world, recorded while the engine runs,
//! for the viewer's timeline.
//!
//! - [`row`]: the two 32-byte rows, events and legs.
//! - [`dict`]: kind and frame names and big ids, numbered on first use.
//! - [`pending`]: the step's buffer, capped, dropping the lowest severity first.
//! - [`chunk`]: a chunk file of up to 4,096 rows, LZ4 and CRC32, with the header the index keeps.
//! - [`store`]: the folder: the engine's [`Archive`] (flush, rollback cut, replacement) and the
//!   viewer's read-only [`ArchiveReader`] (histogram from headers, rows in a span, legs at a tick).
//!
//! The engine hooks (each module's events), the flush at `save_now`, the tiers' compaction and the
//! budget are built on this crate (VIEWER.md §16, V3).

pub mod chunk;
pub mod dict;
pub mod pending;
pub mod row;
pub mod store;

pub use chunk::{Chunk, ChunkHeader, ROWS_PER_CHUNK, Stamp, Tier};
pub use dict::Dictionary;
pub use pending::{Batch, Dropped, Pending};
pub use row::{Event, Handle, Leg, RawRow, Row, Severity};
pub use store::{Archive, ArchiveReader, Entry, Index, Record, dir_for};
