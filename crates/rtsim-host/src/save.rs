// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Java side of a saved world, as the host reads it (and writes it into a fork): the journal
//! the bridge keeps (`data/mic_rtsim_journal.dat`, the core's `JournalSavedData`), the game time
//! (`level.dat`), the module switches (`serverconfig/mic_rtsim-server.toml`) and Deep Time's
//! layout block (`<folder>/manifest.json` `projection`), which gives the session's TOPOLOGY.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use rtsim_space::topology::Topology;
use rtsim_space::topology::layout;
use rtsim_space::topology::record::{self, Source};

use crate::java::WorldSave;
use crate::nbt::{self, Tag};

/// The engine's pages, under the world folder.
pub const PAGES: &str = "rtsim";
/// The bridge's journal and dictionary.
pub const JOURNAL: &str = "data/mic_rtsim_journal.dat";
pub const LEVEL: &str = "level.dat";
/// The core's server config (NeoForge's per-world `serverconfig`).
pub const SERVER_CONFIG: &str = "serverconfig/mic_rtsim-server.toml";
/// Deep Time's world-data folder when the generator names none (its `WorldDataRef`).
pub const DEEP_TIME_FOLDER: &str = "deeptime";

/// The core's modules in their Java order (`config/Module`): bit i is the i-th one.
pub const MODULES: [&str; 9] = [
    "knowledge",
    "settlements",
    "trade",
    "travel",
    "ecology",
    "conflict",
    "ships",
    "industry",
    "cosmos",
];
/// Every module on: the config's default.
pub const ALL_MODULES: u32 = (1 << MODULES.len()) - 1;

/// The journal file: what Java would restore at the next start.
#[derive(Clone)]
pub struct Journal {
    pub save: WorldSave,
    /// The file's `DataVersion`, kept when the host writes it again.
    pub data_version: Option<i32>,
}

impl Journal {
    /// A world that never ran the bridge.
    pub fn fresh() -> Journal {
        Journal {
            save: WorldSave::fresh(),
            data_version: None,
        }
    }

