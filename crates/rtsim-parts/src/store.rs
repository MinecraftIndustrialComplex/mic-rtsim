// SPDX-License-Identifier: AGPL-3.0-or-later
//! The view store and the lease (VIEWER.md §5.2): where the tap's commits go and what the sidecar
//! writes to ask for them.
//!
//! - `<world>/rtsim/view.lease`: the sidecar's process id and its focus (at most
//!   [`MAX_FOCUS`] region keys), written whole (a temporary file renamed over it).
//! - `<world>/rtsim/view/<epoch>/`: a page store the tap commits into without fsync. Page
//!   [`HEADER_TABLE`] 0 is the [`Header`]; [`LAYERS_TABLE`] 0 is the layer list as JSON (so the
//!   store describes itself, and a reader needs no module code); [`PART_TABLE`] holds the parts,
//!   one page each, under the page ids the header's directory gives.

use std::path::{Path, PathBuf};

use rtsim_kernel::codec::{CodecError, Reader, Writer};

use crate::mark::{get_str, opt_str, string};
use crate::part::{PartKey, RegionKey};

pub const HEADER_TABLE: u16 = 0x7600;
pub const LAYERS_TABLE: u16 = 0x7601;
pub const PART_TABLE: u16 = 0x7602;

/// The lease's file name, in `<world>/rtsim/`.
pub const LEASE_FILE: &str = "view.lease";
/// The view store's folder, in `<world>/rtsim/`.
pub const VIEW_DIR: &str = "view";
/// The most focus regions a lease holds (TUNABLE).
pub const MAX_FOCUS: usize = 64;

const HEADER_MAGIC: u32 = u32::from_le_bytes(*b"MICH");
const HEADER_VERSION: u8 = 2;
const LEASE_MAGIC: u32 = u32::from_le_bytes(*b"MICL");
const LEASE_VERSION: u8 = 1;

/// `<rtsim>/view/<epoch>`, the epoch in 16 hex digits so folders sort by epoch.
pub fn epoch_dir(rtsim: &Path, epoch: u64) -> PathBuf {
    rtsim.join(VIEW_DIR).join(format!("{epoch:016x}"))
}

/// The epochs under `<rtsim>/view/`, oldest first.
pub fn epochs(rtsim: &Path) -> Vec<u64> {
    let mut out: Vec<u64> = std::fs::read_dir(rtsim.join(VIEW_DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let name = name.to_str()?;
            (name.len() == 16)
                .then(|| u64::from_str_radix(name, 16).ok())
                .flatten()
        })
        .collect();
    out.sort_unstable();
    out
}

/// A layer as of the last commit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerState {
    /// Changes when any of its parts changed.
    pub version: u64,
    /// Rows in its parts.
    pub rows: u64,
    /// Keys a unit, as the engine bucketed the layer (0: not said). A reader whose own layer
    /// buckets differently takes the units the engine evicted from the sim pages by this one.
    pub per_unit: u64,
    /// Why it draws nothing at all (its module is off), if so.
    pub status: Option<String>,
    /// Why it would be empty, its builder's words (shown when it has no rows).
    pub empty: Option<String>,
}

/// One stored part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartEntry {
    pub region: Option<RegionKey>,
    pub page: u64,
    pub crc: u32,
    pub rows: u32,
}

/// One encoded unit: its parts, and whether everything it reads was resident.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitEntry {
    /// The unit's key (no region).
    pub key: PartKey,
    /// False when the encode met state the engine has evicted (or failed): the reader takes this
    /// unit from the sim pages instead (VIEWER.md §5.2 item 6).
    pub resident: bool,
    /// Game tick it was encoded at.
    pub at: i64,
    pub parts: Vec<PartEntry>,
}

/// The header page (VIEWER.md §5.2 item 3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub epoch: u64,
    /// Commits in this epoch, this one included.
    pub commit: u64,
    pub game_time: i64,
    /// Unix milliseconds at the commit.
    pub wall_ms: u64,
    /// Ticks per second the engine measured.
    pub tps: f64,
    /// The engine's build.
    pub build: String,
    /// The last whole cycle's length (every unit encoded once), milliseconds; 0 before the first.
    pub cycle_ms: u64,
    /// Whole cycles done in this epoch. Until the first, a unit missing from the directory has not
    /// been encoded yet; after it, a unit missing has no rows.
    pub cycles: u64,
    /// By layer index.
    pub layers: Vec<LayerState>,
    /// The resident set, unit by unit: every unit encoded this epoch.
    pub units: Vec<UnitEntry>,
    /// The tap's own counters (`/api/status`): name and value.
    pub stats: Vec<(String, u64)>,
}

