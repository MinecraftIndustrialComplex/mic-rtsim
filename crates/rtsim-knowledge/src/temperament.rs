// SPDX-License-Identifier: AGPL-3.0-or-later
//! Temperament from the seed (C25, PLAN.md §5.7): how bold, sociable and aggressive a notable is.
//! It costs no bytes: it is recomputed from the seed whenever a choice needs it, so two traders
//! of one kind choose differently with no stored state and no new content.
//!
//! The traits are mcrtsim's `SimNpc` traits (research/mcrtsim-mining-2026-10-19.md). Which traits
//! a kind uses, and how far each bends which utility, is data per kind; [`Temperament::bend`]
//! takes that bend as an argument.

use rtsim_kernel::rng::{StreamKey, SystemId};

pub const TEMPERAMENT_SYSTEM: SystemId = SystemId::named("knowledge.temperament");

/// One trait of a temperament.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Trait {
    Bold,
    Sociable,
    Aggressive,
}

/// Each trait is 0..=255, with 128 the middle of the kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Temperament {
    pub bold: u8,
    pub sociable: u8,
    pub aggressive: u8,
}

/// How far a trait bends a utility by default, in percent at the extremes. PLACEHOLDER (P18):
/// the real bend is data per kind.
pub const DEFAULT_BEND_PERCENT: u8 = 50;

impl Temperament {
    /// The temperament of the notable with this seed. Philox keyed by the seed alone, so it is
    /// the same in every world and every session for the same seed.
    pub fn from_seed(seed: u64) -> Temperament {
        let block = StreamKey::new(seed, TEMPERAMENT_SYSTEM, 0).block(0, 0);
        Temperament {
            bold: (block[0] >> 56) as u8,
            sociable: (block[1] >> 56) as u8,
            aggressive: (block[2] >> 56) as u8,
        }
    }

    pub fn get(&self, t: Trait) -> u8 {
        match t {
            Trait::Bold => self.bold,
            Trait::Sociable => self.sociable,
            Trait::Aggressive => self.aggressive,
        }
    }

    /// `base` scaled by the trait: by up to `percent` up at 255 and down at 0, unchanged at 128.
    /// Integer arithmetic, so a choice weighted with it is the same on every machine.
    pub fn bend(&self, t: Trait, base: i32, percent: u8) -> i32 {
        let dev = i64::from(self.get(t)) - 128; // -128..=127
        let scale = 100 * 128 + dev * i64::from(percent); // in 1/(100*128)
        (i64::from(base) * scale / (100 * 128)) as i32
    }

    /// [`Temperament::bend`] with [`DEFAULT_BEND_PERCENT`].
    pub fn weight(&self, t: Trait, base: i32) -> i32 {
        self.bend(t, base, DEFAULT_BEND_PERCENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_temperament_and_seeds_spread() {
        assert_eq!(Temperament::from_seed(7), Temperament::from_seed(7));
        let n = 4096u64;
        let mut sum = [0u64; 3];
        let mut lo = [0u64; 3];
        for s in 0..n {
            let t = Temperament::from_seed(s);
            for (i, v) in [t.bold, t.sociable, t.aggressive].into_iter().enumerate() {
                sum[i] += u64::from(v);
                lo[i] += u64::from(v < 64);
            }
        }
        for i in 0..3 {
            let mean = sum[i] as f64 / n as f64;
            assert!((117.0..138.0).contains(&mean), "trait {i} mean {mean}");
            // A quarter of seeds should fall below 64.
            let frac = lo[i] as f64 / n as f64;
            assert!((0.2..0.3).contains(&frac), "trait {i} low fraction {frac}");
        }
    }

    #[test]
    fn bend_is_centred_and_bounded() {
        let mid = Temperament {
            bold: 128,
            sociable: 0,
            aggressive: 255,
        };
        assert_eq!(mid.bend(Trait::Bold, 1000, 50), 1000);
        assert_eq!(mid.bend(Trait::Sociable, 1000, 50), 500);
        assert!((1490..=1500).contains(&mid.bend(Trait::Aggressive, 1000, 50)));
    }
}
