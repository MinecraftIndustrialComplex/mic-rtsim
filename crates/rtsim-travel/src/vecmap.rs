// SPDX-License-Identifier: AGPL-3.0-or-later
//! A small map kept as a sorted vector (overnight choice E58).
//!
//! An edge direction carries a handful of pipelines (one per good it moves, one per currency it
//! pays in). A `BTreeMap` allocates an eleven-entry node for even one of them; a sorted vector
//! holds exactly what it has. Lookups are binary searches; iteration is in key order, as the
//! B-tree's was, so nothing that iterates changes order.

/// A map from `K` to `V`, sorted by key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VecMap<K, V> {
    pub(crate) items: Vec<(K, V)>,
}

impl<K, V> Default for VecMap<K, V> {
    fn default() -> Self {
        VecMap { items: Vec::new() }
    }
}

impl<K: Ord + Copy, V> VecMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn find(&self, k: &K) -> Result<usize, usize> {
        self.items.binary_search_by(|x| x.0.cmp(k))
    }

    pub fn get(&self, k: &K) -> Option<&V> {
        self.find(k).ok().map(|i| &self.items[i].1)
    }

    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        match self.find(k) {
            Ok(i) => Some(&mut self.items[i].1),
            Err(_) => None,
        }
    }

    pub fn contains_key(&self, k: &K) -> bool {
        self.find(k).is_ok()
    }

    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        match self.find(&k) {
            Ok(i) => Some(std::mem::replace(&mut self.items[i].1, v)),
            Err(i) => {
                self.items.insert(i, (k, v));
                None
            }
        }
    }

    pub fn remove(&mut self, k: &K) -> Option<V> {
        self.find(k).ok().map(|i| self.items.remove(i).1)
    }

    /// The entry for `k`, as `BTreeMap::entry` for the one use this needs.
    pub fn entry(&mut self, k: K) -> Entry<'_, K, V> {
        Entry { map: self, key: k }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.items.iter().map(|(k, v)| (k, v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.items.iter_mut().map(|(k, v)| (&*k, v))
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.items.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.items.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.items.iter_mut().map(|(_, v)| v)
    }

    pub fn retain(&mut self, mut f: impl FnMut(&K, &mut V) -> bool) {
        self.items.retain_mut(|(k, v)| f(k, v));
        if self.items.is_empty() {
            self.items = Vec::new();
        }
    }

    /// Heap bytes held.
    pub fn heap_bytes(&self) -> usize {
        self.items.capacity() * std::mem::size_of::<(K, V)>()
    }
}

impl<'a, K: Ord + Copy, V> IntoIterator for &'a VecMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::iter::Map<std::slice::Iter<'a, (K, V)>, fn(&'a (K, V)) -> (&'a K, &'a V)>;

    fn into_iter(self) -> Self::IntoIter {
        fn pair<K, V>(x: &(K, V)) -> (&K, &V) {
            (&x.0, &x.1)
        }
        self.items.iter().map(pair)
    }
}

pub struct Entry<'a, K, V> {
    map: &'a mut VecMap<K, V>,
    key: K,
}

impl<'a, K: Ord + Copy, V> Entry<'a, K, V> {
    pub fn or_insert_with(self, f: impl FnOnce() -> V) -> &'a mut V {
        let i = match self.map.find(&self.key) {
            Ok(i) => i,
            Err(i) => {
                self.map.items.insert(i, (self.key, f()));
                i
            }
        };
        &mut self.map.items[i].1
    }

    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behaves_like_a_sorted_map() {
        let mut m: VecMap<u16, i64> = VecMap::new();
        *m.entry(5).or_insert_with(|| 1) += 1;
        *m.entry(2).or_default() += 7;
        m.insert(9, 3);
        assert_eq!(m.keys().copied().collect::<Vec<_>>(), vec![2, 5, 9]);
        assert_eq!(m.get(&5), Some(&2));
        m.retain(|k, _| *k != 5);
        assert_eq!(m.len(), 2);
        assert_eq!(m.remove(&2), Some(7));
        m.retain(|_, _| false);
        assert!(m.is_empty() && m.heap_bytes() == 0);
    }
}
