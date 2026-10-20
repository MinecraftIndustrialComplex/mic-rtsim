// SPDX-License-Identifier: AGPL-3.0-or-later
//! Identities that cross the boundary with Java (docs/industry-boundary.md §2), and the sorted
//! (good, amount) vector every other module uses.

use serde::Serialize;

/// A district: a set of chunks in one dimension, the unit that is measured, simulated, reconciled
/// and attacked (INDUSTRY.md §2.1, I2). Allocated by the Java side's district index, stable for the
/// world (overnight choice F3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct DistrictId(pub u64);

/// A good: an item plus its data components, a fluid plus its components (a Destroy mixture is its
/// own good), an energy kind, or a finite deposit (INDUSTRY.md §3.1). The number is an entry of
/// Java's GOOD dictionary (docs/industry-boundary.md §2.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Good(pub u32);

/// A Create train station (`GlobalStation`), numbered by the Java side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct StationId(pub u64);

/// A Create train, numbered by the Java side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct TrainId(pub u64);

/// An Ender Transmission channel buffer (I13), numbered by the Java side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ChannelId(pub u64);

/// What kind of thing a good is. Decides how capacity is counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum GoodClass {
    /// Counted in items; takes inventory slots.
    Item,
    /// Counted in millibuckets; one fluid per tank.
    Fluid,
    /// Counted in the energy kind's own unit (FE, or a plugin's integer unit).
    Energy,
    /// A finite chunk-bound deposit read by a plugin (I11), counted in the plugin's unit.
    Deposit,
}

/// Item space is counted in units of 1/4096 of a slot, so a 64-stack item takes 64 units and an
/// unstackable one 4096. Items whose stack does not divide 4096 round their weight up, which only
/// ever overstates the space they use (overnight choice F12).
pub const SLOT_UNITS: i64 = 4096;

/// What the engine knows about a good, parsed from its dictionary name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GoodInfo {
    pub class: GoodClass,
    /// Items only: the stack size (data components included). 0 for other classes.
    pub max_stack: u32,
    /// The registry id with any component hash, for display.
    pub name: String,
}

impl GoodInfo {
    /// Parses a GOOD dictionary name, `<class>/<max stack>/<registry id>[/<component hash>]`, as
    /// docs/industry-boundary.md §2.2 defines it: `i/64/minecraft:iron_ingot`,
    /// `f/0/destroy:mixture/77ab01ff`, `e/0/fe`, `d/0/oreexcavation:vein`.
    pub fn parse(name: &str) -> Option<GoodInfo> {
        let mut parts = name.splitn(3, '/');
        let class = match parts.next()? {
            "i" => GoodClass::Item,
            "f" => GoodClass::Fluid,
            "e" => GoodClass::Energy,
            "d" => GoodClass::Deposit,
            _ => return None,
        };
        let max_stack: u32 = parts.next()?.parse().ok()?;
        let rest = parts.next()?;
        if rest.is_empty() || (class == GoodClass::Item && max_stack == 0) {
            return None;
        }
        Some(GoodInfo {
            class,
            max_stack: if class == GoodClass::Item { max_stack } else { 0 },
            name: rest.to_string(),
        })
    }

    /// The dictionary name [`GoodInfo::parse`] reads.
    pub fn dictionary_name(&self) -> String {
        let c = match self.class {
            GoodClass::Item => 'i',
            GoodClass::Fluid => 'f',
            GoodClass::Energy => 'e',
            GoodClass::Deposit => 'd',
        };
        format!("{c}/{}/{}", self.max_stack, self.name)
    }

    pub fn item(name: &str, max_stack: u32) -> GoodInfo {
        GoodInfo {
            class: GoodClass::Item,
            max_stack,
            name: name.to_string(),
        }
    }

    pub fn fluid(name: &str) -> GoodInfo {
        GoodInfo {
            class: GoodClass::Fluid,
            max_stack: 0,
            name: name.to_string(),
        }
    }

    pub fn energy(name: &str) -> GoodInfo {
        GoodInfo {
            class: GoodClass::Energy,
            max_stack: 0,
            name: name.to_string(),
        }
    }

    pub fn deposit(name: &str) -> GoodInfo {
        GoodInfo {
            class: GoodClass::Deposit,
            max_stack: 0,
            name: name.to_string(),
        }
    }

    /// Slot units one item takes ([`SLOT_UNITS`]); 0 for goods that take no item slots.
    pub fn slot_weight(&self) -> i64 {
        match self.class {
            GoodClass::Item => {
                let ms = i64::from(self.max_stack.max(1));
                (SLOT_UNITS + ms - 1) / ms
            }
            _ => 0,
        }
    }
}

