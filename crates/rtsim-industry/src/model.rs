// SPDX-License-Identifier: AGPL-3.0-or-later
//! District models behind one interface (DECISIONS.md C28, C29).
//!
//! A district model says which processes a district runs while it is replayed: groups of goods
//! that run together at their rates, with the targets that stop them. Everything else about the
//! replay is the model's surroundings and the same for every model, built by
//! [`crate::district::District::member_spec`]: which pools exist and hold what, where each process
//! may run when only part of the district is loaded (I7), item space, tank and battery caps, power
//! from a PowerGrid line (I13), package links, and jammed outputs.
//!
//! **The black box** ([`BlackBox`]) is the model built so far: the rates learned at the district's
//! boundary (I5), split into the groups its starvation episodes showed apart (I6, Ben's "learned
//! coupling"), one all-or-nothing group until there is evidence. Under C28 it becomes the
//! fallback for machines the glass box does not know, and C29 keeps it as the glass box's
//! checksum.
//!
//! **The glass box** (GLASSBOX.md, C28; designed in parallel and not approved) compiles the real
//! Create graph into a rate network. It is meant to slot in as another [`DistrictModel`]: its
//! compiled processes come out of [`DistrictModel::processes`] in the same form, and the cluster
//! runs them with the same breakpoints, reconciles and threats. What it will need beyond this
//! interface (explicit flows between internal buffers, kinetic limits) is the glass box design's
//! to say; nothing here assumes the black box's shape past `processes`.

use rtsim_kernel::conserve::Diffuser;

use crate::district::Model;
use crate::ids::Good;
use crate::learn::Coupling;

/// One process a model runs: its goods' rates (negative: consumed), the targets that stop it,
/// and its restart latency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelProcess {
    pub terms: Vec<(Good, Diffuser)>,
    /// (good, stop at, restart at).
    pub targets: Vec<(Good, i64, i64)>,
    pub latency: u64,
}

impl ModelProcess {
    pub fn goods(&self) -> impl Iterator<Item = Good> + '_ {
        self.terms.iter().map(|t| t.0)
    }

    pub fn produces(&self, g: Good) -> bool {
        self.terms.iter().any(|t| t.0 == g && t.1.rate_fixed() > 0)
    }
}

/// A district model (C28, C29): the processes it runs, in priority order (the first has the
/// lowest id, so it wins shared stock).
pub trait DistrictModel {
    fn processes(&self) -> Vec<ModelProcess>;
}

/// The black-box model: learned boundary rates split by learned coupling (I6).
pub struct BlackBox<'a> {
    pub model: &'a Model,
    pub coupling: &'a Coupling,
    pub min_episodes: u32,
}

impl DistrictModel for BlackBox<'_> {
    fn processes(&self) -> Vec<ModelProcess> {
        let m = self.model;
        let goods: Vec<(Good, i64)> = m.rates.iter().map(|r| (r.0, r.1.rate_fixed())).collect();
        let targets: Vec<Good> = m
            .targets
            .iter()
            .map(|t| t.good)
            .filter(|g| goods.iter().any(|x| x.0 == *g && x.1 > 0))
            .collect();
        self.coupling
            .partition(&goods, &targets, self.min_episodes)
            .into_iter()
            .map(|group| ModelProcess {
                terms: m
                    .rates
                    .iter()
                    .filter(|r| group.contains(&r.0))
                    .copied()
                    .collect(),
                targets: m
                    .targets
                    .iter()
                    .filter(|t| group.contains(&t.good))
                    .map(|t| (t.good, t.stop_at, t.restart_at))
                    .collect(),
                latency: m.learned.latency_ticks,
            })
            .collect()
    }
}
