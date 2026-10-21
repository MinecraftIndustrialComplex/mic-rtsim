// SPDX-License-Identifier: AGPL-3.0-or-later
//! The viewer's part format (VIEWER.md §4.3, §5.2): what every source writes and the one reader in
//! the core reads, with no module code on the reading side.
//!
//! - [`mark`]: the closed set of mark kinds, mirrored from mic-viewer so the engine can write them
//!   without linking the viewer's server.
//! - [`desc`]: layer descriptors, the same JSON as mic-viewer's.
//! - [`part`]: part keys, regions, the writer and the reader.
//! - [`store`]: the view store's header page and the lease.
//! - [`source`]: the `ViewSource` trait.

pub mod desc;
pub mod mark;
pub mod part;
pub mod source;
pub mod store;

pub use desc::{Group, LayerDesc, Layers};
pub use mark::{Mark, Tick};
pub use part::{MAX_ROWS, Part, PartKey, PartView, PartWriter, REGION, RegionKey, Row, Value, read_part};
pub use source::ViewSource;
pub use store::{Header, LayerState, Lease, PartEntry, UnitEntry};
