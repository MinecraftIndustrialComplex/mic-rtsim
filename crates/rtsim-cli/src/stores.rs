// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli stores <world> [--json]`: the package stores of a saved world (SECTIONS.md §8.2,
//! slice S7), read from the confirmed save through the read-only open, as `rtsim-cli glass` reads
//! the glass pages. A package store (§2.5) is a chain network whose frogports sit in more than one
//! district. Per store: its id and dimension, its frogports (position, district, star), what it
//! holds for each frogport and in the no-route pool (packages, goods by name), the last census's
//! residue, the packages the frontier holds toward each frogport (count and keys), the drawn keys
//! and the store's counts.

use std::path::Path;
use std::process::ExitCode;

use rtsim_industry::ids::{Good, GoodVec};
use rtsim_industry::persist::STORE_TABLE;
use rtsim_industry::stores::{Held, PackageStore, Pool, read_store_page};
use rtsim_kernel::page::PageKey;
use rtsim_kernel::rows::PageSource;
use rtsim_view::industry::IndustryView;
use rtsim_view::journal::Names;
use rtsim_view::keys::{DICT_DIMENSION, DICT_NAME};
use rtsim_view::{Limits, load_world};
use serde_json::{Value, json};

const USAGE: &str = "usage: rtsim-cli stores <world> [--json]";

pub fn cli(args: &[&str]) -> ExitCode {
    let mut world = None;
    let mut as_json = false;
    for a in args {
        match *a {
            "--json" => as_json = true,
            a if a.starts_with("--") => {
                eprintln!("unknown option {a}\n{USAGE}");
                return ExitCode::from(64);
            }
            a if world.is_none() => world = Some(a),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(64);
            }
        }
    }
    let Some(world) = world else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    match report(Path::new(world)) {
        Ok(v) => {
            if as_json {
                println!("{}", serde_json::to_string_pretty(&v).expect("serialize"));
            } else {
                print_text(&v);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rtsim-cli stores: {e}");
            ExitCode::FAILURE
        }
    }
}

fn report(world: &Path) -> Result<Value, String> {
    let saved = load_world(world, Limits::default())?;
    let Some(iv) = saved.industry.as_ref() else {
        return Err(format!("{} has no industry pages", world.display()));
    };
    let ids = saved
        .source
        .table_ids(STORE_TABLE)
        .map_err(|e| format!("cannot list the store pages: {e}"))?;
    let mut stores = Vec::new();
    for id in ids {
        let b = saved
            .source
            .read(PageKey::new(STORE_TABLE, id))
            .map_err(|e| format!("store page {id}: {e}"))?
            .ok_or_else(|| format!("store page {id} is listed and missing"))?;
        let s = read_store_page(id, &b).map_err(|e| format!("store page {id}: {e}"))?;
        stores.push(store(
            &SavedNames {
                iv,
                names: &saved.names,
            },
            &s,
        ));
    }
    Ok(json!({
        "world": saved.scrub(&world.display().to_string()),
        "now": iv.now,
        "stores": stores,
    }))
}

/// How a store's ids read: names for goods, districts and the dimension.
trait Namer {
    fn good(&self, g: Good) -> String;
    fn district(&self, id: u64) -> Option<String>;
    fn dimension(&self, dim: u32) -> String;
}

/// The viewer's names, from the save.
struct SavedNames<'a> {
    iv: &'a IndustryView,
    names: &'a Names,
}

impl Namer for SavedNames<'_> {
    fn good(&self, g: Good) -> String {
        self.iv.good_name(g)
    }

    fn district(&self, id: u64) -> Option<String> {
        self.iv
            .districts
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| self.names.get(DICT_NAME, d.name))
            .map(str::to_string)
    }

    fn dimension(&self, dim: u32) -> String {
        self.names
            .get(DICT_DIMENSION, dim)
            .map_or_else(|| format!("dim{dim}"), str::to_owned)
    }
}

fn pos(p: rtsim_industry::ids::BlockPos) -> [i32; 3] {
    [p.x, p.y, p.z]
}

fn goods(n: &dyn Namer, g: &GoodVec) -> Value {
    g.iter()
        .filter(|&(_, a)| a != 0)
        .map(|(g, amount)| json!({"good": n.good(g), "amount": amount}))
        .collect::<Vec<_>>()
        .into()
}

fn pool(n: &dyn Namer, p: &Pool) -> Value {
    json!({"packages": p.packages, "goods": goods(n, &p.goods)})
}

fn held(h: &[Held]) -> Value {
    json!({
        "count": h.iter().map(|x| x.packages).sum::<u64>(),
        "keys": h.iter().map(|x| json!({
            "epoch": x.key.epoch,
            "serial": x.key.serial,
            "packages": x.packages,
            "at": pos(x.at),
            "since": x.since.0,
            "by": x.by.map(|d| d.0),
        })).collect::<Vec<_>>(),
    })
}