/// A chunk in a dimension (dimension ids from the bridge's dictionary, kind 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ChunkPos {
    pub dim: u32,
    pub x: i32,
    pub z: i32,
}

/// A block position. The dimension travels separately.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn chunk(self, dim: u32) -> ChunkPos {
        ChunkPos {
            dim,
            x: self.x >> 4,
            z: self.z >> 4,
        }
    }
}

/// Goods with amounts, sorted by good, with no zero entries. Small and flat: a district holds tens
/// of goods, not thousands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub struct GoodVec(Vec<(Good, i64)>);

impl GoodVec {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn from_pairs(pairs: impl IntoIterator<Item = (Good, i64)>) -> Self {
        let mut v = Self::new();
        for (g, a) in pairs {
            v.add(g, a);
        }
        v
    }

    pub fn get(&self, g: Good) -> i64 {
        self.0
            .binary_search_by_key(&g, |e| e.0)
            .map_or(0, |i| self.0[i].1)
    }

    pub fn add(&mut self, g: Good, amount: i64) {
        if amount == 0 {
            return;
        }
        match self.0.binary_search_by_key(&g, |e| e.0) {
            Ok(i) => {
                self.0[i].1 = self.0[i].1.checked_add(amount).expect("good overflow");
                if self.0[i].1 == 0 {
                    self.0.remove(i);
                }
            }
            Err(i) => self.0.insert(i, (g, amount)),
        }
    }

    pub fn set(&mut self, g: Good, amount: i64) {
        let cur = self.get(g);
        self.add(g, amount - cur);
    }

    pub fn merge(&mut self, other: &GoodVec) {
        for &(g, a) in &other.0 {
            self.add(g, a);
        }
    }

    pub fn subtract(&mut self, other: &GoodVec) {
        for &(g, a) in &other.0 {
            self.add(g, -a);
        }
    }

    /// `self - other`, per good.
    pub fn minus(&self, other: &GoodVec) -> GoodVec {
        let mut out = self.clone();
        out.subtract(other);
        out
    }

    pub fn iter(&self) -> impl Iterator<Item = (Good, i64)> + '_ {
        self.0.iter().copied()
    }

    pub fn goods(&self) -> impl Iterator<Item = Good> + '_ {
        self.0.iter().map(|e| e.0)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn total(&self) -> i128 {
        self.0.iter().map(|e| i128::from(e.1)).sum()
    }

    pub fn as_slice(&self) -> &[(Good, i64)] {
        &self.0
    }

    pub fn heap_bytes(&self) -> usize {
        self.0.capacity() * std::mem::size_of::<(Good, i64)>()
    }
}

impl FromIterator<(Good, i64)> for GoodVec {
    fn from_iter<T: IntoIterator<Item = (Good, i64)>>(iter: T) -> Self {
        Self::from_pairs(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_names_round_trip() {
        for name in [
            "i/64/minecraft:iron_ingot",
            "i/1/minecraft:diamond_pickaxe/9f3a12c4",
            "f/0/destroy:mixture/77ab01ff",
            "e/0/fe",
            "d/0/oreexcavation:vein",
        ] {
            let g = GoodInfo::parse(name).unwrap();
            assert_eq!(g.dictionary_name(), name);
        }
        assert!(GoodInfo::parse("i/0/minecraft:air").is_none());
        assert!(GoodInfo::parse("x/1/foo").is_none());
        assert!(GoodInfo::parse("i/64/").is_none());
    }

    #[test]
    fn slot_weights_overstate_never_understate() {
        assert_eq!(GoodInfo::item("a", 64).slot_weight(), 64);
        assert_eq!(GoodInfo::item("a", 16).slot_weight(), 256);
        assert_eq!(GoodInfo::item("a", 1).slot_weight(), 4096);
        // 99 items of a 99-stack fill one slot; 99 × 42 = 4158 >= 4096.
        let w = GoodInfo::item("a", 99).slot_weight();
        assert!(w * 99 >= SLOT_UNITS);
        assert_eq!(GoodInfo::fluid("w").slot_weight(), 0);
    }

    #[test]
    fn good_vec_keeps_sorted_nonzero() {
        let mut v = GoodVec::new();
        v.add(Good(3), 5);
        v.add(Good(1), 2);
        v.add(Good(3), -5);
        assert_eq!(v.as_slice(), &[(Good(1), 2)]);
        v.set(Good(2), 7);
        assert_eq!(v.get(Good(2)), 7);
        let w = v.minus(&GoodVec::from_pairs([(Good(2), 7)]));
        assert_eq!(w.as_slice(), &[(Good(1), 2)]);
    }
}
