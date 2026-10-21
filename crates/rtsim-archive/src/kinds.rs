// SPDX-License-Identifier: AGPL-3.0-or-later
//! The kinds the engine records (VIEWER.md §10.1), each with the label and the usual severity the
//! viewer shows. A recorder names its kinds from [`KINDS`] (a test in each module checks), so the
//! archive and the page agree on what a name means. A kind's own rows may still be more or less
//! severe than its usual (a raid that takes a district is Major).
//!
//! Names are `<module>.<what>`: the part before the first dot is the page's group.

use crate::row::Severity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindInfo {
    pub name: &'static str,
    pub label: &'static str,
    pub severity: Severity,
}

/// Every kind a recorder writes. Append; a name never changes meaning (the dictionary keeps it).
pub const KINDS: &[KindInfo] = &[];

/// A kind by name.
pub fn info(name: &str) -> Option<&'static KindInfo> {
    KINDS.iter().find(|k| k.name == name)
}

/// A name's group: the part before the first dot (`"conflict.raid"` is `"conflict"`).
pub fn group(name: &str) -> &str {
    name.split_once('.').map_or(name, |(g, _)| g)
}

/// A label for a name [`KINDS`] lacks: its last part in words (`"herds.route_leg"` is
/// `"Route leg"`).
pub fn fallback_label(name: &str) -> String {
    let tail = name.rsplit('.').next().unwrap_or(name).replace('_', " ");
    let mut c = tail.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_grouped() {
        for (i, k) in KINDS.iter().enumerate() {
            assert!(k.name.contains('.'), "{} has no group", k.name);
            assert!(!k.label.is_empty());
            assert!(
                KINDS[..i].iter().all(|o| o.name != k.name),
                "{} twice",
                k.name
            );
        }
        assert_eq!(group("conflict.raid"), "conflict");
        assert_eq!(group("loose"), "loose");
        assert_eq!(fallback_label("herds.route_leg"), "Route leg");
        assert_eq!(fallback_label(""), "");
    }
}
