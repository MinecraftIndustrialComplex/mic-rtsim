// SPDX-License-Identifier: AGPL-3.0-or-later
//! Agent storage behind a small facade (P4 stays open).
//!
//! Proposal P4 asks whether agents live in `bevy_ecs` or in our own columns, to be settled by the
//! phase 0 storage bench. Until then systems see only [`RowStore`], and [`ColumnTable`] implements
//! it with plain column vectors. A `bevy_ecs` implementation of the same trait can then be benched
//! against it without touching any system.
//!
//! [`ColumnTable`] is one table of rows with the same columns (a structure of arrays, SCALE.md §9):
//! a system that touches one field streams only that field's column. Rows are dense; removal swaps
//! the last row in, and a generational sparse index keeps handles valid across the move.

use std::any::{Any, TypeId};
use std::marker::PhantomData;

use crate::handle::Handle;

/// Anything storable in a column.
pub trait Component: 'static + Send + Sync {}
impl<T: 'static + Send + Sync> Component for T {}

/// A row's identity in a [`RowStore`]. Six bytes, generation-checked.
pub type RowId = Handle<Row>;

/// Tag type for [`RowId`].
pub enum Row {}

/// The facade systems program against.
pub trait RowStore {
    /// What one row holds.
    type Bundle;

    fn insert(&mut self, row: Self::Bundle) -> RowId;
    fn remove(&mut self, id: RowId) -> bool;
    fn contains(&self, id: RowId) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get<C: Component>(&self, id: RowId) -> Option<&C>;
    fn get_mut<C: Component>(&mut self, id: RowId) -> Option<&mut C>;

    /// The whole column, in dense row order; [`RowStore::ids`] gives the matching ids.
    fn column<C: Component>(&self) -> &[C];
    fn column_mut<C: Component>(&mut self) -> &mut [C];
    fn ids(&self) -> &[RowId];
}

/// Makes an empty column of one component type.
pub type ColumnMaker = fn() -> Box<dyn ErasedColumnBox>;

/// A tuple of components that forms a row.
pub trait Bundle: 'static {
    fn columns() -> Vec<(TypeId, ColumnMaker)>;
    fn push(self, columns: &mut [Box<dyn ErasedColumnBox>]);
}

/// A column with its element type erased: how a table holds columns of different types. Public
/// only because [`Bundle`] names it; nothing outside this module implements it.
pub trait ErasedColumnBox: Send + Sync {
    #[doc(hidden)]
    fn inner(&mut self) -> &mut dyn Any;
    #[doc(hidden)]
    fn inner_ref(&self) -> &dyn Any;
    #[doc(hidden)]
    fn swap_remove(&mut self, i: usize);
    #[doc(hidden)]
    fn heap_bytes(&self) -> usize;
}

impl<T: Component> ErasedColumnBox for Vec<T> {
    fn inner(&mut self) -> &mut dyn Any {
        self
    }
    fn inner_ref(&self) -> &dyn Any {
        self
    }
    fn swap_remove(&mut self, i: usize) {
        Vec::swap_remove(self, i);
    }
    fn heap_bytes(&self) -> usize {
        self.capacity() * std::mem::size_of::<T>()
    }
}

macro_rules! bundle_tuple {
    ($($name:ident $idx:tt),+) => {
        impl<$($name: Component),+> Bundle for ($($name,)+) {
            fn columns() -> Vec<(TypeId, ColumnMaker)> {
                vec![$((TypeId::of::<$name>(), (|| Box::new(Vec::<$name>::new()) as Box<dyn ErasedColumnBox>) as ColumnMaker)),+]
            }
            fn push(self, columns: &mut [Box<dyn ErasedColumnBox>]) {
                $(
                    columns[$idx]
                        .inner()
                        .downcast_mut::<Vec<$name>>()
                        .expect("column order matches the bundle")
                        .push(self.$idx);
                )+
            }
        }
    };
}

bundle_tuple!(A 0);
bundle_tuple!(A 0, B 1);
bundle_tuple!(A 0, B 1, C 2);
bundle_tuple!(A 0, B 1, C 2, D 3);
bundle_tuple!(A 0, B 1, C 2, D 3, E 4);
bundle_tuple!(A 0, B 1, C 2, D 3, E 4, F 5);
bundle_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
bundle_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);

#[derive(Clone, Copy)]
struct Sparse {
    generation: u16,
    /// Dense row, or `u32::MAX` when free.
    dense: u32,
}

/// Rows of bundle `B`, one plain `Vec` per component.
pub struct ColumnTable<B: Bundle> {
    sparse: Vec<Sparse>,
    free: Vec<u32>,
    ids: Vec<RowId>,
    columns: Vec<Box<dyn ErasedColumnBox>>,
    types: Vec<TypeId>,
    _bundle: PhantomData<fn(B)>,
}

impl<B: Bundle> Default for ColumnTable<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Bundle> ColumnTable<B> {
    pub fn new() -> Self {
        let (types, makers): (Vec<_>, Vec<_>) = B::columns().into_iter().unzip();
        for (i, t) in types.iter().enumerate() {
            assert!(
                !types[..i].contains(t),
                "a bundle names one component type twice"
            );
        }
        Self {
            sparse: Vec::new(),
            free: Vec::new(),
            ids: Vec::new(),
            columns: makers.into_iter().map(|m| m()).collect(),
            types,
            _bundle: PhantomData,
        }
    }

    fn column_index<C: Component>(&self) -> Option<usize> {
        let t = TypeId::of::<C>();
        self.types.iter().position(|x| *x == t)
    }

