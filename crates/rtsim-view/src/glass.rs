// SPDX-License-Identifier: AGPL-3.0-or-later
//! The hook for G2's glass box (docs/viewer/industry-map.md §6): a compiled district's graph drawn
//! in the industry map's symbols. G2 reads its graph rows (row kinds 16 and 17) into [`Node`]s and
//! [`Edge`]s and calls [`draw`]; nothing on the page changes. Nodes are machine points (the
//! `fact.machines` glyphs and states), edges are `flow` paths with an arrow, a width from the rate
//! and a label with the rate and its unit.

use mic_viewer::api::{LayerDesc, LegendEntry, Mark};
use mic_viewer::scene::SceneWriter;

/// The layer G2's graph draws into.
pub const LAYER: &str = "industry.glass";

/// What a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Machine,
    Store,
    Port,
}

/// A graph node: a machine, store or port of the district.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub x: f64,
    pub z: f64,
    pub kind: NodeKind,
    /// What it is or does, in words ("Mechanical press: iron sheet").
    pub label: String,
    /// Starved or blocked (`warn`), stopped (`bad`), idle (`off`), or running (None).
    pub st: Option<&'static str>,
}

/// A graph edge: goods moving from one node to another at a rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub id: String,
    /// The way the goods go, from the first point to the last (at least two).
    pub p: Vec<[f64; 2]>,
    /// Units a minute.
    pub per_minute: f64,
    /// The good's name ("Iron sheet").
    pub good: String,
    /// The unit: "items", or "mB" for a fluid.
    pub unit: &'static str,
}

/// The layer's description, for G2 to add to the layer list.
pub fn layer_desc() -> LayerDesc {
    LayerDesc::new(
        LAYER,
        "Factories",
        "Flows inside a district",
        "path",
        "#2a8a82",
    )
    .describe("A compiled district's machines and what moves between them, a minute at a time.")
    .kinds(&["point", "path"])
    .line("flow")
    .labels_below(2.0)
    .priority(60)
    .legend_entries(vec![
        LegendEntry::new("Goods moving (wider: more a minute)", "#2a8a82").line("flow"),
        LegendEntry::new("Machine", "ink").glyph("circle"),
        LegendEntry::new("Store", "ink").glyph("square"),
        LegendEntry::new("Package port", "#b7791f").glyph("port"),
        LegendEntry::new("Starved or blocked", "ink")
            .glyph("circle")
            .st("warn"),
    ])
}

/// "1,240 items a minute", "0.5 items a minute", "36,000 mB a minute".
pub fn rate_text(per_minute: f64, unit: &str) -> String {
    let n = if per_minute >= 100.0 || per_minute == per_minute.round() {
        group(per_minute.round() as i64)
    } else if per_minute >= 10.0 {
        format!("{per_minute:.1}")
    } else {
        format!("{per_minute:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    };
    format!("{n} {unit} a minute")
}

fn group(n: i64) -> String {
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// A path's width in pixels for a rate: 2 at one a minute, growing with the rate's logarithm.
pub fn width(per_minute: f64) -> f64 {
    (2.0 + 1.5 * (1.0 + per_minute.max(0.0)).log10()).min(9.0)
}

/// Draws a district's graph into `w` under [`LAYER`].
pub fn draw(nodes: &[Node], edges: &[Edge], w: &mut SceneWriter) {
    w.layer(LAYER);
    for e in edges.iter().filter(|e| e.p.len() >= 2) {
        w.mark(
            LAYER,
            Mark::Path {
                id: Some(e.id.clone()),
                p: e.p.clone(),
                ci: Some(0),
                c: None,
                w: Some(width(e.per_minute)),
                st: Some("flow".into()),
                l: Some(format!("{}: {}", e.good, rate_text(e.per_minute, e.unit))),
                arrow: true,
                n: None,
            },
        );
    }
    for n in nodes {
        let (ci, g) = match n.kind {
            NodeKind::Machine => (1, "circle"),
            NodeKind::Store => (2, "square"),
            NodeKind::Port => (3, "port"),
        };
        w.mark(
            LAYER,
            Mark::Point {
                id: n.id.clone(),
                x: n.x,
                z: n.z,
                ci: Some(ci),
                c: None,
                g: Some(g.into()),
                s: None,
                l: Some(n.label.clone()),
                ring: false,
                a: None,
                st: n.st.map(str::to_owned),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_read_with_their_units() {
        assert_eq!(rate_text(1240.0, "items"), "1,240 items a minute");
        assert_eq!(rate_text(0.5, "items"), "0.5 items a minute");
        assert_eq!(rate_text(12.25, "items"), "12.2 items a minute");
        assert_eq!(rate_text(36000.0, "mB"), "36,000 mB a minute");
        assert!(width(1.0) < width(100.0) && width(1e9) <= 9.0);
    }

    #[test]
    fn a_graph_draws_as_flows_and_machines() {
        let nodes = [
            Node {
                id: "a".into(),
                x: 0.0,
                z: 0.0,
                kind: NodeKind::Machine,
                label: "Press".into(),
                st: None,
            },
            Node {
                id: "b".into(),
                x: 8.0,
                z: 0.0,
                kind: NodeKind::Store,
                label: "Vault".into(),
                st: Some("warn"),
            },
        ];
        let edges = [Edge {
            id: "a-b".into(),
            p: vec![[0.0, 0.0], [8.0, 0.0]],
            per_minute: 60.0,
            good: "Iron sheet".into(),
            unit: "items",
        }];
        let mut w = SceneWriter::new();
        draw(&nodes, &edges, &mut w);
        let s = w.finish(0, 0.0);
        let rows = &s.layers[LAYER];
        assert_eq!(rows.len(), 3);
        let text = serde_json::to_string(rows.marks()).unwrap();
        assert!(text.contains("Iron sheet: 60 items a minute"), "{text}");
        assert!(text.contains(r#""st":"warn""#));
    }
}
