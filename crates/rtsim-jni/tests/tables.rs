// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engines' page tables against the lists that load them (RP-285).
//!
//! The industry link loads its pages through `TABLES` (`Link::load` reads `stored_keys`, a fresh
//! start deletes what `deletions` finds there). A table left out of that list is written at every
//! save and never read back: the package stores' (0x4906) was, so on main a restart lost every
//! store. These tests fail the moment a table is absent from the list that owns it, so the next
//! new table cannot slip through.
//!
//! (a) scans each engine crate's `src` for table constants and page-key literals; (b) saves a real
//! industry engine and checks every page it writes is in the link's list.
//!
//! The scan skips everything from the first `#[cfg(test)]` of a file on (the inline test modules
//! are last in these crates), and tests, benches and rtsim-cli are not scanned.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use rtsim_industry::engine::Engine;
use rtsim_industry::fake::FakeWorld;
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec, StoreId};
use rtsim_industry::learn::{Learned, LinkKey};
use rtsim_industry::obs::{
    ChainDirection, ChainPackage, DistrictDecl, InvKind, InventoryDecl, LoadState, Obs, StoreDecl,
    StoreFrogport, SweepEnd, Timed,
};
use rtsim_industry::persist::{
    CLUSTER_TABLE, DISTRICT_TABLE, GLASS_TABLE, GLOBAL_TABLE, LEG_TABLE, STORE_TABLE,
};
use rtsim_industry::scenario::{IronLine, iron_line, run_iron_line_until};
use rtsim_industry::tunables::Tunables;
use rtsim_jni::engine::mirrored::INDUSTRY_TABLES;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

// ---- (a) the source scan -------------------------------------------------------------------

/// Tables a crate defines that no TABLES list holds, because the link loads them by an explicit
/// key and not by listing a table: (crate, table, where it is loaded).
const LOADED_BY_KEY: &[(&str, u16, &str)] = &[
    // rtsim-conflict's scenario.rs names the meta page's key `PageKey::new(0x4300, 0)`; the link
    // reads it as KEY_CONFLICT (crates/rtsim-jni/src/engine/conflict.rs, TABLE 0x4300, id 0).
    (
        "rtsim-conflict",
        0x4300,
        "jni engine/conflict.rs KEY_CONFLICT",
    ),
];