    /// The world's journal; [`Journal::fresh`] when the file does not exist.
    pub fn read(world: &Path) -> Result<Journal, String> {
        let path = world.join(JOURNAL);
        match fs::read(&path) {
            Ok(b) => Journal::parse(&b).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Journal::fresh()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// A journal file's bytes (gzipped NBT), read as the core's `JournalSavedData.load` does.
    pub fn parse(gz: &[u8]) -> Result<Journal, String> {
        let (_, root) = nbt::read_gz(gz)?;
        let data = root.get("data").ok_or("no data compound")?;
        let long = |n: &str| {
            data.get(n)
                .and_then(Tag::as_i64)
                .ok_or_else(|| format!("no {n}"))
        };
        let last_applied_cmd = long("lastAppliedCmdSeq")? as u64;
        let next_obs = long("nextObsSeq")? as u64;
        let seqs = match data.get("seqs") {
            Some(Tag::LongArray(v)) => v.clone(),
            None => Vec::new(),
            Some(_) => return Err("seqs is not a long array".into()),
        };
        let records: Vec<Vec<u8>> = match data.get("records") {
            Some(Tag::List(_, v)) => v
                .iter()
                .map(|t| match t {
                    Tag::ByteArray(b) => Ok(b.clone()),
                    _ => Err("a record that is not a byte array".to_string()),
                })
                .collect::<Result<_, _>>()?,
            None => Vec::new(),
            Some(_) => return Err("records is not a list".into()),
        };
        if seqs.len() != records.len() {
            return Err(format!(
                "corrupt: {} sequences, {} records",
                seqs.len(),
                records.len()
            ));
        }
        let kinds = match data.get("dictKinds") {
            Some(Tag::IntArray(v)) => v.clone(),
            None => Vec::new(),
            Some(_) => return Err("dictKinds is not an int array".into()),
        };
        let names: Vec<String> = match data.get("dictNames") {
            Some(Tag::List(_, v)) => v.iter().filter_map(Tag::as_str).collect(),
            None => Vec::new(),
            Some(_) => return Err("dictNames is not a list".into()),
        };
        if kinds.len() != names.len() {
            return Err(format!(
                "the dictionary is corrupt: {} kinds, {} names",
                kinds.len(),
                names.len()
            ));
        }
        let data_version = root
            .get("DataVersion")
            .and_then(Tag::as_i64)
            .map(|v| v as i32);
        Ok(Journal {
            save: WorldSave {
                journal: seqs
                    .into_iter()
                    .map(|s| s as u64)
                    .zip(records)
                    .collect::<VecDeque<_>>(),
                next_obs,
                last_applied_cmd,
                dict: kinds.into_iter().map(|k| k as u32).zip(names).collect(),
            },
            data_version,
        })
    }

    /// The file's bytes, as `JournalSavedData.save` writes them.
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = &self.save;
        let data = Tag::Compound(vec![
            (
                b"lastAppliedCmdSeq".to_vec(),
                Tag::Long(s.last_applied_cmd as i64),
            ),
            (b"nextObsSeq".to_vec(), Tag::Long(s.next_obs as i64)),
            (
                b"seqs".to_vec(),
                Tag::LongArray(s.journal.iter().map(|(q, _)| *q as i64).collect()),
            ),
            (
                b"records".to_vec(),
                Tag::List(
                    nbt::BYTE_ARRAY,
                    s.journal
                        .iter()
                        .map(|(_, r)| Tag::ByteArray(r.clone()))
                        .collect(),
                ),
            ),
            (
                b"dictKinds".to_vec(),
                Tag::IntArray(s.dict.iter().map(|(k, _)| *k as i32).collect()),
            ),
            (
                b"dictNames".to_vec(),
                Tag::List(
                    nbt::STRING,
                    s.dict.iter().map(|(_, n)| Tag::string(n)).collect(),
                ),
            ),
        ]);
        let mut root = Tag::Compound(vec![(b"data".to_vec(), data)]);
        if let Some(v) = self.data_version {
            root.put("DataVersion", Tag::Int(v));
        }
        nbt::write_gz(b"", &root)
    }

    /// Writes the file under the folder `dir` (a fork), through a temporary file and a rename.
    pub fn write(&self, dir: &Path) -> Result<(), String> {
        write_atomic(&dir.join(JOURNAL), &self.to_bytes())
    }

    /// The first observation sequence the journal holds (`nextObsSeq` when it holds none).
    pub fn first_obs(&self) -> u64 {
        self.save
            .journal
            .front()
            .map_or(self.save.next_obs, |(s, _)| *s)
    }
}

/// Writes `bytes` to `path` through `<path>.tmp`, an fsync and a rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// `level.dat`'s root, when the world has one.
pub fn level(world: &Path) -> Result<Option<Tag>, String> {
    let path = world.join(LEVEL);
    match fs::read(&path) {
        Ok(b) => nbt::read_gz(&b)
            .map(|(_, t)| Some(t))
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// The overworld's game time `level.dat` holds (`Data.Time`).
pub fn level_time(level: &Tag) -> Option<i64> {
    level.get("Data")?.get("Time")?.as_i64()
}

/// Sets `level.dat`'s game time in the folder `dir` (a fork), advancing the day clock
/// (`Data.DayTime`) by as much; nothing else in the file changes.
pub fn set_level_time(dir: &Path, time: i64) -> Result<(), String> {
    let path = dir.join(LEVEL);
    let b = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (name, mut root) = nbt::read_gz(&b).map_err(|e| format!("{}: {e}", path.display()))?;
    let data = root
        .get_mut("Data")
        .ok_or_else(|| format!("{}: no Data", path.display()))?;
    let was = data.get("Time").and_then(Tag::as_i64).unwrap_or(time);
    data.put("Time", Tag::Long(time));
    if let Some(day) = data.get("DayTime").and_then(Tag::as_i64) {
        data.put("DayTime", Tag::Long(day.saturating_add(time - was)));
    }
    write_atomic(&path, &nbt::write_gz(&name, &root))
}

/// Deep Time's world-data folder, as the overworld's generator in `level.dat` names it (its
/// `folder`), else [`DEEP_TIME_FOLDER`].
pub fn deep_time_folder(level: Option<&Tag>) -> String {
    fn find(t: &Tag) -> Option<String> {
        match t {
            Tag::Compound(es) => es
                .iter()
                .find_map(|(n, e)| if n == b"folder" { e.as_str() } else { find(e) }),
            _ => None,
        }
    }
    level
        .and_then(|l| l.get("Data")?.get("WorldGenSettings")?.get("dimensions"))
        .and_then(|d| d.get("minecraft:overworld"))
        .and_then(|o| o.get("generator"))
        .and_then(find)
        .unwrap_or_else(|| DEEP_TIME_FOLDER.to_string())
}

/// The module mask the world's server config gives (`effectiveMask`): every `[modules]` switch,
/// on unless the file says `false`; every module on without the file.
pub fn module_mask(world: &Path) -> u32 {
    let Ok(text) = fs::read_to_string(world.join(SERVER_CONFIG)) else {
        return ALL_MODULES;
    };
    let mut mask = ALL_MODULES;
    let mut in_modules = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_modules = line.trim_matches(['[', ']']).trim() == "modules";
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if !in_modules {
            continue;
        }
        let k = k.trim().trim_matches('"');
        if let Some(i) = MODULES.iter().position(|m| *m == k)
            && v.trim() == "false"
        {
            mask &= !(1 << i);
        }
    }
    mask
}

/// The overworld's topology from Deep Time's layout block, with where it came from.
pub struct Layout {
    pub topology: Topology,
    pub source: Source,
    pub manifest: PathBuf,
}

/// Deep Time's layout of the world in `world`, if it has a world-data folder with a layout block.
/// The edge_stop border holds (the wrap engine is not installed in shipped builds), as Java's
/// `DeepTimeLayoutSource` sees it. A Lambert block is Toroidal World's x wrap, as Java reports it.
pub fn deep_time_layout(world: &Path, folder: &str) -> Result<Option<Layout>, String> {
    let manifest = world.join(folder).join("manifest.json");
    let text = match fs::read_to_string(&manifest) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", manifest.display())),
    };
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let Some(p) = json.get("projection") else {
        return Ok(None);
    };
    let topology = layout::from_projection_json(&p.to_string(), false)
        .map_err(|e| format!("{}: the layout block: {e}", manifest.display()))?;
    let source = if p.get("kind").and_then(|k| k.as_str()) == Some("lambert") {
        Source::ToroidalWorld
    } else {
        Source::DeepTime
    };
    Ok(Some(Layout {
        topology,
        source,
        manifest,
    }))
}

impl Layout {
    /// The TOPOLOGY record for Java's dimension id `dim`.
    pub fn record(&self, dim: u32) -> Vec<u8> {
        record::encode(&layout::Spec::of(&self.topology), self.source, dim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rtsim-host-save-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_journal_round_trips_through_its_file() {
        let d = tmp("journal");
        assert_eq!(
            Journal::read(&d).unwrap().save.next_obs,
            1,
            "no file: fresh"
        );
        let mut j = Journal::fresh();
        j.data_version = Some(3955);
        j.save.last_applied_cmd = 41;
        j.save.next_obs = 12;
        j.save.journal.push_back((10, vec![1; 24]));
        j.save.journal.push_back((11, vec![2; 32]));
        j.save.dict = vec![(1, "minecraft:overworld".into()), (3, "x:y".into())];
        j.write(&d).unwrap();
        let back = Journal::read(&d).unwrap();
        assert_eq!(back.save.last_applied_cmd, 41);
        assert_eq!(back.save.next_obs, 12);
        assert_eq!(back.save.journal, j.save.journal);
        assert_eq!(back.save.dict, j.save.dict);
        assert_eq!(back.data_version, Some(3955));
        assert_eq!(back.first_obs(), 10);
        assert_eq!(back.to_bytes(), j.to_bytes());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_module_switches_read_from_the_server_config() {
        let d = tmp("config");
        assert_eq!(module_mask(&d), ALL_MODULES);
        fs::create_dir_all(d.join("serverconfig")).unwrap();
        fs::write(
            d.join(SERVER_CONFIG),
            "# c\n[modules]\n\t#x\n\tships = false\n\ttrade = true\n[budgets]\n\tindustry = false\n",
        )
        .unwrap();
        assert_eq!(module_mask(&d), ALL_MODULES & !(1 << 6));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn level_time_is_set_and_the_day_clock_follows() {
        let d = tmp("level");
        let root = Tag::Compound(vec![(
            b"Data".to_vec(),
            Tag::Compound(vec![
                (b"Time".to_vec(), Tag::Long(1000)),
                (b"DayTime".to_vec(), Tag::Long(6000)),
                (b"LevelName".to_vec(), Tag::string("w")),
            ]),
        )]);
        fs::write(d.join(LEVEL), nbt::write_gz(b"", &root)).unwrap();
        set_level_time(&d, 25_000).unwrap();
        let l = level(&d).unwrap().unwrap();
        assert_eq!(level_time(&l), Some(25_000));
        let data = l.get("Data").unwrap();
        assert_eq!(data.get("DayTime").and_then(Tag::as_i64), Some(30_000));
        assert_eq!(
            data.get("LevelName").and_then(Tag::as_str).as_deref(),
            Some("w")
        );
        assert_eq!(deep_time_folder(Some(&l)), DEEP_TIME_FOLDER);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_guyou_layout_block_gives_the_overworlds_topology() {
        let d = tmp("layout");
        assert!(deep_time_layout(&d, "deeptime").unwrap().is_none());
        fs::create_dir_all(d.join("deeptime")).unwrap();
        let spec = deeptime_spec();
        fs::write(
            d.join("deeptime/manifest.json"),
            format!("{{\"format\": \"0.5\", \"projection\": {spec}}}"),
        )
        .unwrap();
        let l = deep_time_layout(&d, "deeptime").unwrap().unwrap();
        assert_eq!(l.topology, layout::guyou(layout::GUYOU_16K, true));
        assert_eq!(l.source, Source::DeepTime);
        let rec = l.record(5);
        assert_eq!(rtsim_jni::wire::u16_at(&rec, 0), record::TOPOLOGY);
        let _ = fs::remove_dir_all(&d);
    }

    /// FORMAT.md's example block for the 16k Guyou world.
    fn deeptime_spec() -> &'static str {
        r#"{
  "kind": "petroff_guyou", "layout_version": 1, "hemisphere_blocks": 6656, "lambda_c_deg": 31.25,
  "domain": {"x_min": -6656.0, "x_max": 6656.0, "z_min": -3328.0, "z_max": 3328.0},
  "edges": [
    {"name": "west",  "kind": "slide", "axis": "x", "side": "min", "coord": -6656, "span": [-3328, 3328], "iso": [0, 13312, 0], "owner": "toroidal_world"},
    {"name": "east",  "kind": "slide", "axis": "x", "side": "max", "coord": 6656, "span": [-3328, 3328], "iso": [0, -13312, 0], "owner": "toroidal_world"},
    {"name": "north", "kind": "half_turn", "axis": "z", "side": "min", "coord": -3328, "span": [-6656, 6656], "iso": [2, 0, -6656], "pivot": [0, -3328], "owner": "wrap_engine"},
    {"name": "south", "kind": "half_turn", "axis": "z", "side": "max", "coord": 3328, "span": [-6656, 6656], "iso": [2, 0, 6656], "pivot": [0, 3328], "owner": "wrap_engine"}
  ],
  "cone_points": [], "poles": [],
  "border": {"kind": "edge_stop", "edges": ["north", "south"], "inset_blocks": 1.0, "temporary": true}
}"#
    }
}