impl Header {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(HEADER_MAGIC);
        w.u8(HEADER_VERSION);
        w.var_u64(self.epoch);
        w.var_u64(self.commit);
        w.var_i64(self.game_time);
        w.var_u64(self.wall_ms);
        w.f64(self.tps);
        w.bytes(self.build.as_bytes());
        w.var_u64(self.cycle_ms);
        w.var_u64(self.cycles);
        w.var_u64(self.layers.len() as u64);
        for l in &self.layers {
            w.var_u64(l.version);
            w.var_u64(l.rows);
            w.var_u64(l.per_unit);
            opt_str(&mut w, &l.status);
            opt_str(&mut w, &l.empty);
        }
        w.var_u64(self.units.len() as u64);
        for u in &self.units {
            u.key.encode(&mut w);
            w.bool(u.resident);
            w.var_i64(u.at);
            w.var_u64(u.parts.len() as u64);
            for p in &u.parts {
                match p.region {
                    Some(r) => {
                        w.bool(true);
                        w.var_u64(r.pack());
                    }
                    None => w.bool(false),
                }
                w.var_u64(p.page);
                w.u32(p.crc);
                w.var_u64(u64::from(p.rows));
            }
        }
        w.var_u64(self.stats.len() as u64);
        for (k, v) in &self.stats {
            w.bytes(k.as_bytes());
            w.var_u64(*v);
        }
        w.into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Result<Header, String> {
        Self::decode_inner(bytes).map_err(|e| format!("view header: {e}"))
    }

    fn decode_inner(bytes: &[u8]) -> Result<Header, CodecError> {
        let mut r = Reader::new(bytes);
        if r.u32()? != HEADER_MAGIC {
            return Err(CodecError::Invalid("bad magic".into()));
        }
        let v = r.u8()?;
        if v != HEADER_VERSION {
            return Err(CodecError::Invalid(format!("version {v}")));
        }
        let mut h = Header {
            epoch: r.var_u64()?,
            commit: r.var_u64()?,
            game_time: r.var_i64()?,
            wall_ms: r.var_u64()?,
            tps: r.f64()?,
            build: string(&mut r)?,
            cycle_ms: r.var_u64()?,
            cycles: r.var_u64()?,
            ..Header::default()
        };
        let n = r.len_prefix(4)?;
        for _ in 0..n {
            h.layers.push(LayerState {
                version: r.var_u64()?,
                rows: r.var_u64()?,
                per_unit: r.var_u64()?,
                status: get_str(&mut r)?,
                empty: get_str(&mut r)?,
            });
        }
        let n = r.len_prefix(6)?;
        for _ in 0..n {
            let key = PartKey::decode(&mut r)?;
            let resident = r.bool()?;
            let at = r.var_i64()?;
            let m = r.len_prefix(7)?;
            let mut parts = Vec::with_capacity(m);
            for _ in 0..m {
                let region = if r.bool()? {
                    Some(RegionKey::unpack(r.var_u64()?))
                } else {
                    None
                };
                parts.push(PartEntry {
                    region,
                    page: r.var_u64()?,
                    crc: r.u32()?,
                    rows: r.var_u64()? as u32,
                });
            }
            h.units.push(UnitEntry {
                key,
                resident,
                at,
                parts,
            });
        }
        let n = r.len_prefix(2)?;
        for _ in 0..n {
            h.stats.push((string(&mut r)?, r.var_u64()?));
        }
        r.finish()?;
        Ok(h)
    }

    /// A counter from [`Header::stats`].
    pub fn stat(&self, name: &str) -> Option<u64> {
        self.stats.iter().find(|(k, _)| k == name).map(|(_, v)| *v)
    }
}

/// What the sidecar asks for (VIEWER.md §5.2 item 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lease {
    pub pid: u32,
    /// Regions open pages show at the Group level or deeper, sorted by key with no repeats, at
    /// most [`MAX_FOCUS`] (the sidecar keeps the nearest ones when its pages show more).
    pub focus: Vec<RegionKey>,
}

impl Lease {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(LEASE_MAGIC);
        w.u8(LEASE_VERSION);
        w.u32(self.pid);
        let n = self.focus.len().min(MAX_FOCUS);
        w.var_u64(n as u64);
        for r in &self.focus[..n] {
            w.u64(r.pack());
        }
        w.into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Result<Lease, String> {
        let bad = |e: CodecError| format!("view lease: {e}");
        let mut r = Reader::new(bytes);
        if r.u32().map_err(bad)? != LEASE_MAGIC {
            return Err("view lease: bad magic".into());
        }
        if r.u8().map_err(bad)? != LEASE_VERSION {
            return Err("view lease: unknown version".into());
        }
        let pid = r.u32().map_err(bad)?;
        let n = r.len_prefix(8).map_err(bad)?.min(MAX_FOCUS);
        let mut focus = Vec::with_capacity(n);
        for _ in 0..n {
            focus.push(RegionKey::unpack(r.u64().map_err(bad)?));
        }
        Ok(Lease { pid, focus })
    }

    /// Writes the lease whole: a temporary file in the same folder, renamed over the old one.
    pub fn write(&self, rtsim: &Path) -> std::io::Result<()> {
        let path = rtsim.join(LEASE_FILE);
        let tmp = rtsim.join(format!("{LEASE_FILE}.{}.tmp", self.pid));
        std::fs::write(&tmp, self.encode())?;
        rtsim_kernel::page::rename_over(&tmp, &path)
    }

    pub fn read(rtsim: &Path) -> std::io::Result<Lease> {
        let b = std::fs::read(rtsim.join(LEASE_FILE))?;
        Lease::decode(&b).map_err(std::io::Error::other)
    }
}
