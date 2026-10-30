// SPDX-License-Identifier: AGPL-3.0-or-later
//! A camp's plunder mood (EQUILIBRIUM.md §2.4.5, Ben's FB-8; docs/overnight-choices-equilibrium.md
//! EQC285 to EQC289): the wait to its next plunder depends on who the raiders are and what has
//! happened to them lately.
//!
//! - **Who they are** is the chief's temperament, which the engine already reads for this clock
//!   (`Temperament::from_seed(chief_seed)`: a bold or aggressive chief goes sooner, a meek one
//!   later). A new band (a refound) has a new chief, so a new temperament; a chief who falls is
//!   succeeded by the heir, whose temperament is another.
//! - **What has happened** is two memories as the kernel keeps them (a value and the day it was
//!   last bumped, halving in `half_life_days`): *grief*, the share of the cap the camp lost in
//!   war parties (whoever killed them) and in its war chiefs, and *spoils*, the raids it brought
//!   loot home from. The mean wait is the culture's `plunder_mean_days`, bent by the temperament
//!   and then by `exp(loss_gain x grief - spoils_gain x spoils)`, clamped to the data's bounds.
//! - The draw is the exponential one it always was, from the same stream (`SYS_PLUNDER`, the
//!   faction's key, the draw's number): the mood moves the mean only.
//!
//! Only a culture with a `plunder_mood` block carries a [`Mood`] (the raider); every other
//! culture's clock is the old one, bit for bit. The mood is saved in the faction row.

use super::*;
use rtsim_kernel::renewal::Pressure;

/// A camp's recent raiding history (EQC287): both memories fade.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mood {
    /// Members lost, as a share of the cap, lately.
    pub grief: Pressure,
    /// Raids that brought loot home, lately.
    pub spoils: Pressure,
}

impl Conflict {
    /// The camp's grief and spoils at `at`, if its culture has a plunder mood.
    pub fn mood_now(&self, f: u32, at: Tick) -> Option<(f32, f32)> {
        let m = self.culture(f).plunder_mood?;
        let h = self.factions[f as usize].mood.as_deref()?;
        let day = (at.0 / TICKS_PER_DAY) as u32;
        Some((
            h.grief.now(day, m.def.half_life_days),
            h.spoils.now(day, m.def.half_life_days),
        ))
    }

    /// The mean wait to the camp's next plunder at `at`, in game days (EQC287), or `None` for a
    /// culture that does not plunder: `plunder_mean_days` bent by the chief's aggression, and for
    /// a culture with a mood by the camp's history.
    pub fn plunder_mean_days(&self, f: u32, at: Tick) -> Option<f64> {
        let c = self.culture(f);
        let base = c.def.plunder_mean_days?;
        let bend = self.temper(f).bend(Trait::Aggressive, 1000, 50).max(1);
        let mean = base * 1000.0 / f64::from(bend);
        let Some(m) = c.plunder_mood else {
            return Some(mean);
        };
        let (grief, spoils) = self.mood_now(f, at).unwrap_or((0.0, 0.0));
        let log = f64::from(m.def.loss_gain) * f64::from(grief)
            - f64::from(m.def.spoils_gain) * f64::from(spoils);
        let factor = log
            .exp()
            .clamp(f64::from(m.def.min_factor), f64::from(m.def.max_factor));
        Some(mean / m.temper_mean * factor)
    }

    /// The camp lost `share` of its cap at `at` (a test hook as well: the engine's own callers
    /// are a war party's losses and a war chief's fall). A faction with no mood books nothing.
    pub fn grieve(&mut self, f: u32, share: f32, at: Tick) {
        let Some(m) = self.culture(f).plunder_mood else {
            return;
        };
        if !(share > 0.0 && share.is_finite()) {
            return;
        }
        let day = (at.0 / TICKS_PER_DAY) as u32;
        if let Some(h) = self.factions[f as usize].mood.as_deref_mut() {
            h.grief.bump(day, share, m.def.half_life_days);
        }
    }

    /// A plunder brought loot home at `at` (a test hook as well). A faction with no mood books
    /// nothing.
    pub fn gloat(&mut self, f: u32, at: Tick) {
        let Some(m) = self.culture(f).plunder_mood else {
            return;
        };
        let day = (at.0 / TICKS_PER_DAY) as u32;
        if let Some(h) = self.factions[f as usize].mood.as_deref_mut() {
            h.spoils.bump(day, 1.0, m.def.half_life_days);
        }
    }

    /// `lost` members of a war party fell at `at`, whoever killed them.
    pub(super) fn grieve_members(&mut self, f: u32, lost: u32, at: Tick) {
        if lost == 0 || self.culture(f).plunder_mood.is_none() {
            return;
        }
        let cap = self.factions[f as usize].cap.max(1);
        self.grieve(f, lost as f32 / cap as f32, at);
    }

    /// The camp's war chief fell at `at`.
    pub(super) fn grieve_chief(&mut self, f: u32, at: Tick) {
        let Some(m) = self.culture(f).plunder_mood else {
            return;
        };
        self.grieve(f, m.def.chief_loss_share, at);
    }

    /// A new band has no history (a refound): the old band's memories go with it.
    pub(super) fn forget_raids(&mut self, f: u32) {
        if let Some(h) = self.factions[f as usize].mood.as_deref_mut() {
            *h = Mood::default();
        }
    }
}