/// One `const NAME: u16 = 0x....;` with TABLE in its name, or one `PageKey::new(0x4.., ..)` literal.
#[derive(Debug)]
struct Found {
    what: String,
    value: u16,
    file: PathBuf,
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The text of a source file up to its inline test module.
fn production_text(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    match text.find("#[cfg(test)]") {
        Some(i) => text[..i].to_string(),
        None => text,
    }
}

fn hex_u16(s: &str) -> Option<u16> {
    let digits: String = s.chars().take_while(char::is_ascii_hexdigit).collect();
    u16::from_str_radix(&digits, 16).ok()
}

/// Every table constant and page-key literal of a crate's `src`, whose files `keep` accepts.
fn scan(krate: &str, keep: impl Fn(&Path) -> bool) -> Vec<Found> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(krate)
        .join("src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    assert!(!files.is_empty(), "no sources under {}", root.display());
    let mut found = Vec::new();
    for file in files.into_iter().filter(|f| keep(f)) {
        let text = production_text(&file);
        for line in text.lines() {
            let l = line.trim();
            let l = l
                .strip_prefix("pub(crate) ")
                .or_else(|| l.strip_prefix("pub "))
                .unwrap_or(l);
            let Some(rest) = l.strip_prefix("const ") else {
                continue;
            };
            let Some((name, tail)) = rest.split_once(':') else {
                continue;
            };
            let Some(value) = tail.trim().strip_prefix("u16 = 0x") else {
                continue;
            };
            if name.contains("TABLE")
                && let Some(value) = hex_u16(value)
            {
                found.push(Found {
                    what: format!("const {name}"),
                    value,
                    file: file.clone(),
                });
            }
        }
        // Literal keys: `PageKey::new(0x4...` or `0x5...` (whitespace and newlines allowed).
        let mut at = 0;
        while let Some(i) = text[at..].find("PageKey::new(") {
            let rest = text[at + i + "PageKey::new(".len()..].trim_start();
            at += i + "PageKey::new(".len();
            if let Some(hex) = rest.strip_prefix("0x")
                && let Some(value) = hex_u16(hex)
                && matches!(value >> 12, 4 | 5)
            {
                found.push(Found {
                    what: "PageKey::new literal".into(),
                    value,
                    file: file.clone(),
                });
            }
        }
    }
    found
}

/// Asserts every found table is in `list` or allowed by key; returns the failures' text.
fn missing(krate: &str, found: &[Found], list: &[u16], list_name: &str) -> Vec<String> {
    found
        .iter()
        .filter(|f| !list.contains(&f.value))
        .filter(|f| {
            !LOADED_BY_KEY
                .iter()
                .any(|(c, v, _)| *c == krate && *v == f.value)
        })
        .map(|f| {
            format!(
                "{krate}: {} = {:#06x} ({}) is in no {list_name} and not in LOADED_BY_KEY",
                f.what,
                f.value,
                f.file.display()
            )
        })
        .collect()
}

fn assert_none_missing(failures: Vec<String>) {
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn every_industry_table_is_in_the_links_list() {
    let found = scan("rtsim-industry", |_| true);
    // The scan sees the six tables the list holds, so it is reading the right file.
    for t in [
        GLOBAL_TABLE,
        DISTRICT_TABLE,
        CLUSTER_TABLE,
        GLASS_TABLE,
        LEG_TABLE,
        STORE_TABLE,
    ] {
        assert!(found.iter().any(|f| f.value == t), "scan missed {t:#06x}");
    }
    assert_none_missing(missing(
        "rtsim-industry",
        &found,
        &INDUSTRY_TABLES,
        "rtsim-jni TABLES (the link's, which loads)",
    ));
}

#[test]
fn the_viewers_industry_tables_are_the_links() {
    let link: BTreeSet<u16> = INDUSTRY_TABLES.into_iter().collect();
    let view: BTreeSet<u16> = rtsim_view::keys::INDUSTRY_TABLES.into_iter().collect();
    assert_eq!(
        link, view,
        "rtsim-view keys.rs INDUSTRY_TABLES vs the link's"
    );
}

#[test]
fn every_ecology_table_is_in_its_list() {
    let in_herds = |f: &Path| f.components().any(|c| c.as_os_str() == "herds");
    let dragons = scan("rtsim-ecology", |f| !in_herds(f));
    let herds = scan("rtsim-ecology", |f| in_herds(f));
    assert!(
        dragons.len() >= 10 && herds.len() >= 5,
        "{dragons:?} {herds:?}"
    );
    let mut failures = missing(
        "rtsim-ecology",
        &dragons,
        &rtsim_ecology::DRAGON_PAGE_TABLES,
        "rtsim-ecology persist.rs TABLES (dragons, lairs, oil, nests)",
    );
    failures.extend(missing(
        "rtsim-ecology",
        &herds,
        &rtsim_ecology::herds::PAGE_TABLES,
        "rtsim-ecology herds/persist.rs TABLES",
    ));
    assert_none_missing(failures);
}

#[test]
fn every_conflict_table_is_in_its_list() {
    let found = scan("rtsim-conflict", |_| true);
    assert!(found.len() >= 9, "{found:?}");
    assert_none_missing(missing(
        "rtsim-conflict",
        &found,
        &rtsim_conflict::PAGE_TABLES,
        "rtsim-conflict persist.rs TABLES",
    ));
}

#[test]
fn the_allowlist_holds_only_what_is_still_used() {
    // An allowlisted table that no source names any more is a stale excuse.
    for (krate, value, why) in LOADED_BY_KEY {
        let found = scan(krate, |_| true);
        assert!(
            found.iter().any(|f| f.value == *value),
            "{krate} no longer defines {value:#06x} ({why}): drop it from LOADED_BY_KEY"
        );
    }
}

// ---- (b) the runtime check -----------------------------------------------------------------

const IRON: Good = Good(1);
const MINE: DistrictId = DistrictId(1);
const MILL: DistrictId = DistrictId(2);
const S: StoreId = StoreId(5);
const PORT: u32 = 1;
const BATCH: i64 = 64;

fn frog(d: DistrictId) -> BlockPos {
    BlockPos::new(if d == MINE { 10 } else { 20 }, 64, 0)
}

fn vault(d: DistrictId) -> BlockPos {
    BlockPos::new(if d == MINE { -100 } else { 200 }, 64, 0)
}

fn send(e: &mut Engine, at: u64, obs: Obs) {
    e.ingest(&Timed { at: Tick(at), obs });
}

/// Copied from crates/rtsim-industry/tests/chain_replay.rs: a store with a frogport in each district.
fn store_decl() -> StoreDecl {
    let port = |d: DistrictId| StoreFrogport {
        district: Some(d),
        inventory: Some(PORT),
        star: false,
        pos: frog(d),
        conveyor: BlockPos::new(frog(d).x + 1, 64, 0),
        from: None,
    };
    StoreDecl {
        store: S,
        dim: 0,
        epoch: 0,
        into: None,
        frogports: vec![port(MINE), port(MILL)],
        hops: vec![1],
    }
}

fn chain(d: DistrictId, packages: u32) -> Obs {
    Obs::ChainPackage(ChainPackage {
        store: S,
        district: Some(d),
        inventory: Some(PORT),
        direction: ChainDirection::Out,
        frogport: if d == MINE { 0 } else { 1 },
        pos: frog(d),
        destination: Some(1),
        packages,
        address: 0,
        op: None,
        key: None,
        goods: GoodVec::from_pairs([(IRON, BATCH * i64::from(packages))]),
    })
}

fn live(e: &mut Engine, d: DistrictId, at: u64) {
    for p in [vault(d), frog(d)] {
        send(
            e,
            at,
            Obs::ChunkLoaded {
                district: d,
                chunk: ChunkPos {
                    dim: 0,
                    x: p.x >> 4,
                    z: p.z >> 4,
                },
                applied_epoch: 0,
            },
        );
    }
    send(
        e,
        at,
        Obs::LoadState {
            district: d,
            state: LoadState::Live,
            ticking_chunks: 2,
            total_chunks: 2,
        },
    );
}

fn away(e: &mut Engine, d: DistrictId, at: u64) {
    send(
        e,
        at,
        Obs::LoadState {
            district: d,
            state: LoadState::Unloaded,
            ticking_chunks: 0,
            total_chunks: 2,
        },
    );
    e.advance_to(Tick(at + 1));
}

/// Copied from chain_replay.rs: a live district with a vault and a frogport, learned with `rate`
/// iron a tick and a chain link of `link` toward the mill's frogport.
fn district(e: &mut Engine, d: DistrictId, stock: i64, rate: f64, link: f64) {
    send(
        e,
        0,
        Obs::District(DistrictDecl {
            district: d,
            revision: 1,
            dim: 0,
            name: 0,
            not_learnable: None,
            inventory_count: 2,
        }),
    );
    for (index, pos, kind, slots) in [
        (0u32, vault(d), InvKind::ITEMS, 100u32),
        (PORT, frog(d), InvKind::ITEMS | InvKind::POSTBOX, 18),
    ] {
        send(
            e,
            0,
            Obs::Inventory(InventoryDecl {
                district: d,
                revision: 1,
                index,
                pos,
                kind: InvKind(kind),
                item_slots: slots,
                fluid_capacity: 0,
                energy_capacity: 0,
            }),
        );
    }
    live(e, d, 0);
    for (inventory, n) in [(0, stock), (PORT, 0)] {
        send(
            e,
            10,
            Obs::StockSample {
                district: d,
                inventory,
                sweep: 1,
                goods: GoodVec::from_pairs([(IRON, n)]),
            },
        );
    }
    send(
        e,
        10,
        Obs::SweepEnd(SweepEnd {
            district: d,
            sweep: 1,
            records: 2,
            inventories: 2,
            live: true,
            creative: false,
            overstressed: false,
        }),
    );
    e.adopt_model(
        d,
        Learned {
            at: Tick(20),
            window_ticks: 12_000,
            rates: vec![(IRON, rate)],
            links: vec![(
                LinkKey::Chain {
                    inventory: PORT,
                    to: frog(MILL),
                    good: IRON,
                },
                link,
            )],
            confidence: 1.0,
            points: 0,
            longest_gap: 0,
            latency_ticks: 0,
        },
        vec![],
    );
}

/// The mine and the mill on one store with a package booked, both districts away: a cluster
/// replays them around the store's pool (chain_replay.rs's `pair` and `away`).
fn chain_pair() -> Engine {
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    e.define_good(IRON, GoodInfo::item("iron", 64));
    send(&mut e, 0, Obs::PackageStore(store_decl()));
    district(&mut e, MINE, 1_000, 0.05, -0.05);
    district(&mut e, MILL, 0, -0.04, 0.05);
    send(&mut e, 15, chain(MINE, 1));
    away(&mut e, MINE, 100);
    away(&mut e, MILL, 100);
    e.advance_to(Tick(6_100));
    e.drain_commands();
    e
}

/// The iron line (districts, glass boxes, train legs through `threats`), then a store with a
/// package in it (the line has no chain network of its own).
fn iron_line_with_a_store() -> FakeWorld {
    let c = IronLine {
        seed: 5,
        days: 10,
        threats: true,
        control: false,
        approach: true,
        tick_first: true,
    };
    let mut w = iron_line(c, Tunables::default());
    w.check_saves = false;
    run_iron_line_until(&mut w, &c, Tick(3 * TICKS_PER_DAY));
    let now = w.engine.now().0;
    send(&mut w.engine, now, Obs::PackageStore(store_decl()));
    send(&mut w.engine, now, chain(MINE, 1));
    w
}

fn tables_of(e: &Engine) -> BTreeMap<u16, usize> {
    let mut by = BTreeMap::new();
    for (k, _) in e.snapshot() {
        *by.entry(k.table()).or_default() += 1;
    }
    by
}

#[test]
fn every_page_the_industry_engine_saves_is_in_the_links_list() {
    let mut reached: BTreeMap<u16, usize> = BTreeMap::new();
    for (name, tables) in [
        ("chain pair", tables_of(&chain_pair())),
        (
            "iron line with a store",
            tables_of(&iron_line_with_a_store().engine),
        ),
    ] {
        for (&t, &n) in &tables {
            *reached.entry(t).or_default() += n;
        }
        let stray: Vec<String> = tables
            .keys()
            .filter(|t| !INDUSTRY_TABLES.contains(t))
            .map(|t| format!("{t:#06x}"))
            .collect();
        assert!(
            stray.is_empty(),
            "{name}: the engine saves pages in tables {stray:?} that the link's TABLES does not \
             list, so a restart never reads them back and a fresh start never deletes them"
        );
    }
    // The fixtures reach these tables (the check is only as strong as that): a change that
    // stops them reaching one must say so here and not pass unnoticed.
    let unreached: Vec<String> = INDUSTRY_TABLES
        .iter()
        .filter(|t| !reached.contains_key(t))
        .map(|t| format!("{t:#06x}"))
        .collect();
    assert!(
        reached.contains_key(&STORE_TABLE),
        "the fixtures no longer reach the package stores' table"
    );
    eprintln!("tables reached: {reached:#x?}; not reached: {unreached:?}");
    assert!(
        unreached.is_empty(),
        "tables of the link's list the fixtures did not reach: {unreached:?}"
    );
}
