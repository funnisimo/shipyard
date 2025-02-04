mod add_component;
mod bulk_add_entity;
mod delete;
mod drain;
mod remove;
mod sparse_array;
mod window;

pub use add_component::TupleAddComponent;
pub use bulk_add_entity::BulkAddEntity;
pub use delete::TupleDelete;
pub use drain::SparseSetDrain;
pub use remove::TupleRemove;
pub use sparse_array::SparseArray;

pub(crate) use window::{FullRawWindow, FullRawWindowMut};

use crate::component::Component;
use crate::entity_id::EntityId;
use crate::memory_usage::StorageMemoryUsage;
use crate::storage::{Storage, StorageId};
use crate::tracking::Tracking;
use crate::tracking::{is_track_within_bounds, TrackingTimestamp};
use crate::{error, track};
use alloc::vec::Vec;
use core::any::type_name;
use core::{
    cmp::{Ord, Ordering},
    fmt,
};

pub(crate) const BUCKET_SIZE: usize = 256 / core::mem::size_of::<EntityId>();

/// Default component storage.
// A sparse array is a data structure with 2 vectors: one sparse, the other dense.
// Only usize can be added. On insertion, the number is pushed into the dense vector
// and sparse[number] is set to dense.len() - 1.
// For all number present in the sparse array, dense[sparse[number]] == number.
// For all other values if set sparse[number] will have any value left there
// and if set dense[sparse[number]] != number.
// We can't be limited to store solely integers, this is why there is a third vector.
// It mimics the dense vector in regard to insertion/deletion.
pub struct SparseSet<T: Component> {
    pub(crate) sparse: SparseArray<EntityId, BUCKET_SIZE>,
    pub(crate) dense: Vec<EntityId>,
    pub(crate) data: Vec<T>,
    pub(crate) last_insert: u32,
    pub(crate) last_modified: u32,
    pub(crate) insertion_data: Vec<u32>,
    pub(crate) modification_data: Vec<u32>,
    pub(crate) deletion_data: Vec<(EntityId, u32, T)>,
    pub(crate) removal_data: Vec<(EntityId, u32)>,
    pub(crate) is_tracking_insertion: bool,
    pub(crate) is_tracking_modification: bool,
    pub(crate) is_tracking_deletion: bool,
    pub(crate) is_tracking_removal: bool,
}

impl<T: fmt::Debug + Component> fmt::Debug for SparseSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.dense.iter().zip(&self.data))
            .finish()
    }
}

impl<T: Component> SparseSet<T> {
    #[inline]
    pub(crate) fn new() -> Self {
        SparseSet {
            sparse: SparseArray::new(),
            dense: Vec::new(),
            data: Vec::new(),
            last_insert: 0,
            last_modified: 0,
            insertion_data: Vec::new(),
            modification_data: Vec::new(),
            deletion_data: Vec::new(),
            removal_data: Vec::new(),
            is_tracking_insertion: false,
            is_tracking_modification: false,
            is_tracking_deletion: false,
            is_tracking_removal: false,
        }
    }
    /// Returns a new [`SparseSet`] to be used in custom storage.
    #[inline]
    pub fn new_custom_storage() -> Self {
        SparseSet::new()
    }
    /// Returns a slice of all the components in this storage.
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }
}

impl<T: Component> SparseSet<T> {
    /// Returns `true` if `entity` owns a component in this storage.
    #[inline]
    pub fn contains(&self, entity: EntityId) -> bool {
        self.index_of(entity).is_some()
    }
    /// Returns the length of the storage.
    #[inline]
    pub fn len(&self) -> usize {
        self.dense.len()
    }
    /// Returns true if the storage's length is 0.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }
}

