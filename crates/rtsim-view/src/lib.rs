// SPDX-License-Identifier: AGPL-3.0-or-later
//! The save viewer (VIEWER.md, stage V1): a saved world's `rtsim/` pages opened read only,
//! every module decoded as the native link's start decodes it, and served to the shared viewer
//! page (mic-viewer) as layers, cards and a HABITAT base map.
//!
//! - [`load`]: [`load::load_world`], the read-only open and every module's decode.
//! - [`industry`]: the industry pages without every district decoded at once.
//! - [`keys`]: where each module keeps its pages (mirrored from rtsim-jni).
//! - [`journal`]: Java's dictionary from the world's `data/mic_rtsim_journal.dat`.
//! - [`surveyor`]: Surveyor's terrain summaries as the base map over HABITAT (V5).
//! - [`layers`], [`parts`]: rtsim-layers' builders over a save, and the one reader that draws
//!   parts (a save's or the live view store's) into scenes.
//! - [`live`]: the live source (V4): the server's view store, its lease, and the save behind it.

pub mod cards;
pub mod events;
pub mod habitat;
pub mod industry;
pub mod journal;
pub mod keys;
pub mod layers;
pub mod live;
pub mod load;
pub mod parts;
pub mod surveyor;
pub mod view;

pub use load::{Limits, Saved, Status, load_world};
pub use view::View;
