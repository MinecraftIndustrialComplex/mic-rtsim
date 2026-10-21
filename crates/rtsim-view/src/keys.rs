// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where each module keeps its pages, mirrored from the native link (`rtsim-jni`'s `engine/*.rs`),
//! which this crate cannot link: rtsim-cli, which serves the viewer, sets its own global allocator,
//! and rtsim-jni is a mic_native plugin. A test (`tests/keys.rs`) checks every value here against
//! the link's own, and the populated-world test reads pages the link wrote.

use rtsim_kernel::page::PageKey;

/// A module's tables: its own and the row and index tables at plus 0x10 and up.
pub fn tables(base: u16) -> std::ops::RangeInclusive<u16> {
    base..=base + 0x1f
}

/// The page beside a module's meta page that holds its state hash (TEST saves only).
pub fn hash_key(meta: PageKey) -> PageKey {
    PageKey::new(meta.table(), meta.id() + 1)
}

/// The state hash page's magic, `RTSH`.
pub const HASH_MAGIC: u32 = u32::from_le_bytes(*b"RTSH");

/// Phase 1: the link's meta page and the input tail (0x5000), the snapshot (0x5001: the world's
/// meta page at id 0, the link's part in pieces after it).
pub mod phase1 {
    use rtsim_kernel::page::PageKey;

    pub const TABLE: u16 = 0x5000;
    pub const SNAPSHOT_TABLE: u16 = 0x5001;
    pub const KEY_META: PageKey = PageKey::new(TABLE, 0);
    pub const WORLD_META: PageKey = PageKey::new(SNAPSHOT_TABLE, 0);
    /// Inputs per log page.
    pub const CHUNK: usize = 1024;
    /// Snapshot bytes per page.
    pub const PIECE: usize = 1 << 20;
    pub const META_MAGIC: u32 = u32::from_le_bytes(*b"RTP1");
    pub const META_VERSION: u16 = 4;
    pub const SNAP_MAGIC: u32 = u32::from_le_bytes(*b"RTPS");
    /// The world's seed (a PLACEHOLDER in the link too: the bridge does not send Minecraft's).
    pub const WORLD_SEED: u64 = 0x6d69_635f_7031;

    pub fn chunk_key(k: usize) -> PageKey {
        PageKey::new(TABLE, 1 + k as u64)
    }

    pub fn piece_key(k: usize) -> PageKey {
        PageKey::new(SNAPSHOT_TABLE, 1 + k as u64)
    }
}

/// The dragons' meta page.
pub const DRAGONS: PageKey = PageKey::new(0x4400, 0);
/// The herds' meta page.
pub const HERDS: PageKey = PageKey::new(0x4800, 0);
/// The ships' meta page.
pub const SHIPS: PageKey = PageKey::new(0x5300, 0);
/// The conflict's meta page.
pub const CONFLICT: PageKey = PageKey::new(0x4300, 0);
/// The world model's meta page; its regions are table [`WORLD_MODEL_REGIONS`].
pub const WORLD_MODEL: PageKey = PageKey::new(0x5700, 0);
pub const WORLD_MODEL_REGIONS: u16 = 0x5701;
/// The industry engine's tables (`rtsim_industry::persist`): globals, districts, clusters.
pub const INDUSTRY_TABLES: [u16; 3] = [
    rtsim_industry::persist::GLOBAL_TABLE,
    rtsim_industry::persist::DISTRICT_TABLE,
    rtsim_industry::persist::CLUSTER_TABLE,
];

/// Java's dictionary kind of dimension names (docs/boundary.md §2).
pub const DICT_DIMENSION: u32 = 1;
/// Java's dictionary kind of names (district and station names; docs/industry-boundary.md §2.2).
pub const DICT_NAME: u32 = 0x11;