impl<T: Component> SparseSet<T> {
    /// Returns the index of `entity`'s component in the `dense` and `data` vectors.  
    /// This index is only valid for this storage and until a modification happens.
    #[inline]
    pub fn index_of(&self, entity: EntityId) -> Option<usize> {
        self.sparse.get(entity).and_then(|sparse_entity| {
            if entity.gen() == sparse_entity.gen() {
                Some(sparse_entity.uindex())
            } else {
                None
            }
        })
    }
    /// Returns the index of `entity`'s component in the `dense` and `data` vectors.  
    /// This index is only valid for this storage and until a modification happens.
    ///
    /// # Safety
    ///
    /// `entity` has to own a component of this type.  
    /// The index is only valid until a modification occurs in the storage.
    #[inline]
    pub unsafe fn index_of_unchecked(&self, entity: EntityId) -> usize {
        self.sparse.get_unchecked(entity).uindex()
    }
    /// Returns the `EntityId` at a given `index`.
    #[inline]
    pub fn id_at(&self, index: usize) -> Option<EntityId> {
        self.dense.get(index).copied()
    }
    #[inline]
    pub(crate) fn private_get(&self, entity: EntityId) -> Option<&T> {
        self.index_of(entity)
            .map(|index| unsafe { self.data.get_unchecked(index) })
    }
}

impl<T: Component> SparseSet<T> {
    /// Inserts `value` in the `SparseSet`.
    ///
    /// # Tracking
    ///
    /// In case `entity` had a component of this type, the new component will be considered `modified`.  
    /// In all other cases it'll be considered `inserted`.
    #[track_caller]
    pub(crate) fn insert(&mut self, entity: EntityId, value: T, current: u32) -> Option<T> {
        self.sparse.allocate_at(entity);

        // at this point there can't be nothing at the sparse index
        let sparse_entity = unsafe { self.sparse.get_mut_unchecked(entity) };

        let old_component;

        if sparse_entity.is_dead() {
            *sparse_entity =
                EntityId::new_from_index_and_gen(self.dense.len() as u64, entity.gen());

            if self.is_tracking_insertion {
                self.insertion_data.push(current);
            }
            if self.is_tracking_modification {
                self.modification_data.push(0);
            }

            self.dense.push(entity);
            self.data.push(value);

            old_component = None;
        } else if entity.gen() >= sparse_entity.gen() {
            let old_data = unsafe {
                core::mem::replace(self.data.get_unchecked_mut(sparse_entity.uindex()), value)
            };

            if entity.gen() == sparse_entity.gen() {
                old_component = Some(old_data);
            } else {
                old_component = None;
            }

            sparse_entity.copy_gen(entity);

            let dense_entity = unsafe { self.dense.get_unchecked_mut(sparse_entity.uindex()) };

            if self.is_tracking_modification {
                unsafe {
                    *self
                        .modification_data
                        .get_unchecked_mut(sparse_entity.uindex()) = current;
                }
            }

            dense_entity.copy_index_gen(entity);
        } else {
            old_component = None;
        }

        old_component
    }
}

impl<T: Component> SparseSet<T> {
    /// Same as `delete` but checks tracking at runtime.
    #[inline]
    pub(crate) fn dyn_delete(&mut self, entity: EntityId, current: u32) -> bool {
        if let Some(component) = self.actual_remove(entity) {
            if self.is_tracking_deletion() {
                self.deletion_data.push((entity, current, component));
            }

            true
        } else {
            false
        }
    }

    /// Same as `remove` but checks tracking at runtime.
    #[inline]
    pub(crate) fn dyn_remove(&mut self, entity: EntityId, current: u32) -> Option<T> {
        let component = self.actual_remove(entity);

        if component.is_some() && self.is_tracking_removal() {
            self.removal_data.push((entity, current));
        }

        component
    }