fn store(n: &dyn Namer, s: &PackageStore) -> Value {
    let ports = s
        .frogports
        .iter()
        .zip(&s.pools)
        .map(|(f, p)| {
            let at = |m: &std::collections::BTreeMap<_, Vec<Held>>| {
                m.get(&f.pos).map_or_else(|| held(&[]), |h| held(h))
            };
            json!({
                "pos": pos(f.pos),
                "conveyor": pos(f.conveyor),
                "district": f.district.map(|d| d.0),
                "district_name": f.district.and_then(|d| n.district(d.0)),
                "inventory": f.inventory,
                "star": f.star,
                "pool": pool(n, p),
                "held": at(&s.on_hold),
                "drawn": at(&s.drawn),
            })
        })
        .collect::<Vec<_>>();
    let c = &s.counts;
    json!({
        "store": s.id.0,
        "dimension": n.dimension(s.dim),
        "epoch": s.epoch,
        "frogports": ports,
        "hops": s.hops,
        "no_route": pool(n, &s.no_route),
        "left": s.left.iter().map(|(p, l)| json!({"pos": pos(*p), "pool": pool(n, l)})).collect::<Vec<_>>(),
        "residue": {
            "packages": s.residue_packages,
            "goods": goods(n, &s.residue),
            "census_at": s.census_at.map(|t| t.0),
        },
        "owed": s.owed.iter().map(|(p, g)| json!({"pos": pos(*p), "goods": goods(n, g)})).collect::<Vec<_>>(),
        "counts": {
            "packages_out": c.packages_out,
            "packages_in": c.packages_in,
            "unrouted": c.unrouted,
            "in_beyond": c.in_beyond,
            "censuses": c.censuses,
            "declarations": c.declarations,
            "stale_records": c.stale_records,
            "in_census": c.in_census,
            "written": c.written,
            "held": c.held,
            "held_landed": c.held_landed,
            "held_stale": c.held_stale,
            "held_again": c.held_again,
            "held_released": c.held_released,
            "held_unlisted": c.held_unlisted,
            "drawn": c.drawn,
            "drawn_removed": c.drawn_removed,
            "drawn_back": c.drawn_back,
        },
    })
}

fn goods_text(g: &Value) -> String {
    let l = g.as_array().cloned().unwrap_or_default();
    if l.is_empty() {
        return "no goods".into();
    }
    l.iter()
        .map(|x| format!("{} {}", x["amount"], x["good"].as_str().unwrap_or("?")))
        .collect::<Vec<_>>()
        .join(", ")
}

