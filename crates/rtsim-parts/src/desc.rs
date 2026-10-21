// SPDX-License-Identifier: AGPL-3.0-or-later
//! Layer descriptors (VIEWER.md §4.3), mirrored from mic-viewer's `api::LayerDesc` with the same
//! JSON, so the view store carries them as JSON and the sidecar reads them straight into the
//! viewer's own types. One addition, `fields`: the card fields a layer's rows carry, by index, for
//! a layer whose cards the sidecar cannot build (a plugin's, VWD-17).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub color: String,
    pub glyph: String,
    pub size: f64,
    pub width: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LegendEntry {
    pub label: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
}

/// Everything the page needs to draw a layer it has never heard of.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerDesc {
    pub id: String,
    pub group: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub kinds: Vec<String>,
    pub priority: i32,
    pub default_on: bool,
    pub min_bpp: f64,
    pub max_bpp: f64,
    pub style: Style,
    #[serde(default)]
    pub legend: Vec<LegendEntry>,
    pub has_cards: bool,
    pub has_past: bool,
    #[serde(default)]
    pub animated: bool,
    /// The labels of the card fields its rows carry (a row's field `i` is labelled `fields[i]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
}

impl LayerDesc {
    /// A layer with defaults: every zoom, on, no legend, cards (as mic-viewer's).
    pub fn new(id: &str, group: &str, label: &str, kind: &str, color: &str) -> LayerDesc {
        LayerDesc {
            id: id.into(),
            group: group.into(),
            label: label.into(),
            description: String::new(),
            kinds: vec![kind.into()],
            priority: 0,
            default_on: true,
            min_bpp: 0.0,
            max_bpp: 1e9,
            style: Style {
                color: color.into(),
                glyph: "circle".into(),
                size: 6.0,
                width: 2.0,
            },
            legend: Vec::new(),
            has_cards: true,
            has_past: false,
            animated: kind == "leg",
            fields: Vec::new(),
        }
    }

    pub fn describe(mut self, d: &str) -> Self {
        self.description = d.into();
        self
    }

    pub fn priority(mut self, p: i32) -> Self {
        self.priority = p;
        self
    }

    pub fn zoom(mut self, min_bpp: f64, max_bpp: f64) -> Self {
        self.min_bpp = min_bpp;
        self.max_bpp = max_bpp;
        self
    }

    pub fn off(mut self) -> Self {
        self.default_on = false;
        self
    }

    pub fn glyph(mut self, g: &str, size: f64) -> Self {
        self.style.glyph = g.into();
        self.style.size = size;
        self
    }

    pub fn width(mut self, w: f64) -> Self {
        self.style.width = w;
        self
    }

    pub fn kinds(mut self, kinds: &[&str]) -> Self {
        self.kinds = kinds.iter().map(|k| (*k).to_owned()).collect();
        self
    }

    pub fn legend(mut self, entries: &[(&str, &str)]) -> Self {
        self.legend = entries
            .iter()
            .map(|(l, c)| LegendEntry {
                label: (*l).into(),
                color: (*c).into(),
                glyph: None,
            })
            .collect();
        self
    }

    pub fn no_cards(mut self) -> Self {
        self.has_cards = false;
        self
    }

    pub fn past(mut self) -> Self {
        self.has_past = true;
        self
    }

    pub fn fields(mut self, labels: &[&str]) -> Self {
        self.fields = labels.iter().map(|l| (*l).to_owned()).collect();
        self
    }
}

/// The layer list: groups in panel order, then every layer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Layers {
    pub groups: Vec<Group>,
    pub layers: Vec<LayerDesc>,
}

impl Layers {
    /// The list as the view store's layers page holds it.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// The list from the view store's layers page.
    pub fn from_json(bytes: &[u8]) -> Result<Layers, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("view layers page: {e}"))
    }
}
