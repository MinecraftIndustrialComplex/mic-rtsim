// SPDX-License-Identifier: AGPL-3.0-or-later
//! The headless host (VIEWER.md §12): mic-rtsim's engine run on a copy of a world's pages with no
//! Minecraft.
//!
//! - [`fork`]: `rtsim-cli export`, the copy of a world's rtsim state through the read-only open.
//! - [`run`]: `rtsim-cli run`, the engine on a fork with the Java side of an empty server.
//! - [`save`]: the Java side's files of a world (journal, `level.dat`, config, Deep Time layout).
//! - [`java`]: the fake Java side rtsim-jni's tests and the host share.
//! - [`nbt`]: Minecraft's NBT, read and written whole.
//! - [`view`]: the host source, the viewer served from the host's own saves.
pub mod fork;
pub mod java;
pub mod nbt;
pub mod run;
pub mod save;
pub mod view;