    #[inline]
    pub(crate) fn actual_remove(&mut self, entity: EntityId) -> Option<T> {
        let sparse_entity = self.sparse.get(entity)?;

        if entity.gen() >= sparse_entity.gen() {
            unsafe {
                *self.sparse.get_mut_unchecked(entity) = EntityId::dead();
            }

            self.dense.swap_remove(sparse_entity.uindex());
            if self.is_tracking_insertion() {
                self.insertion_data.swap_remove(sparse_entity.uindex());
            }
            if self.is_tracking_modification() {
                self.modification_data.swap_remove(sparse_entity.uindex());
            }
            let component = self.data.swap_remove(sparse_entity.uindex());

            // The SparseSet could now be empty or the removed component could have been the last one
            if sparse_entity.uindex() < self.dense.len() {
                unsafe {
                    let last = *self.dense.get_unchecked(sparse_entity.uindex());
                    self.sparse
                        .get_mut_unchecked(last)
                        .copy_index(sparse_entity);
                }
            }

            if entity.gen() == sparse_entity.gen() {
                Some(component)
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl<T: Component> SparseSet<T> {
    /// Removes the *inserted* flag on all components of this storage.
    pub(crate) fn private_clear_all_inserted(&mut self, current: u32) {
        self.last_insert = current;
    }
    /// Removes the *modified* flag on all components of this storage.
    pub(crate) fn private_clear_all_modified(&mut self, current: u32) {
        self.last_modified = current;
    }
    /// Removes the *inserted* and *modified* flags on all components of this storage.
    pub(crate) fn private_clear_all_inserted_and_modified(&mut self, current: u32) {
        self.last_insert = current;
        self.last_modified = current;
    }
    /// Clear all deletion tracking data.
    pub fn clear_all_deleted(&mut self) {
        self.deletion_data.clear();
    }
    /// Clear all deletion tracking data older than some timestamp.
    pub fn clear_all_deleted_older_than_timestamp(&mut self, timestamp: TrackingTimestamp) {
        self.deletion_data.retain(|(_, t, _)| {
            is_track_within_bounds(timestamp.0, t.wrapping_sub(u32::MAX / 2), *t)
        });
    }
    /// Clear all removal tracking data.
    pub fn clear_all_removed(&mut self) {
        self.removal_data.clear();
    }
    /// Clear all removal tracking data older than some timestamp.
    pub fn clear_all_removed_older_than_timestamp(&mut self, timestamp: TrackingTimestamp) {
        self.removal_data
            .retain(|(_, t)| is_track_within_bounds(timestamp.0, t.wrapping_sub(u32::MAX / 2), *t));
    }
    /// Clear all deletion and removal tracking data.
    pub fn clear_all_removed_and_deleted(&mut self) {
        self.removal_data.clear();
    }
    /// Clear all deletion and removal tracking data older than some timestamp.
    pub fn clear_all_removed_and_deleted_older_than_timestamp(
        &mut self,
        timestamp: TrackingTimestamp,
    ) {
        self.deletion_data.retain(|(_, t, _)| {
            is_track_within_bounds(timestamp.0, t.wrapping_sub(u32::MAX / 2), *t)
        });
        self.removal_data
            .retain(|(_, t)| is_track_within_bounds(timestamp.0, t.wrapping_sub(u32::MAX / 2), *t));
    }
}

impl<T: Component> SparseSet<T> {
    /// Make this storage track insertions.
    pub fn track_insertion(&mut self) -> &mut SparseSet<T> {
        if !self.is_tracking_insertion() {
            self.is_tracking_insertion = true;

            self.insertion_data
                .extend(core::iter::repeat(0).take(self.dense.len()));
        }

        self
    }
    /// Make this storage track modification.
    pub fn track_modification(&mut self) -> &mut SparseSet<T> {
        if !self.is_tracking_modification() {
            self.is_tracking_modification = true;

            self.modification_data
                .extend(core::iter::repeat(0).take(self.dense.len()));
        }

        self
    }
    /// Make this storage track deletions.
    pub fn track_deletion(&mut self) -> &mut SparseSet<T> {
        self.is_tracking_deletion = true;
        self
    }
    /// Make this storage track removals.
    pub fn track_removal(&mut self) -> &mut SparseSet<T> {
        self.is_tracking_removal = true;
        self
    }
    /// Make this storage track insertions, modifications, deletions and removals.
    pub fn track_all(&mut self) {
        self.track_insertion()
            .track_modification()
            .track_deletion()
            .track_removal();
    }
    /// Returns `true` if the storage tracks insertion.
    pub fn is_tracking_insertion(&self) -> bool {
        self.is_tracking_insertion
    }
    /// Returns `true` if the storage tracks modification.
    pub fn is_tracking_modification(&self) -> bool {
        self.is_tracking_modification
    }
    /// Returns `true` if the storage tracks deletion.
    pub fn is_tracking_deletion(&self) -> bool {
        self.is_tracking_deletion
    }
    /// Returns `true` if the storage tracks removal.
    pub fn is_tracking_removal(&self) -> bool {
        self.is_tracking_removal
    }
    /// Returns `true` if the storage tracks insertion, deletion or removal.
    pub fn is_tracking_any(&self) -> bool {
        self.is_tracking_insertion()
            || self.is_tracking_modification()
            || self.is_tracking_deletion()
            || self.is_tracking_removal()
    }
    pub(crate) fn check_tracking<TRACK: Tracking>(&self) -> Result<(), error::GetStorage> {
        if (TRACK::as_const() & track::InsertionConst != 0 && !self.is_tracking_insertion())
            || (TRACK::as_const() & track::ModificationConst != 0
                && !self.is_tracking_modification())
            || (TRACK::as_const() & track::DeletionConst != 0 && !self.is_tracking_deletion())
            || (TRACK::as_const() & track::RemovalConst != 0 && !self.is_tracking_removal())
        {
            return Err(error::GetStorage::TrackingNotEnabled {
                name: Some(type_name::<SparseSet<T>>()),
                id: StorageId::of::<SparseSet<T>>(),
                tracking: TRACK::as_const(),
            });
        }

        Ok(())
    }
    pub(crate) fn enable_tracking<TRACK: Tracking>(&mut self) {
        if TRACK::as_const() & track::InsertionConst != 0 {
            self.track_insertion();
        }
        if TRACK::as_const() & track::ModificationConst != 0 {
            self.track_modification();
        }
        if TRACK::as_const() & track::DeletionConst != 0 {
            self.track_deletion();
        }
        if TRACK::as_const() & track::RemovalConst != 0 {
            self.track_removal();
        }
    }
}

impl<T: Component> SparseSet<T> {
    /// Reserves memory for at least `additional` components. Adding components can still allocate though.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.dense.reserve(additional);
        self.data.reserve(additional);
    }
    /// Sorts the `SparseSet` with a comparator function, but may not preserve the order of equal elements.
    pub fn sort_unstable_by<F: FnMut(&T, &T) -> Ordering>(&mut self, mut compare: F) {
        let mut transform: Vec<usize> = (0..self.dense.len()).collect();

        transform.sort_unstable_by(|&i, &j| {
            // SAFE dense and data have the same length
            compare(unsafe { self.data.get_unchecked(i) }, unsafe {
                self.data.get_unchecked(j)
            })
        });

        let mut pos;
        for i in 0..transform.len() {
            // SAFE we're in bound
            pos = unsafe { *transform.get_unchecked(i) };
            while pos < i {
                // SAFE we're in bound
                pos = unsafe { *transform.get_unchecked(pos) };
            }
            self.dense.swap(i, pos);
            self.data.swap(i, pos);
        }

        for (i, id) in self.dense.iter().enumerate() {
            unsafe {
                self.sparse.get_mut_unchecked(*id).set_index(i as u64);
            }
        }
    }

    /// Applies the given function `f` to the entities `a` and `b`.\
    /// The two entities shouldn't point to the same component.  
    ///
    /// ### Panics
    ///
    /// - MissingComponent - if one of the entity doesn't have any component in the storage.
    /// - IdenticalIds - if the two entities point to the same component.
    #[track_caller]
    pub(crate) fn private_apply<R, F: FnOnce(&mut T, &T) -> R>(
        &mut self,
        a: EntityId,
        b: EntityId,
        f: F,
        current: u32,
    ) -> R {
        let a_index = self.index_of(a).unwrap_or_else(move || {
            panic!(
                "Entity {:?} does not have any component in this storage.",
                a
            )
        });
        let b_index = self.index_of(b).unwrap_or_else(move || {
            panic!(
                "Entity {:?} does not have any component in this storage.",
                b
            )
        });

        if a_index != b_index {
            if self.is_tracking_modification {
                self.modification_data[a_index] = current;
            }

            let a = unsafe { &mut *self.data.as_mut_ptr().add(a_index) };
            let b = unsafe { &*self.data.as_mut_ptr().add(b_index) };

            f(a, b)
        } else {
            panic!("Cannot use apply with identical components.");
        }
    }

    /// Applies the given function `f` to the entities `a` and `b`.\
    /// The two entities shouldn't point to the same component.  
    ///
    /// ### Panics
    ///
    /// - MissingComponent - if one of the entity doesn't have any component in the storage.
    /// - IdenticalIds - if the two entities point to the same component.
    #[track_caller]
    pub(crate) fn private_apply_mut<R, F: FnOnce(&mut T, &mut T) -> R>(
        &mut self,
        a: EntityId,
        b: EntityId,
        f: F,
        current: u32,
    ) -> R {
        let a_index = self.index_of(a).unwrap_or_else(move || {
            panic!(
                "Entity {:?} does not have any component in this storage.",
                a
            )
        });
        let b_index = self.index_of(b).unwrap_or_else(move || {
            panic!(
                "Entity {:?} does not have any component in this storage.",
                b
            )
        });

        if a_index != b_index {
            if self.is_tracking_modification {
                self.modification_data[a_index] = current;
                self.modification_data[b_index] = current;
            }

            let a = unsafe { &mut *self.data.as_mut_ptr().add(a_index) };
            let b = unsafe { &mut *self.data.as_mut_ptr().add(b_index) };

            f(a, b)
        } else {
            panic!("Cannot use apply with identical components.");
        }
    }

    /// Deletes all components in this storage.
    pub(crate) fn private_clear(&mut self, current: u32) {
        for &id in &self.dense {
            unsafe {
                *self.sparse.get_mut_unchecked(id) = EntityId::dead();
            }
        }

        self.insertion_data.clear();

        let is_tracking_deletion = self.is_tracking_deletion();

        let iter = self
            .dense
            .drain(..)
            .zip(self.data.drain(..))
            .map(|(entity, component)| (entity, current, component));

        if is_tracking_deletion {
            self.deletion_data.extend(iter);
        }
    }

    /// Creates a draining iterator that empties the storage and yields the removed items.
    pub(crate) fn private_drain(&mut self, current: u32) -> SparseSetDrain<'_, T> {
        if self.is_tracking_removal {
            self.removal_data
                .extend(self.dense.iter().map(|&entity| (entity, current)));
        }

        for id in &self.dense {
            // SAFE ids from sparse_set.dense are always valid
            unsafe {
                *self.sparse.get_mut_unchecked(*id) = EntityId::dead();
            }
        }

        let dense_ptr = self.dense.as_ptr();
        let dense_len = self.dense.len();

        unsafe {
            self.dense.set_len(0);
        }

        SparseSetDrain {
            dense_ptr,
            dense_len,
            data: self.data.drain(..),
        }
    }
}

impl<T: Ord + Component> SparseSet<T> {
    /// Sorts the `SparseSet`, but may not preserve the order of equal elements.
    pub fn sort_unstable(&mut self) {
        self.sort_unstable_by(Ord::cmp)
    }
}

impl<T: 'static + Component> Storage for SparseSet<T> {
    #[inline]
    fn delete(&mut self, entity: EntityId, current: u32) {
        self.dyn_delete(entity, current);
    }
    #[inline]
    fn clear(&mut self, current: u32) {
        self.private_clear(current);
    }
    fn memory_usage(&self) -> Option<StorageMemoryUsage> {
        Some(StorageMemoryUsage {
            storage_name: type_name::<Self>().into(),
            allocated_memory_bytes: self.sparse.reserved_memory()
                + (self.dense.capacity() * core::mem::size_of::<EntityId>())
                + (self.data.capacity() * core::mem::size_of::<T>())
                + (self.insertion_data.capacity() * core::mem::size_of::<u32>())
                + (self.deletion_data.capacity() * core::mem::size_of::<(T, EntityId)>())
                + (self.removal_data.capacity() * core::mem::size_of::<EntityId>())
                + core::mem::size_of::<Self>(),
            used_memory_bytes: self.sparse.used_memory()
                + (self.dense.len() * core::mem::size_of::<EntityId>())
                + (self.data.len() * core::mem::size_of::<T>())
                + (self.insertion_data.len() * core::mem::size_of::<u32>())
                + (self.deletion_data.len() * core::mem::size_of::<(EntityId, T)>())
                + (self.removal_data.len() * core::mem::size_of::<EntityId>())
                + core::mem::size_of::<Self>(),
            component_count: self.len(),
        })
    }
    fn sparse_array(&self) -> Option<&SparseArray<EntityId, BUCKET_SIZE>> {
        Some(&self.sparse)
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
    fn clear_all_removed_and_deleted(&mut self) {
        self.deletion_data.clear();
        self.removal_data.clear();
    }
    fn clear_all_removed_and_deleted_older_than_timestamp(&mut self, timestamp: TrackingTimestamp) {
        self.deletion_data.retain(|(_, t, _)| {
            is_track_within_bounds(timestamp.0, t.wrapping_sub(u32::MAX / 2), *t)
        });

        self.removal_data
            .retain(|(_, t)| is_track_within_bounds(timestamp.0, t.wrapping_sub(u32::MAX / 2), *t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Component;

    #[derive(PartialEq, Eq, Debug)]
    struct STR(&'static str);

    impl Component for STR {}

    #[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
    struct I32(i32);

    impl Component for I32 {}

    #[test]
    fn insert() {
        let mut array = SparseSet::new();

        assert!(array
            .insert(EntityId::new_from_parts(0, 0), STR("0"), 0)
            .is_none());
        assert_eq!(array.dense, &[EntityId::new_from_parts(0, 0)]);
        assert_eq!(array.data, &[STR("0")]);
        assert_eq!(
            array.private_get(EntityId::new_from_parts(0, 0)),
            Some(&STR("0"))
        );

        assert!(array
            .insert(EntityId::new_from_parts(1, 0), STR("1"), 0)
            .is_none());
        assert_eq!(
            array.dense,
            &[
                EntityId::new_from_parts(0, 0),
                EntityId::new_from_parts(1, 0)
            ]
        );
        assert_eq!(array.data, &[STR("0"), STR("1")]);
        assert_eq!(
            array.private_get(EntityId::new_from_parts(0, 0)),
            Some(&STR("0"))
        );
        assert_eq!(
            array.private_get(EntityId::new_from_parts(1, 0)),
            Some(&STR("1"))
        );

        assert!(array
            .insert(EntityId::new_from_parts(5, 0), STR("5"), 0)
            .is_none());
        assert_eq!(
            array.dense,
            &[
                EntityId::new_from_parts(0, 0),
                EntityId::new_from_parts(1, 0),
                EntityId::new_from_parts(5, 0)
            ]
        );
        assert_eq!(array.data, &[STR("0"), STR("1"), STR("5")]);
        assert_eq!(
            array.private_get(EntityId::new_from_parts(5, 0)),
            Some(&STR("5"))
        );

        assert_eq!(array.private_get(EntityId::new_from_parts(4, 0)), None);
    }

    #[test]
    fn remove() {
        let mut array = SparseSet::new();
        array.insert(EntityId::new_from_parts(0, 0), STR("0"), 0);
        array.insert(EntityId::new_from_parts(5, 0), STR("5"), 0);
        array.insert(EntityId::new_from_parts(10, 0), STR("10"), 0);

        assert_eq!(
            array.dyn_remove(EntityId::new_from_parts(0, 0), 0),
            Some(STR("0")),
        );
        assert_eq!(
            array.dense,
            &[
                EntityId::new_from_parts(10, 0),
                EntityId::new_from_parts(5, 0)
            ]
        );
        assert_eq!(array.data, &[STR("10"), STR("5")]);
        assert_eq!(array.private_get(EntityId::new_from_parts(0, 0)), None);
        assert_eq!(
            array.private_get(EntityId::new_from_parts(5, 0)),
            Some(&STR("5"))
        );
        assert_eq!(
            array.private_get(EntityId::new_from_parts(10, 0)),
            Some(&STR("10"))
        );

        array.insert(EntityId::new_from_parts(3, 0), STR("3"), 0);
        array.insert(EntityId::new_from_parts(100, 0), STR("100"), 0);
        assert_eq!(
            array.dense,
            &[
                EntityId::new_from_parts(10, 0),
                EntityId::new_from_parts(5, 0),
                EntityId::new_from_parts(3, 0),
                EntityId::new_from_parts(100, 0)
            ]
        );
        assert_eq!(array.data, &[STR("10"), STR("5"), STR("3"), STR("100")]);
        assert_eq!(array.private_get(EntityId::new_from_parts(0, 0)), None);
        assert_eq!(
            array.private_get(EntityId::new_from_parts(3, 0)),
            Some(&STR("3"))
        );
        assert_eq!(
            array.private_get(EntityId::new_from_parts(5, 0)),
            Some(&STR("5"))
        );
        assert_eq!(
            array.private_get(EntityId::new_from_parts(10, 0)),
            Some(&STR("10"))
        );
        assert_eq!(
            array.private_get(EntityId::new_from_parts(100, 0)),
            Some(&STR("100"))
        );

        assert_eq!(
            array.dyn_remove(EntityId::new_from_parts(3, 0), 0),
            Some(STR("3")),
        );
        assert_eq!(
            array.dense,
            &[
                EntityId::new_from_parts(10, 0),
                EntityId::new_from_parts(5, 0),
                EntityId::new_from_parts(100, 0)
            ]
        );
        assert_eq!(array.data, &[STR("10"), STR("5"), STR("100")]);
        assert_eq!(array.private_get(EntityId::new_from_parts(0, 0)), None);
        assert_eq!(array.private_get(EntityId::new_from_parts(3, 0)), None);
        assert_eq!(
            array.private_get(EntityId::new_from_parts(5, 0)),
            Some(&STR("5"))
        );
        assert_eq!(
            array.private_get(EntityId::new_from_parts(10, 0)),
            Some(&STR("10"))
        );
        assert_eq!(
            array.private_get(EntityId::new_from_parts(100, 0)),
            Some(&STR("100"))
        );

        assert_eq!(
            array.dyn_remove(EntityId::new_from_parts(100, 0), 0),
            Some(STR("100"))
        );
        assert_eq!(
            array.dense,
            &[
                EntityId::new_from_parts(10, 0),
                EntityId::new_from_parts(5, 0)
            ]
        );
        assert_eq!(array.data, &[STR("10"), STR("5")]);
        assert_eq!(array.private_get(EntityId::new_from_parts(0, 0)), None);
        assert_eq!(array.private_get(EntityId::new_from_parts(3, 0)), None);
        assert_eq!(
            array.private_get(EntityId::new_from_parts(5, 0)),
            Some(&STR("5"))
        );
        assert_eq!(
            array.private_get(EntityId::new_from_parts(10, 0)),
            Some(&STR("10"))
        );
        assert_eq!(array.private_get(EntityId::new_from_parts(100, 0)), None);
    }

    #[test]
    fn drain() {
        let mut sparse_set = SparseSet::new();

        sparse_set.insert(EntityId::new(0), I32(0), 0);
        sparse_set.insert(EntityId::new(1), I32(1), 0);

        let mut drain = sparse_set.private_drain(0);

        assert_eq!(drain.next(), Some(I32(0)));
        assert_eq!(drain.next(), Some(I32(1)));
        assert_eq!(drain.next(), None);

        drop(drain);

        assert_eq!(sparse_set.len(), 0);
        assert_eq!(sparse_set.private_get(EntityId::new(0)), None);
    }

    #[test]
    fn drain_with_id() {
        let mut sparse_set = SparseSet::new();

        sparse_set.insert(EntityId::new(0), I32(0), 0);
        sparse_set.insert(EntityId::new(1), I32(1), 0);

        let mut drain = sparse_set.private_drain(0).with_id();

        assert_eq!(drain.next(), Some((EntityId::new(0), I32(0))));
        assert_eq!(drain.next(), Some((EntityId::new(1), I32(1))));
        assert_eq!(drain.next(), None);

        drop(drain);

        assert_eq!(sparse_set.len(), 0);
        assert_eq!(sparse_set.private_get(EntityId::new(0)), None);
    }

    #[test]
    fn drain_empty() {
        let mut sparse_set = SparseSet::<I32>::new();

        assert_eq!(sparse_set.private_drain(0).next(), None);
        assert_eq!(sparse_set.private_drain(0).with_id().next(), None);

        assert_eq!(sparse_set.len(), 0);
    }

    #[test]
    fn unstable_sort() {
        let mut array = SparseSet::new();

        for i in (0..100).rev() {
            let mut entity_id = EntityId::zero();
            entity_id.set_index(100 - i);
            array.insert(entity_id, I32(i as i32), 0);
        }

        array.sort_unstable();

        for window in array.data.windows(2) {
            assert!(window[0] < window[1]);
        }
        for i in 0..100 {
            let mut entity_id = crate::entity_id::EntityId::zero();
            entity_id.set_index(100 - i);
            assert_eq!(array.private_get(entity_id), Some(&I32(i as i32)));
        }
    }

    #[test]
    fn partially_sorted_unstable_sort() {
        let mut array = SparseSet::new();

        for i in 0..20 {
            let mut entity_id = EntityId::zero();
            entity_id.set_index(i);
            assert!(array.insert(entity_id, I32(i as i32), 0).is_none());
        }
        for i in (20..100).rev() {
            let mut entity_id = EntityId::zero();
            entity_id.set_index(100 - i + 20);
            assert!(array.insert(entity_id, I32(i as i32), 0).is_none());
        }

        array.sort_unstable();

        for window in array.data.windows(2) {
            assert!(window[0] < window[1]);
        }
        for i in 0..20 {
            let mut entity_id = crate::entity_id::EntityId::zero();
            entity_id.set_index(i);
            assert_eq!(array.private_get(entity_id), Some(&I32(i as i32)));
        }
        for i in 20..100 {
            let mut entity_id = crate::entity_id::EntityId::zero();
            entity_id.set_index(100 - i + 20);
            assert_eq!(array.private_get(entity_id), Some(&I32(i as i32)));
        }
    }

    #[test]
    fn debug() {
        let mut sparse_set = SparseSet::new();

        sparse_set.insert(EntityId::new(0), STR("0"), 0);
        sparse_set.insert(EntityId::new(5), STR("5"), 0);
        sparse_set.insert(EntityId::new(10), STR("10"), 0);

        println!("{:#?}", sparse_set);
    }

    #[test]
    fn multiple_enable_tracking() {
        let mut sparse_set = SparseSet::new();

        sparse_set.insert(EntityId::new_from_parts(0, 0), I32(0), 0);

        sparse_set.track_all();
        sparse_set.track_all();
        sparse_set.track_all();

        assert_eq!(sparse_set.insertion_data.len(), 1);
        assert_eq!(sparse_set.modification_data.len(), 1);
    }
}
