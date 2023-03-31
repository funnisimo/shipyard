use crate::all_storages::AllStorages;
use crate::atomic_refcell::{ARef, ARefMut};
use crate::storage::{SBox, StorageId};
use crate::{error, Component, SparseSet};
use core::any::type_name;

/// Low level access to storage.
///
/// Useful with custom storage or to define custom views.
pub trait ComponentStorageAccess {
    /// Returns a [`ARef`] to the requested `S` storage.
    fn component_storage<T: 'static + Component + Send + Sync>(
        &self,
    ) -> Result<ARef<'_, &'_ SparseSet<T>>, error::GetStorage>;

    /// Returns a [`ARefMut`] to the requested `S` storage.
    fn component_storage_mut<T: 'static + Component + Send + Sync>(
        &self,
    ) -> Result<ARefMut<'_, &'_ mut SparseSet<T>>, error::GetStorage>;

    /// Returns a [`ARef`] to the requested `S` storage and create it if it does not exist.
    fn component_storage_or_insert<T: 'static + Component + Send + Sync>(
        &self,
    ) -> Result<ARef<'_, &'_ SparseSet<T>>, error::GetStorage>;

    /// Returns a [`ARefMut`] to the requested `S` storage and create it if it does not exist.
    fn component_storage_or_insert_mut<T: 'static + Component + Send + Sync>(
        &self,
    ) -> Result<ARefMut<'_, &'_ mut SparseSet<T>>, error::GetStorage>;
}

impl ComponentStorageAccess for AllStorages {
    #[inline]
    fn component_storage<T: 'static + Component + Send + Sync>(
        &self,
    ) -> Result<ARef<'_, &'_ SparseSet<T>>, error::GetStorage> {
        let storages = self.storages.read();
        let storage = storages.get(&StorageId::of::<SparseSet<T>>());
        if let Some(storage) = storage {
            let storage = unsafe { &*storage.0 }.borrow();
            drop(storages);
            match storage {
                Ok(storage) => Ok(ARef::map(storage, |storage| {
                    storage.as_any().downcast_ref().unwrap()
                })),
                Err(err) => Err(error::GetStorage::StorageBorrow {
                    name: Some(type_name::<SparseSet<T>>()),
                    id: StorageId::of::<SparseSet<T>>(),
                    borrow: err,
                }),
            }
        } else {
            Err(error::GetStorage::MissingStorage {
                name: Some(type_name::<SparseSet<T>>()),
                id: StorageId::of::<SparseSet<T>>(),
            })
        }
    }

    #[inline]
    fn component_storage_mut<T: 'static + Component + Send + Sync>(
        &self,
    ) -> Result<ARefMut<'_, &'_ mut SparseSet<T>>, error::GetStorage> {
        let storages = self.storages.read();
        let storage = storages.get(&StorageId::of::<SparseSet<T>>());
        if let Some(storage) = storage {
            let storage = unsafe { &*storage.0 }.borrow_mut();
            drop(storages);
            match storage {
                Ok(storage) => Ok(ARefMut::map(storage, |storage| {
                    storage.as_any_mut().downcast_mut().unwrap()
                })),
                Err(err) => Err(error::GetStorage::StorageBorrow {
                    name: Some(type_name::<SparseSet<T>>()),
                    id: StorageId::of::<SparseSet<T>>(),
                    borrow: err,
                }),
            }
        } else {
            Err(error::GetStorage::MissingStorage {
                name: Some(type_name::<SparseSet<T>>()),
                id: StorageId::of::<SparseSet<T>>(),
            })
        }
    }

    #[inline]
    fn component_storage_or_insert<T>(
        &self,
    ) -> Result<ARef<'_, &'_ SparseSet<T>>, error::GetStorage>
    where
        T: 'static + Component + Send + Sync,
    {
        let storage_id = StorageId::of::<SparseSet<T>>();

        let storages = self.storages.read();
        let storage = storages.get(&storage_id);
        if let Some(storage) = storage {
            let storage = unsafe { &*storage.0 }.borrow();
            drop(storages);
            match storage {
                Ok(storage) => Ok(ARef::map(storage, |storage| {
                    storage.as_any().downcast_ref().unwrap()
                })),
                Err(err) => Err(error::GetStorage::StorageBorrow {
                    name: Some(type_name::<SparseSet<T>>()),
                    id: StorageId::of::<SparseSet<T>>(),
                    borrow: err,
                }),
            }
        } else {
            drop(storages);
            let mut storages = self.storages.write();

            let storage = unsafe {
                &*storages
                    .entry(storage_id)
                    .or_insert_with(|| {
                        self.comp_registry.write().as_mut().unwrap().register::<T>();
                        SBox::new(SparseSet::<T>::new())
                    })
                    .0
            }
            .borrow()
            .map_err(|err| error::GetStorage::StorageBorrow {
                name: Some(type_name::<SparseSet<T>>()),
                id: StorageId::of::<SparseSet<T>>(),
                borrow: err,
            });

            Ok(ARef::map(storage.unwrap(), |storage| {
                storage.as_any().downcast_ref::<SparseSet<T>>().unwrap()
            }))
        }
    }

    #[inline]
    fn component_storage_or_insert_mut<T>(
        &self,
    ) -> Result<ARefMut<'_, &'_ mut SparseSet<T>>, error::GetStorage>
    where
        T: 'static + Component + Send + Sync,
    {
        let storage_id = StorageId::of::<SparseSet<T>>();

        let storages = self.storages.read();
        let storage = storages.get(&storage_id);
        if let Some(storage) = storage {
            let storage = unsafe { &*storage.0 }.borrow_mut();
            drop(storages);
            match storage {
                Ok(storage) => Ok(ARefMut::map(storage, |storage| {
                    storage.as_any_mut().downcast_mut().unwrap()
                })),
                Err(err) => Err(error::GetStorage::StorageBorrow {
                    name: Some(type_name::<SparseSet<T>>()),
                    id: StorageId::of::<SparseSet<T>>(),
                    borrow: err,
                }),
            }
        } else {
            drop(storages);
            let mut storages = self.storages.write();

            let storage = unsafe {
                &*storages
                    .entry(storage_id)
                    .or_insert_with(|| {
                        self.comp_registry.write().as_mut().unwrap().register::<T>();

                        SBox::new(SparseSet::<T>::new())
                    })
                    .0
            }
            .borrow_mut()
            .map_err(|err| error::GetStorage::StorageBorrow {
                name: Some(type_name::<SparseSet<T>>()),
                id: StorageId::of::<SparseSet<T>>(),
                borrow: err,
            });

            Ok(ARefMut::map(storage.unwrap(), |storage| {
                storage.as_any_mut().downcast_mut::<SparseSet<T>>().unwrap()
            }))
        }
    }
}
