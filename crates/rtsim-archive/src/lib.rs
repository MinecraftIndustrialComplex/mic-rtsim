// SPDX-License-Identifier: AGPL-3.0-or-later
//! The event archive (VIEWER.md §10, VW4): what happened in a world, recorded while the engine runs,
//! for the viewer's timeline.
//!
//! - [`row`]: the two 32-byte rows, events and legs.
//! - [`dict`]: kind and frame names and big ids, numbered on first use.
//! - [`kinds`]: the kinds recorders write, with the labels and severities the page shows.
//! - [`pending`]: the step's buffer, capped, dropping the lowest severity first.
//! - [`record`]: a module's [`Tape`] and the engine's [`Recorder`] over the pending buffer.
//! - [`counts`]: the count tier's 16-byte rows, per region per day and coarser.
//! - [`chunk`]: a chunk file of up to 4,096 rows, LZ4 and CRC32, with the header the index keeps.
//! - [`store`]: the folder: the engine's [`Archive`] (flush, rollback cut, budget and compaction)
//!   and the viewer's read-only [`ArchiveReader`] (histogram from headers, rows in a span, legs at a
//!   tick, count rows as heat).

pub mod chunk;
pub mod counts;
pub mod dict;
pub mod kinds;
pub mod pending;
pub mod record;
pub mod row;
pub mod store;

pub use chunk::{Chunk, ChunkHeader, ROWS_PER_CHUNK, Stamp, Tier};
pub use counts::{Count, TICKS_PER_DAY};
pub use dict::Dictionary;
pub use kinds::Kind;
pub use pending::{Batch, Dropped, Pending};
pub use record::{At, Recorder, Tape};
pub use row::{Event, Handle, Leg, RawRow, Row, Severity};
pub use store::{Archive, ArchiveReader, Budget, Compaction, Entry, Index, Record, dir_for};
