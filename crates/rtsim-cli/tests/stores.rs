//! `rtsim-cli stores` on a saved GameTest world (SECTIONS.md §8.2, slice S7): the create fixture
//! (`crates/rtsim-view/tests/data/create`) holds six package stores from the S4 GameTests; they are
//! read through the read-only open, as text and as JSON, with nothing in the save's folder changed.

use std::path::PathBuf;
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../rtsim-view/tests/data")
        .join(name)
}

fn stores(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rtsim-cli"))
        .arg("stores")
        .args(args)
        .output()
        .unwrap()
}

fn dir_bytes(dir: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                todo.push(p);
            } else if p.is_file() {
                out.push((p.clone(), std::fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn stores_lists_the_create_saves_package_stores_and_writes_nothing() {
    let world = fixture("create");
    let w = world.to_str().unwrap();
    let before = dir_bytes(&world);

    let out = stores(&[w, "--json"]);
    assert!(out.status.success(), "{out:?}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["now"].as_u64().unwrap() > 0, "{v}");
    let list = v["stores"].as_array().unwrap();
    assert_eq!(list.len(), 6, "{v}");
    for s in list {
        assert!(s["store"].as_u64().unwrap() > 0, "{s}");
        assert_eq!(s["dimension"], "minecraft:overworld", "{s}");
        let ports = s["frogports"].as_array().unwrap();
        assert!(ports.len() >= 2, "a store has two frogports at least: {s}");
        for f in ports {
            assert!(f["pos"].as_array().is_some_and(|p| p.len() == 3), "{f}");
            assert!(f["pool"]["packages"].is_u64(), "{f}");
            assert!(f["held"]["keys"].is_array(), "{f}");
            assert!(f["drawn"]["keys"].is_array(), "{f}");
        }
        assert!(s["no_route"]["packages"].is_u64(), "{s}");
        assert!(s["counts"]["declarations"].as_u64().unwrap() >= 1, "{s}");
    }
    // store 2 booked packages and took a census: its residue is named goods, not ids
    let two = list.iter().find(|s| s["store"] == 2).unwrap();
    assert_eq!(two["counts"]["packages_out"], 5, "{two}");
    assert_eq!(two["residue"]["packages"], -2, "{two}");
    assert_eq!(
        two["residue"]["goods"][0]["good"], "minecraft:iron_ingot",
        "{two}"
    );

    // the text form: one block per store
    let t = stores(&[w]);
    assert!(t.status.success(), "{t:?}");
    let text = String::from_utf8(t.stdout).unwrap();
    assert!(text.contains("6 package stores"), "{text}");
    assert!(
        text.contains("store 2 in minecraft:overworld: 2 frogports"),
        "{text}"
    );
    assert!(
        text.contains("residue: -2 packages (-3 minecraft:iron_ingot)"),
        "{text}"
    );
    assert!(text.contains("no-route pool"), "{text}");

    assert_eq!(dir_bytes(&world), before, "stores wrote into the save");
}

#[test]
fn stores_says_so_for_a_world_with_none_and_refuses_bad_arguments() {
    let core = fixture("core");
    let out = stores(&[core.to_str().unwrap()]);
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("0 package stores")
    );

    let w = fixture("create");
    let w = w.to_str().unwrap();
    assert_eq!(stores(&[]).status.code(), Some(64));
    assert_eq!(stores(&[w, "--graph"]).status.code(), Some(64));
    assert_eq!(stores(&[w, "extra"]).status.code(), Some(64));
    let none = stores(&["/nonexistent-world"]);
    assert_eq!(none.status.code(), Some(1));
}
