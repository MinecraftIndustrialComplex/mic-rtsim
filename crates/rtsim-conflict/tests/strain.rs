// SPDX-License-Identifier: AGPL-3.0-or-later
//! S7, conflict pressure (EQUILIBRIUM.md §2.4.5, docs/overnight-choices-equilibrium.md EQC200 on):
//! a pool camp's recruiting slows by `2^-P` while a player's kills stand, a camp that is thin and
//! under heavy pressure breaks, and a broken camp is refounded after a wait. The body camps (the
//! Frostmaw's den, the Umvuthana's grove) are S9's and keep what they book today.

use rtsim_conflict::Cultures;
use rtsim_conflict::culture::{
    BANDIT_JSON, Culture, CultureDef, FROSTMAW_JSON, GOBLIN_JSON, RAIDER_JSON, UMVUTHANA_JSON,
};

fn def(json: &str) -> CultureDef {
    serde_json::from_str(json).unwrap()
}

/// `json` with `edit` applied, resolved: the error text when the culture is refused.
fn refused(json: &str, edit: impl FnOnce(&mut CultureDef)) -> Option<String> {
    let mut d = def(json);
    edit(&mut d);
    // Resolved from the definition itself: JSON has no NaN to carry.
    Culture::resolve(d).err()
}

// ---------------------------------------------------------------- culture data (EQC202)

/// EQC202: the three pool cultures carry the block with the decided numbers, the two body
/// cultures do not, and each way the block can be wrong is its own error.
#[test]
fn pool_renewal_parses_and_validates() {
    let c = Cultures::builtin();
    for n in ["goblin", "bandit", "raider"] {
        let cu = c.get(c.index_of(n).unwrap());
        let r = cu
            .pool_renewal
            .unwrap_or_else(|| panic!("{n} carries a pool_renewal"));
        // FB-1 (a): a half-life of 192 days, the break at P 2.
        assert_eq!(r.def.half_life_days, 192.0, "{n}");
        assert_eq!(r.def.break_pressure, 2.0, "{n}");
        assert_eq!(r.def.recruit_cap_days, 96.0, "{n}");
        assert_eq!(r.def.break_share, 0.1, "{n}");
        assert_eq!(
            (
                r.def.refound_base_days,
                r.def.refound_cap_days,
                r.def.refound_share
            ),
            (48.0, 384.0, 0.25),
            "{n}"
        );
        // The recruiting rule's base is the culture's own wait for a warrior: at no pressure the
        // delay is 1 / recruits_per_day, and it stops growing at the cap.
        assert!(
            (r.recruit.base_days as f64 - 1.0 / cu.def.recruits_per_day).abs() < 1e-6,
            "{n}: base {}",
            r.recruit.base_days
        );
        assert_eq!(r.recruit.cap_days, 96.0, "{n}");
        assert_eq!(r.recruit.half_life_days, 192.0, "{n}");
        assert_eq!(r.refound.base_days, 48.0, "{n}");
        assert_eq!(r.refound.cap_days, 384.0, "{n}");
        // A pool camp has no renewal of the members' and no bodies.
        assert!(cu.renewal.is_none() && !cu.def.camp_bodies, "{n}");
    }
    for n in ["frostmaw", "umvuthana"] {
        assert!(c.get(c.index_of(n).unwrap()).pool_renewal.is_none(), "{n}");
    }

    // Each wrong block is its own error.
    let e = refused(GOBLIN_JSON, |d| d.camp_bodies = true).expect("a body camp is refused");
    assert!(
        e.contains("pool_renewal is for a camp with no bodies"),
        "{e}"
    );
    let e = refused(GOBLIN_JSON, |d| d.recruits_per_day = 0.0).expect("no rate is refused");
    assert!(e.contains("recruits_per_day"), "{e}");
    let e = refused(FROSTMAW_JSON, |d| {
        d.pool_renewal = def(GOBLIN_JSON).pool_renewal
    })
    .expect("a renewal and a pool_renewal are refused");
    assert!(e.contains("pool_renewal"), "{e}");
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let e = refused(BANDIT_JSON, |d| {
            d.pool_renewal.as_mut().unwrap().half_life_days = bad
        })
        .expect("not finite");
        assert!(e.contains("finite"), "{e}");
        let e = refused(BANDIT_JSON, |d| {
            d.pool_renewal.as_mut().unwrap().refound_share = bad
        })
        .expect("not finite");
        assert!(e.contains("finite"), "{e}");
    }
    for bad in [0.0, 1.0, -0.1, 1.5] {
        let e = refused(RAIDER_JSON, |d| {
            d.pool_renewal.as_mut().unwrap().break_share = bad
        })
        .expect("break_share outside (0, 1)");
        assert!(e.contains("break_share"), "{e}");
    }
    for bad in [0.0, -0.5, 1.01] {
        let e = refused(RAIDER_JSON, |d| {
            d.pool_renewal.as_mut().unwrap().refound_share = bad
        })
        .expect("refound_share outside (0, 1]");
        assert!(e.contains("refound_share"), "{e}");
    }
    // One is a whole band: allowed.
    assert_eq!(
        refused(RAIDER_JSON, |d| d
            .pool_renewal
            .as_mut()
            .unwrap()
            .refound_share = 1.0),
        None
    );
    let e = refused(GOBLIN_JSON, |d| {
        d.pool_renewal.as_mut().unwrap().break_pressure = 0.0
    })
    .expect("break_pressure above 0");
    assert!(e.contains("break_pressure"), "{e}");
    let e = refused(GOBLIN_JSON, |d| {
        d.pool_renewal.as_mut().unwrap().half_life_days = 0.0
    })
    .expect("half-life above 0");
    assert!(e.contains("half_life_days"), "{e}");
    // A refound cap under its base, and a recruit cap under the culture's own wait (1 / 2 days).
    let e = refused(GOBLIN_JSON, |d| {
        d.pool_renewal.as_mut().unwrap().refound_cap_days = 47.0
    })
    .expect("cap under base");
    assert!(e.contains("cap_days"), "{e}");
    assert_eq!(
        refused(GOBLIN_JSON, |d| d
            .pool_renewal
            .as_mut()
            .unwrap()
            .refound_cap_days = 48.0),
        None,
        "a cap equal to its base is allowed"
    );
    let e = refused(GOBLIN_JSON, |d| {
        d.pool_renewal.as_mut().unwrap().recruit_cap_days = 0.4
    })
    .expect("recruit cap under the culture's wait");
    assert!(e.contains("cap_days"), "{e}");
}

/// The body cultures' files carry no `pool_renewal` key at all (S9 owns them), and the document
/// round-trips through the definition type without gaining one.
#[test]
fn the_body_cultures_files_are_untouched() {
    for json in [FROSTMAW_JSON, UMVUTHANA_JSON] {
        assert!(!json.contains("pool_renewal"));
        assert!(def(json).pool_renewal.is_none());
    }
}
