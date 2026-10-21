// SPDX-License-Identifier: AGPL-3.0-or-later
//! The one reader (VIEWER.md §4.3): parts, from a save's builders or the live view store, drawn
//! into one scene writer per frame. Neither source's marks reach a scene any other way, so a live
//! layer and a save's layer draw the same marks from the same state.

use std::collections::BTreeMap;

use mic_viewer::api::{Layers, Mark};
use mic_viewer::scene::SceneWriter;

/// What the reader drew: a scene writer per frame, the bounds of each frame's marks, marks per
/// layer, and why each empty layer is empty.
#[derive(Default)]
pub struct Drawn {
    pub frames: BTreeMap<u32, SceneWriter>,
    pub bounds: BTreeMap<u32, [f64; 4]>,
    pub counts: BTreeMap<String, usize>,
    pub reasons: BTreeMap<String, String>,
}

impl Drawn {
    /// One mark of `layer` in `frame`.
    pub fn mark(&mut self, layer: &str, frame: u32, m: Mark) {
        let b = m.bbox();
        if b.iter().all(|v| v.is_finite()) {
            let e = self
                .bounds
                .entry(frame)
                .or_insert([f64::MAX, f64::MAX, f64::MIN, f64::MIN]);
            *e = [
                e[0].min(b[0]),
                e[1].min(b[1]),
                e[2].max(b[2]),
                e[3].max(b[3]),
            ];
        }
        self.frames.entry(frame).or_default().mark(layer, m);
    }

    /// Every mark row of one part, drawn under `layer`. Rows of kinds this reader does not draw
    /// (a later version's graph rows) are skipped. Returns the marks drawn.
    pub fn part(&mut self, layer: &str, bytes: &[u8]) -> Result<usize, String> {
        let v = rtsim_parts::read_part(bytes)?;
        let mut n = 0;
        for r in v.rows {
            if let Some(m) = r.mark {
                self.mark(layer, r.frame, to_viewer(m));
                n += 1;
            }
        }
        Ok(n)
    }
}

/// A part's mark as the viewer's (the same fields, kind for kind).
pub fn to_viewer(m: rtsim_parts::Mark) -> Mark {
    use rtsim_parts::Mark as P;
    match m {
        P::Point {
            id,
            x,
            z,
            ci,
            c,
            g,
            s,
            l,
            ring,
        } => Mark::Point {
            id,
            x,
            z,
            ci,
            c,
            g,
            s,
            l,
            ring,
        },
        P::Leg {
            id,
            p,
            t0,
            t1,
            ci,
            c,
            g,
            l,
        } => Mark::Leg {
            id,
            p,
            t0,
            t1,
            ci,
            c,
            g,
            l,
        },
        P::Path { id, p, ci, c, w } => Mark::Path { id, p, ci, c, w },
        P::Edge { id, a, b, w, ci, c } => Mark::Edge { id, a, b, w, ci, c },
        P::Area {
            id,
            x,
            z,
            r,
            r#box,
            ci,
            c,
            l,
        } => Mark::Area {
            id,
            x,
            z,
            r,
            r#box,
            ci,
            c,
            l,
        },
        P::Cell {
            x0,
            z0,
            size,
            w,
            h,
            v,
            ramp,
            lo,
            hi,
        } => Mark::Cell {
            x0,
            z0,
            size,
            w,
            h,
            v,
            ramp,
            lo,
            hi,
        },
        P::Cluster { x, z, n, by } => Mark::Cluster { x, z, n, by },
        P::Label { x, z, text } => Mark::Label { x, z, text },
    }
}

/// A layer list as the viewer's. The descriptors share their JSON; the parts' `fields` (card
/// field labels) are not the viewer's and are dropped.
pub fn viewer_layers(l: &rtsim_parts::Layers) -> Layers {
    serde_json::to_value(l)
        .and_then(serde_json::from_value)
        .unwrap_or_default()
}

/// A layer list from its JSON (the view store's layers page).
pub fn layers_from_json(bytes: &[u8]) -> Result<rtsim_parts::Layers, String> {
    rtsim_parts::Layers::from_json(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_parts::PartWriter;

    #[test]
    fn every_kind_reads_into_the_viewers_mark() {
        let ms = [
            rtsim_parts::Mark::point(1, 2.0, 3.0)
                .with_ci(2)
                .with_label("x")
                .with_size(1.5)
                .with_ring(),
            rtsim_parts::Mark::leg(2, [0.0, 0.0], [10.0, 5.0], 100, 200).with_color("#fff"),
            rtsim_parts::Mark::Path {
                id: Some("p".into()),
                p: vec![[0.0, 0.0], [1.0, 1.0]],
                ci: None,
                c: None,
                w: Some(2.0),
            },
            rtsim_parts::Mark::edge([0.0, 0.0], [600.0, 0.0], 1.5),
            rtsim_parts::Mark::disc(Some("d".into()), -700.0, 9.0, 30.0),
            rtsim_parts::Mark::rect(None, [0.0, 0.0, 16.0, 32.0]),
            rtsim_parts::Mark::Label {
                x: 1.0,
                z: 2.0,
                text: "hi".into(),
            },
        ];
        let mut w = PartWriter::marks(0, 100, true);
        for (i, m) in ms.iter().enumerate() {
            w.push(i as u64, 0, m);
        }
        let mut d = Drawn::default();
        let mut n = 0;
        for (_, p) in w.finish() {
            n += d.part("l", &p.bytes).unwrap();
        }
        assert_eq!(n, ms.len());
        // Each mark's JSON is the viewer's JSON of the same mark.
        let want: std::collections::BTreeSet<String> = ms
            .iter()
            .map(|m| serde_json::to_string(m).unwrap())
            .collect();
        let got: std::collections::BTreeSet<String> = ms
            .iter()
            .cloned()
            .map(|m| serde_json::to_string(&to_viewer(m)).unwrap())
            .collect();
        assert_eq!(want, got);
    }

    #[test]
    fn layer_lists_cross_by_json() {
        let l = rtsim_parts::Layers {
            groups: vec![rtsim_parts::Group {
                id: "g".into(),
                label: "G".into(),
            }],
            layers: vec![
                rtsim_parts::LayerDesc::new("a.b", "g", "B", "point", "#123456")
                    .fields(&["State"])
                    .off(),
            ],
        };
        let v = viewer_layers(&l);
        assert_eq!(v.layers.len(), 1);
        assert_eq!(v.layers[0].id, "a.b");
        assert!(!v.layers[0].default_on);
        let back = layers_from_json(&serde_json::to_vec(&l).unwrap()).unwrap();
        assert_eq!(back, l);
    }
}
