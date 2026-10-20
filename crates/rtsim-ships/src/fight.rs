// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fights nobody watches, and the hazard pirates put on a lane.
//!
//! **Unobserved fights** are resolved with Lanchester's square law (ships fight with aimed fire:
//! cannons, crossbows), as `rtsim-industry` resolves attacks on districts (I22): the attackers'
//! surviving share is `sqrt(A² - (d/a)·D²) / A`. **Never better than observed:** the result is
//! floored by the best outcome for the defender among real fights of the same pair of roles at
//! comparable strength (within [`crate::ShipTunables::comparable_factor`]), so an abstract fight
//! never goes better for the defender than the real ones the plugin reported.
//!
//! **A lane's hazard** (SCALE.md §3: encounters between flows are hazard rates on the edge) is the
//! share of each consignment the hunting pirates take: their strength times a rate, cut by the
//! lane's patrols (M2: a patrol is a flow that lowers the hazard) through the same law, capped.

use serde::Serialize;

use crate::tunables::ShipTunables;

/// Who fights whom, for the observed floors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Pairing {
    /// Pirates attacking a merchant.
    PirateMerchant = 0,
    /// Pirates meeting a patrol.
    PiratePatrol = 1,
}

impl Pairing {
    pub const COUNT: usize = 2;

    pub fn from_u8(v: u8) -> Option<Pairing> {
        match v {
            0 => Some(Pairing::PirateMerchant),
            1 => Some(Pairing::PiratePatrol),
            _ => None,
        }
    }
}

/// One real fight's result: the attackers' strength and their surviving share.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Observed {
    pub strength: f64,
    pub survived: f64,
}

/// An abstract fight's result.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Outcome {
    /// The attackers' surviving share, after the floor.
    pub survived: f64,
    /// What Lanchester alone gave.
    pub lanchester: f64,
    /// The defenders' surviving share (the square law's other side).
    pub defender_survived: f64,
    /// The real outcome that floored it, if any.
    pub floor: Option<f64>,
}

/// The attackers' surviving share by the square law, `a·A²` against `d·D²`.
pub fn square_law(a: f64, d: f64, tun: &ShipTunables) -> f64 {
    if a <= 0.0 {
        return 0.0;
    }
    let k = tun.defender_effectiveness / tun.attacker_effectiveness.max(1e-9);
    let left = a * a - k * d * d;
    if left <= 0.0 { 0.0 } else { left.sqrt() / a }
}

/// The defenders' surviving share by the same law.
fn defender_share(a: f64, d: f64, tun: &ShipTunables) -> f64 {
    if d <= 0.0 {
        return 0.0;
    }
    let k = tun.attacker_effectiveness / tun.defender_effectiveness.max(1e-9);
    let left = d * d - k * a * a;
    if left <= 0.0 { 0.0 } else { left.sqrt() / d }
}

/// Resolves an abstract fight of strength `a` against `d`, floored by `observed` real fights of
/// the same pairing.
pub fn resolve(a: f64, d: f64, observed: &[Observed], tun: &ShipTunables) -> Outcome {
    let lanchester = square_law(a, d, tun);
    let k = tun.comparable_factor.max(1.0);
    let floor = observed
        .iter()
        .filter(|o| o.strength >= a / k && o.strength <= a * k)
        .map(|o| o.survived)
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))));
    let mut survived = lanchester;
    if let Some(f) = floor {
        survived = survived.max(f);
    }
    let survived = survived.clamp(0.0, 1.0);
    // The defenders lose what the attackers' survival implies: when the floor lifts the
    // attackers, the defenders fare no better than the law says at that result.
    let defender_survived = if survived > lanchester {
        defender_share(a, d, tun).min(1.0 - survived)
    } else {
        defender_share(a, d, tun)
    };
    Outcome {
        survived,
        lanchester,
        defender_survived: defender_survived.clamp(0.0, 1.0),
        floor,
    }
}

/// The hazard on a lane, thousandths of each consignment: pirate strength `a` cut by patrol
/// strength `n` through the square law, times the rate, capped.
pub fn hazard_milli(a: f64, n: f64, tun: &ShipTunables) -> i64 {
    if a <= 0.0 {
        return 0;
    }
    let through = a * square_law(a, n, tun);
    let raw = (through * tun.share_per_strength_milli as f64).round() as i64;
    raw.clamp(0, tun.max_share_milli)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_square_law_and_its_floor() {
        let t = ShipTunables::default();
        // 5 against 3: sqrt(25 - 9) / 5 = 0.8.
        assert!((square_law(5.0, 3.0, &t) - 0.8).abs() < 1e-12);
        assert_eq!(square_law(3.0, 5.0, &t), 0.0);
        let r = resolve(3.0, 5.0, &[], &t);
        assert_eq!(r.survived, 0.0);
        assert!((r.defender_survived - 0.8).abs() < 1e-12);
        // A real fight at strength 4 where the pirates kept half: the abstract one at 3 cannot
        // go better for the defender than that.
        let obs = [Observed {
            strength: 4.0,
            survived: 0.5,
        }];
        let r = resolve(3.0, 5.0, &obs, &t);
        assert_eq!(r.survived, 0.5);
        assert_eq!(r.floor, Some(0.5));
        assert!(r.defender_survived <= 0.5);
        // Not comparable (strength 20 is over twice 3): no floor.
        let far = [Observed {
            strength: 20.0,
            survived: 0.9,
        }];
        assert_eq!(resolve(3.0, 5.0, &far, &t).survived, 0.0);
    }

    #[test]
    fn patrols_lower_the_hazard() {
        let t = ShipTunables::default();
        let open = hazard_milli(6.0, 0.0, &t);
        let patrolled = hazard_milli(6.0, 4.0, &t);
        let swept = hazard_milli(6.0, 9.0, &t);
        assert_eq!(open, 120);
        assert!(patrolled < open && patrolled > 0, "{patrolled}");
        assert_eq!(swept, 0);
        assert_eq!(hazard_milli(1000.0, 0.0, &t), t.max_share_milli);
    }
}