    fn dense_of(&self, id: RowId) -> Option<usize> {
        let s = self.sparse.get(id.index() as usize)?;
        (s.generation == id.generation() && s.dense != u32::MAX).then_some(s.dense as usize)
    }

    /// Heap bytes of every column and index, for the memory budget.
    pub fn heap_bytes(&self) -> usize {
        self.columns.iter().map(|c| c.heap_bytes()).sum::<usize>()
            + self.sparse.capacity() * std::mem::size_of::<Sparse>()
            + self.free.capacity() * 4
            + self.ids.capacity() * std::mem::size_of::<RowId>()
    }
}

impl<B: Bundle> RowStore for ColumnTable<B> {
    type Bundle = B;

    fn insert(&mut self, row: B) -> RowId {
        let dense = u32::try_from(self.ids.len()).expect("table full");
        let id = if let Some(index) = self.free.pop() {
            let s = &mut self.sparse[index as usize];
            s.dense = dense;
            RowId::from_parts(index, s.generation)
        } else {
            let index = u32::try_from(self.sparse.len()).expect("table full");
            self.sparse.push(Sparse {
                generation: 1,
                dense,
            });
            RowId::from_parts(index, 1)
        };
        row.push(&mut self.columns);
        self.ids.push(id);
        id
    }

    fn remove(&mut self, id: RowId) -> bool {
        let Some(dense) = self.dense_of(id) else {
            return false;
        };
        for c in &mut self.columns {
            c.swap_remove(dense);
        }
        self.ids.swap_remove(dense);
        if let Some(&moved) = self.ids.get(dense) {
            self.sparse[moved.index() as usize].dense = dense as u32;
        }
        let s = &mut self.sparse[id.index() as usize];
        s.dense = u32::MAX;
        if s.generation == u16::MAX {
            // Retired, as in `Arena`: never wrap a generation.
            s.generation = 0;
        } else {
            s.generation += 1;
            self.free.push(id.index());
        }
        true
    }

    fn contains(&self, id: RowId) -> bool {
        self.dense_of(id).is_some()
    }

    fn len(&self) -> usize {
        self.ids.len()
    }

    fn get<C: Component>(&self, id: RowId) -> Option<&C> {
        let d = self.dense_of(id)?;
        self.column::<C>().get(d)
    }

    fn get_mut<C: Component>(&mut self, id: RowId) -> Option<&mut C> {
        let d = self.dense_of(id)?;
        self.column_mut::<C>().get_mut(d)
    }

    /// Panics if `C` is not in the bundle: asking a table for a column it never has is a bug.
    fn column<C: Component>(&self) -> &[C] {
        let i = self
            .column_index::<C>()
            .unwrap_or_else(|| panic!("no column {}", std::any::type_name::<C>()));
        self.columns[i]
            .inner_ref()
            .downcast_ref::<Vec<C>>()
            .expect("column type")
    }

    fn column_mut<C: Component>(&mut self) -> &mut [C] {
        let i = self
            .column_index::<C>()
            .unwrap_or_else(|| panic!("no column {}", std::any::type_name::<C>()));
        self.columns[i]
            .inner()
            .downcast_mut::<Vec<C>>()
            .expect("column type")
    }

    fn ids(&self) -> &[RowId] {
        &self.ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Pos(i32, i32);
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Hunger(u8);

    #[test]
    fn insert_get_remove() {
        let mut t: ColumnTable<(Pos, Hunger)> = ColumnTable::new();
        let a = t.insert((Pos(1, 2), Hunger(3)));
        let b = t.insert((Pos(4, 5), Hunger(6)));
        assert_eq!(t.get::<Pos>(a), Some(&Pos(1, 2)));
        t.get_mut::<Hunger>(b).unwrap().0 = 9;
        assert!(t.remove(a));
        assert!(!t.remove(a));
        assert_eq!(t.get::<Pos>(a), None);
        assert_eq!(t.get::<Hunger>(b), Some(&Hunger(9)));
        assert_eq!(t.column::<Pos>(), &[Pos(4, 5)]);
        assert_eq!(t.ids(), &[b]);
        let c = t.insert((Pos(0, 0), Hunger(0)));
        assert_eq!(c.index(), a.index());
        assert_ne!(c, a);
    }

    proptest! {
        /// The table agrees with a model through inserts and removes; columns stay aligned.
        #[test]
        fn matches_model(ops in prop::collection::vec((any::<bool>(), any::<u16>()), 1..300)) {
            let mut t: ColumnTable<(u32, u64)> = ColumnTable::new();
            let mut model: Vec<(RowId, Option<u32>)> = Vec::new();
            for (ins, x) in ops {
                if ins || model.is_empty() {
                    let id = t.insert((u32::from(x), u64::from(x) * 3));
                    model.push((id, Some(u32::from(x))));
                } else {
                    let i = x as usize % model.len();
                    let (id, v) = model[i];
                    prop_assert_eq!(t.remove(id), v.is_some());
                    model[i].1 = None;
                }
                for (id, v) in &model {
                    prop_assert_eq!(t.get::<u32>(*id).copied(), *v);
                    prop_assert_eq!(t.get::<u64>(*id).copied(), v.map(|v| u64::from(v) * 3));
                }
                prop_assert_eq!(t.len(), model.iter().filter(|m| m.1.is_some()).count());
                prop_assert_eq!(t.column::<u32>().len(), t.ids().len());
            }
        }
    }
}