fn keys_text(h: &Value) -> String {
    h["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|k| format!("{}:{}", k["epoch"], k["serial"]))
        .collect::<Vec<_>>()
        .join(" ")
}

fn print_text(v: &Value) {
    let ss = v["stores"].as_array().cloned().unwrap_or_default();
    println!(
        "{}: industry at tick {}; {} package stores",
        v["world"].as_str().unwrap_or("?"),
        v["now"],
        ss.len()
    );
    for s in &ss {
        let ports = s["frogports"].as_array().cloned().unwrap_or_default();
        println!(
            "  store {} in {}: {} frogports, epoch {}",
            s["store"],
            s["dimension"].as_str().unwrap_or("?"),
            ports.len(),
            s["epoch"]
        );
        for (i, f) in ports.iter().enumerate() {
            let p = &f["pos"];
            let owner = match f["district"].as_u64() {
                Some(d) => format!(
                    "district {d} '{}'",
                    f["district_name"].as_str().unwrap_or("?")
                ),
                None => "no district".to_string(),
            };
            println!(
                "    frogport {i} at ({} {} {}): {owner}{}",
                p[0],
                p[1],
                p[2],
                if f["star"].as_bool() == Some(true) {
                    ", star"
                } else {
                    ""
                }
            );
            println!(
                "      pool: {} packages ({})",
                f["pool"]["packages"],
                goods_text(&f["pool"]["goods"])
            );
            for (what, key) in [("held", "held"), ("drawn", "drawn")] {
                let h = &f[key];
                if h["count"].as_u64().unwrap_or(0) > 0
                    || !h["keys"].as_array().is_none_or(Vec::is_empty)
                {
                    println!(
                        "      {what}: {} packages, keys {}",
                        h["count"],
                        keys_text(h)
                    );
                }
            }
        }
        println!(
            "    no-route pool: {} packages ({})",
            s["no_route"]["packages"],
            goods_text(&s["no_route"]["goods"])
        );
        let r = &s["residue"];
        println!(
            "    residue: {} packages ({}){}",
            r["packages"],
            goods_text(&r["goods"]),
            if r["census_at"].is_null() {
                ", no census yet".to_string()
            } else {
                format!(", census at {}", r["census_at"])
            }
        );
        let c = &s["counts"];
        println!(
            "    counts: {} out, {} in, {} unrouted, {} in beyond the pool, {} censuses, {} declarations, {} held, {} landed, {} drawn, {} removed, {} drawn back",
            c["packages_out"],
            c["packages_in"],
            c["unrouted"],
            c["in_beyond"],
            c["censuses"],
            c["declarations"],
            c["held"],
            c["held_landed"],
            c["drawn"],
            c["drawn_removed"],
            c["drawn_back"],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_industry::ids::{BlockPos, DistrictId, StoreId};
    use rtsim_industry::obs::HeldKey;
    use rtsim_industry::stores::Frogport;
    use rtsim_kernel::clock::Tick;

    struct Plain;

    impl Namer for Plain {
        fn good(&self, g: Good) -> String {
            format!("good{}", g.0)
        }

        fn district(&self, id: u64) -> Option<String> {
            (id == 7).then(|| "mill".to_string())
        }

        fn dimension(&self, dim: u32) -> String {
            format!("dim{dim}")
        }
    }

    fn port(d: Option<u64>, x: i32) -> Frogport {
        Frogport {
            district: d.map(DistrictId),
            inventory: d.map(|_| 0),
            star: d.is_none(),
            pos: BlockPos::new(x, 64, 0),
            conveyor: BlockPos::new(x, 65, 0),
        }
    }

    fn held(serial: u64, at: i32, by: Option<u64>) -> Held {
        Held {
            key: HeldKey { epoch: 5, serial },
            packages: 1,
            goods: GoodVec::from_pairs([(Good(2), 8)]),
            at: BlockPos::new(at, 65, 0),
            since: Tick(900),
            by: by.map(DistrictId),
        }
    }

    /// A store with pools, a no-route pool, a residue, held and drawn packages and counts.
    fn sample() -> PackageStore {
        let mut s = PackageStore::new(StoreId(3), 0);
        s.frogports = vec![port(Some(7), 10), port(None, 40)];
        s.hops = vec![30];
        s.pools = vec![Pool::default(), Pool::default()];
        s.pools[0].add(2, &GoodVec::from_pairs([(Good(1), 16)]));
        s.no_route.add(1, &GoodVec::from_pairs([(Good(4), 3)]));
        s.residue = GoodVec::from_pairs([(Good(1), -4)]);
        s.residue_packages = -1;
        s.census_at = Some(Tick(800));
        s.epoch = 5;
        s.on_hold.insert(
            BlockPos::new(10, 64, 0),
            vec![held(11, 12, None), held(12, 12, None)],
        );
        s.drawn
            .insert(BlockPos::new(40, 64, 0), vec![held(13, 38, Some(7))]);
        s.counts.packages_out = 6;
        s.counts.drawn = 1;
        s
    }

    #[test]
    fn a_store_lists_its_frogports_pools_residue_held_and_drawn_keys() {
        let v = store(&Plain, &sample());
        assert_eq!(v["store"], 3);
        assert_eq!(v["dimension"], "dim0");
        let ports = v["frogports"].as_array().unwrap();
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0]["pos"], json!([10, 64, 0]));
        assert_eq!(ports[0]["district"], 7);
        assert_eq!(ports[0]["district_name"], "mill");
        assert_eq!(ports[1]["district"], Value::Null);
        assert_eq!(ports[1]["star"], true);
        assert_eq!(ports[0]["pool"]["packages"], 2);
        assert_eq!(
            ports[0]["pool"]["goods"],
            json!([{"good": "good1", "amount": 16}])
        );
        assert_eq!(v["no_route"]["packages"], 1);
        assert_eq!(v["no_route"]["goods"][0]["good"], "good4");
        assert_eq!(v["residue"]["packages"], -1);
        assert_eq!(v["residue"]["goods"][0]["amount"], -4);
        assert_eq!(v["residue"]["census_at"], 800);
        // the held packages are toward the first frogport, with their keys, the drawn toward the second
        assert_eq!(ports[0]["held"]["count"], 2);
        let keys = ports[0]["held"]["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0]["epoch"], 5);
        assert_eq!(keys[0]["serial"], 11);
        assert_eq!(keys[1]["serial"], 12);
        assert_eq!(ports[0]["drawn"]["count"], 0);
        assert_eq!(ports[1]["drawn"]["count"], 1);
        assert_eq!(ports[1]["drawn"]["keys"][0]["serial"], 13);
        assert_eq!(ports[1]["drawn"]["keys"][0]["by"], 7);
        assert_eq!(v["counts"]["packages_out"], 6);
        assert_eq!(v["counts"]["drawn"], 1);
    }

    #[test]
    fn a_store_page_round_trips_into_the_listing() {
        let s = sample();
        let b = rtsim_industry::stores::store_page(&s);
        let back = read_store_page(3, &b).unwrap();
        assert_eq!(store(&Plain, &back), store(&Plain, &s));
    }
}
