//! `rtsim-cli glass` on a saved GameTest world (SECTIONS.md §8.2, slice S1): the create fixture
//! (`crates/rtsim-view/tests/data/create`, a fresh create GameTest save) read through the read-only
//! open, as text, as JSON and as a compile graph, with nothing in the save's folder changed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;

fn create_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../rtsim-view/tests/data/create")
}

fn glass(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rtsim-cli"))
        .arg("glass")
        .args(args)
        .output()
        .unwrap()
}

/// Every file under `dir` with its bytes and modification time.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let meta = std::fs::metadata(&d).unwrap();
        out.insert(d.clone(), (Vec::new(), meta.modified().unwrap()));
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            let meta = std::fs::symlink_metadata(&p).unwrap();
            if meta.is_dir() {
                todo.push(p);
            } else {
                let bytes = if meta.is_file() {
                    std::fs::read(&p).unwrap()
                } else {
                    Vec::new()
                };
                out.insert(p, (bytes, meta.modified().unwrap()));
            }
        }
    }
    out
}

fn json(out: &Output) -> serde_json::Value {
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn glass_reads_a_create_gametest_save_and_writes_nothing() {
    let world = create_fixture();
    let w = world.to_str().unwrap();
    let before = snapshot(&world);

    let all = json(&glass(&[w, "--json"]));
    let districts = all["districts"].as_array().unwrap();
    assert!(!districts.is_empty(), "{all}");
    assert!(all["now"].as_u64().unwrap() > 0, "{all}");
    let d = &districts[0];
    let id = d["district"].as_u64().unwrap();
    assert!(d["compile"]["nodes"].as_u64().unwrap() > 0, "{d}");
    assert!(
        d["compile"].get("graph_nodes").is_none(),
        "--json leaves the graph out: {d}"
    );

    // one district, with its compile's nodes and edges
    let one = json(&glass(&[w, &id.to_string(), "--graph"]));
    let only = one["districts"].as_array().unwrap();
    assert_eq!(only.len(), 1, "{one}");
    assert_eq!(only[0]["district"].as_u64(), Some(id));
    let nodes = only[0]["compile"]["graph_nodes"].as_array().unwrap();
    assert_eq!(
        nodes.len() as u64,
        only[0]["compile"]["nodes"].as_u64().unwrap(),
        "{one}"
    );
    assert!(
        nodes
            .iter()
            .all(|n| n["pos"]["x"].is_i64() && n["kind"].is_string()),
        "{one}"
    );
    assert!(only[0]["compile"]["graph_edges"].is_array(), "{one}");

    // the text form names the district
    let text = glass(&[w]);
    assert!(text.status.success(), "{text:?}");
    assert!(
        String::from_utf8_lossy(&text.stdout).contains(&format!("district {id} ")),
        "{text:?}"
    );

    // a district with no glass page, a name in place of an id, and two output forms
    let missing = glass(&[w, "999999999"]);
    assert_eq!(missing.status.code(), Some(1), "{missing:?}");
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("has no glass page"),
        "{missing:?}"
    );
    assert_eq!(glass(&[w, "mill"]).status.code(), Some(64));
    assert_eq!(glass(&[w, "--json", "--graph"]).status.code(), Some(64));

    assert!(before == snapshot(&world), "the save's folder changed");
}
