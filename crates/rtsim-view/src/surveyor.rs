// SPDX-License-Identifier: AGPL-3.0-or-later
//! Surveyor's terrain summaries as each frame's base map at one block a pixel (VIEWER.md §9, V5),
//! drawn over HABITAT: Surveyor leaves the columns it never recorded transparent, so HABITAT's
//! coarse cells show where nobody has been. A world without Surveyor keeps HABITAT alone.
//!
//! Reading and drawing are mic-viewer-surveyor's; this module finds each frame's folder, names the
//! layers and routes their tiles through the sidecar's one worker (VWD-15).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use mic_viewer::api::TileLayerDesc;
use mic_viewer::tiles::{Rendered, TileError, TileSource};
use mic_viewer_surveyor::{RENDER_VERSION, Surveyor, default_max_y, dir_for};

/// Every frame's Surveyor map, by tile layer name.
pub struct SurveyorTiles {
    maps: BTreeMap<String, Surveyor>,
    worker: Arc<Mutex<()>>,
}

impl SurveyorTiles {
    /// The Surveyor maps of the frames `(frame id, layer slug)` under `world`, sharing
    /// `cache_bytes` of pictures; `None` when no frame has one. A frame id must be a dimension
    /// name (`minecraft:the_nether`): a frame known only by number (`dim3`) has no folder to look in.
    pub fn open(
        world: &Path,
        frames: &[(String, String)],
        cache_bytes: usize,
        worker: Arc<Mutex<()>>,
    ) -> Option<SurveyorTiles> {
        let found: Vec<(String, Surveyor)> = frames
            .iter()
            .filter(|(id, _)| id.contains(':'))
            .filter_map(|(id, slug)| {
                let s = Surveyor::open(dir_for(world, id)).ok().flatten()?;
                Some((layer_name(slug), s.with_max_y(default_max_y(id))))
            })
            .collect();
        if found.is_empty() {
            return None;
        }
        let each = cache_bytes / found.len();
        Some(SurveyorTiles {
            maps: found
                .into_iter()
                .map(|(name, s)| (name, s.with_cache_bytes(each)))
                .collect(),
            worker,
        })
    }

    /// A frame's layer descriptor, if it has a map.
    pub fn desc(&self, slug: &str) -> Option<TileLayerDesc> {
        let name = layer_name(slug);
        let s = self.maps.get(&name)?;
        Some(s.layer(
            &name,
            "Terrain (Surveyor)",
            &format!("tiles/{name}/{{z}}/{{x}}/{{y}}.png"),
        ))
    }

    pub fn layer_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.maps.keys().map(String::as_str)
    }

    /// Region files parsed so far, over every frame.
    pub fn parses(&self) -> u64 {
        self.maps.values().map(Surveyor::parses).sum()
    }

    /// Picture bytes cached, over every frame.
    pub fn cached_bytes(&self) -> usize {
        self.maps.values().map(Surveyor::cached_bytes).sum()
    }
}

/// The tile layer of a frame slug.
pub fn layer_name(slug: &str) -> String {
    format!("surveyor-{slug}")
}

impl TileSource for SurveyorTiles {
    fn render(&self, layer: &str, z: i32, x: i64, y: i64) -> Result<Rendered, TileError> {
        let Some(s) = self.maps.get(layer) else {
            return Err(TileError::Missing(format!("no base map {layer:?}")));
        };
        let _one = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        s.tile(z, x, y)
    }

    fn opaque(&self, _layer: &str) -> bool {
        false
    }

    fn version(&self) -> u32 {
        RENDER_VERSION
    }
}
