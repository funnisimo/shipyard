use crate::all_storages::{AllStorages, CustomStorageAccess, TupleDeleteAny, TupleRetain};
use crate::atomic_refcell::{AtomicRefCell, Ref, RefMut};
use crate::borrow::{Borrow, IntoBorrow};
use crate::entity_id::EntityId;
use crate::info::WorkloadsTypeUsage;
use crate::memory_usage::WorldMemoryUsage;
use crate::public_transport::ShipyardRwLock;
use crate::reserve::BulkEntityIter;
use crate::scheduler::{Batches, Scheduler};
use crate::sparse_set::{BulkAddEntity, TupleAddComponent, TupleDelete, TupleRemove};
use crate::storage::{Storage, StorageId};
use crate::{error, Component};
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU32;

/// `World` contains all data this library will manipulate.
pub struct World {
    pub(crate) all_storages: AtomicRefCell<AllStorages>,
    pub(crate) scheduler: AtomicRefCell<Scheduler>,
    counter: Arc<AtomicU32>,
}

#[cfg(feature = "std")]
impl Default for World {
    /// Creates an empty `World`.
    fn default() -> Self {
        let counter = Arc::new(AtomicU32::new(1));
        World {
            #[cfg(not(feature = "thread_local"))]
            all_storages: AtomicRefCell::new(AllStorages::new(counter.clone())),
            #[cfg(feature = "thread_local")]
            all_storages: AtomicRefCell::new_non_send(
                AllStorages::new(counter.clone()),
                std::thread::current().id(),
            ),
            scheduler: AtomicRefCell::new(Default::default()),
            counter,
        }
    }
}

impl World {
    /// Creates an empty `World`.
    #[cfg(feature = "std")]
    pub fn new() -> Self {
        Default::default()
    }
    #[cfg(all(test, not(feature = "std")))]
    pub fn new() -> Self {
        Self::new_with_custom_lock::<parking_lot::RawRwLock>()
    }
    /// Creates an empty `World` with a custom RwLock for `AllStorages`.
    pub fn new_with_custom_lock<L: ShipyardRwLock>() -> Self {
        let counter = Arc::new(AtomicU32::new(1));
        World {
            #[cfg(not(feature = "thread_local"))]
            all_storages: AtomicRefCell::new(AllStorages::new_with_lock::<L>(counter.clone())),
            #[cfg(feature = "thread_local")]
            all_storages: AtomicRefCell::new_non_send(
                AllStorages::new_with_lock::<L>(counter.clone()),
                std::thread::current().id(),
            ),
            scheduler: AtomicRefCell::new(Default::default()),
            counter,
        }
    }
    /// Adds a new unique storage, unique storages store a single value.  
    /// To access a unique storage value, use [`UniqueView`] or [`UniqueViewMut`].  
    /// Does nothing if the storage already exists.
    ///
    /// ### Borrows
    ///
    /// - [`AllStorages`] (shared)
    ///
    /// ### Errors
    ///
    /// - [`AllStorages`] borrow failed.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, UniqueView, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// let world = World::new();
    ///
    /// world.add_unique(U32(0)).unwrap();
    ///
    /// let i = world.borrow::<UniqueView<U32>>().unwrap();
    /// assert_eq!(i.0, 0);
    /// ```
    ///
    /// [`AllStorages`]: crate::AllStorages
    /// [`UniqueView`]: crate::UniqueView
    /// [`UniqueViewMut`]: crate::UniqueViewMut
    pub fn add_unique<T: Send + Sync + Component>(
        &self,
        component: T,
    ) -> Result<(), error::Borrow> {
        self.all_storages.borrow()?.add_unique(component);
        Ok(())
    }
    /// Adds a new unique storage, unique storages store a single value.  
    /// To access a `!Send` unique storage value, use [`NonSend`] with [`UniqueView`] or [`UniqueViewMut`].  
    /// Does nothing if the storage already exists.
    ///
    /// ### Borrows
    ///
    /// - [`AllStorages`] (shared)
    ///
    /// ### Errors
    ///
    /// - [`AllStorages`] borrow failed.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, NonSend, UniqueView, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// let world = World::new();
    ///
    /// // I'm using `u32` here but imagine it's a `!Send` type
    /// world.add_unique_non_send(U32(0)).unwrap();
    ///
    /// let i = world.borrow::<NonSend<UniqueView<U32>>>().unwrap();
    /// assert_eq!(i.0, 0);
    /// ```
    ///
    /// [`AllStorages`]: crate::AllStorages
    /// [`UniqueView`]: crate::UniqueView
    /// [`UniqueViewMut`]: crate::UniqueViewMut
    /// [`NonSend`]: crate::NonSend
    #[cfg(feature = "thread_local")]
    #[cfg_attr(docsrs, doc(cfg(feature = "thread_local")))]
    pub fn add_unique_non_send<T: Sync + Component>(
        &self,
        component: T,
    ) -> Result<(), error::Borrow> {
        self.all_storages.borrow()?.add_unique_non_send(component);
        Ok(())
    }
    /// Adds a new unique storage, unique storages store a single value.  
    /// To access a `!Sync` unique storage value, use [`NonSync`] with [`UniqueView`] or [`UniqueViewMut`].  
    /// Does nothing if the storage already exists.
    ///
    /// ### Borrows
    ///
    /// - [`AllStorages`] (shared)
    ///
    /// ### Errors
    ///
    /// - [`AllStorages`] borrow failed.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, NonSync, UniqueView, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// let world = World::new();
    ///
    /// // I'm using `u32` here but imagine it's a `!Sync` type
    /// world.add_unique_non_sync(U32(0)).unwrap();
    ///
    /// let i = world.borrow::<NonSync<UniqueView<U32>>>().unwrap();
    /// assert_eq!(i.0, 0);
    /// ```
    ///
    /// [`AllStorages`]: crate::AllStorages
    /// [`UniqueView`]: crate::UniqueView
    /// [`UniqueViewMut`]: crate::UniqueViewMut
    /// [`NonSync`]: crate::NonSync
    #[cfg(feature = "thread_local")]
    #[cfg_attr(docsrs, doc(cfg(feature = "thread_local")))]
    pub fn add_unique_non_sync<T: Send + Component>(
        &self,
        component: T,
    ) -> Result<(), error::Borrow> {
        self.all_storages.borrow()?.add_unique_non_sync(component);
        Ok(())
    }
    /// Adds a new unique storage, unique storages store a single value.  
    /// To access a `!Send + !Sync` unique storage value, use [`NonSendSync`] with [`UniqueView`] or [`UniqueViewMut`].  
    /// Does nothing if the storage already exists.
    ///
    /// ### Borrows
    ///
    /// - [`AllStorages`] (shared)
    ///
    /// ### Errors
    ///
    /// - [`AllStorages`] borrow failed.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, NonSendSync, UniqueView, World};
    ///
    /// let world = World::new();
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// // I'm using `u32` here but imagine it's a `!Send + !Sync` type
    /// world.add_unique_non_send_sync(U32(0)).unwrap();
    ///
    /// let i = world.borrow::<NonSendSync<UniqueView<U32>>>().unwrap();
    /// assert_eq!(i.0, 0);
    /// ```
    ///
    /// [`AllStorages`]: crate::AllStorages
    /// [`UniqueView`]: crate::UniqueView
    /// [`UniqueViewMut`]: crate::UniqueViewMut
    /// [`NonSendSync`]: crate::NonSync
    #[cfg(feature = "thread_local")]
    #[cfg_attr(docsrs, doc(cfg(feature = "thread_local")))]
    pub fn add_unique_non_send_sync<T: Component>(
        &self,
        component: T,
    ) -> Result<(), error::Borrow> {
        self.all_storages
            .borrow()?
            .add_unique_non_send_sync(component);
        Ok(())
    }
    /// Removes a unique storage.
    ///
    /// ### Borrows
    ///
    /// - [`AllStorages`] (shared)
    /// - `Unique<T>` storage (exclusive)
    ///
    /// ### Errors
    ///
    /// - [`AllStorages`] borrow failed.
    /// - `Unique<T>` storage borrow failed.
    /// - `Unique<T>` storage did not exist.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, UniqueView, World};
    ///
    /// #[derive(Component, Debug)]
    /// struct U32(u32);
    ///
    /// let world = World::new();
    ///
    /// world.add_unique(U32(0)).unwrap();
    ///
    /// let i = world.remove_unique::<U32>().unwrap();
    /// assert_eq!(i.0, 0);
    /// ```
    ///
    /// [`AllStorages`]: crate::AllStorages
    pub fn remove_unique<T: Component>(&self) -> Result<T, error::UniqueRemove> {
        self.all_storages
            .borrow()
            .map_err(|_| error::UniqueRemove::AllStorages)?
            .remove_unique::<T>()
    }
    #[doc = "Borrows the requested storages, if they don't exist they'll get created.  
You can use a tuple to get multiple storages at once.

You can use:
* [View]\\<T\\> for a shared access to `T` storage
* [ViewMut]\\<T\\> for an exclusive access to `T` storage
* [EntitiesView] for a shared access to the entity storage
* [EntitiesViewMut] for an exclusive reference to the entity storage
* [AllStoragesViewMut] for an exclusive access to the storage of all components, ⚠️ can't coexist with any other storage borrow
* [UniqueView]\\<T\\> for a shared access to a `T` unique storage
* [UniqueViewMut]\\<T\\> for an exclusive access to a `T` unique storage
* `Option<V>` with one or multiple views for fallible access to one or more storages"]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "* <span style=\"display: table;color: #2f2f2f;background-color: #C4ECFF;border-width: 1px;border-style: solid;border-color: #7BA5DB;padding: 3px;margin-bottom: 5px; font-size: 90%\">This is supported on <strong><code style=\"background-color: #C4ECFF\">feature=\"thread_local\"</code></strong> only:</span>"
    )]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "    * [NonSend]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send`
    * [NonSend]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send`  
[NonSend] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send` unique storage."
    )]
    #[cfg_attr(
        all(feature = "thread_local", not(docsrs)),
        doc = "* [NonSend]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send`
* [NonSend]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send`  
[NonSend] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send` unique storage."
    )]
    #[cfg_attr(
        not(feature = "thread_local"),
        doc = "* NonSend: must activate the *thread_local* feature"
    )]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "    * [NonSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Sync`
    * [NonSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Sync`  
[NonSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Sync` unique storage."
    )]
    #[cfg_attr(
        all(feature = "thread_local", not(docsrs)),
        doc = "* [NonSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Sync`
* [NonSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Sync`  
[NonSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Sync` unique storage."
    )]
    #[cfg_attr(
        not(feature = "thread_local"),
        doc = "* NonSync: must activate the *thread_local* feature"
    )]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "    * [NonSendSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send` nor `Sync`
    * [NonSendSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send` nor `Sync`  
[NonSendSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send + !Sync` unique storage."
    )]
    #[cfg_attr(
        all(feature = "thread_local", not(docsrs)),
        doc = "* [NonSendSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send` nor `Sync`
* [NonSendSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send` nor `Sync`  
[NonSendSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send + !Sync` unique storage."
    )]
    #[cfg_attr(
        not(feature = "thread_local"),
        doc = "* NonSendSync: must activate the *thread_local* feature"
    )]
    #[doc = "
### Borrows

- [AllStorages] (exclusive) when requesting [AllStoragesViewMut]
- [AllStorages] (shared) + storage (exclusive or shared) for all other views

### Errors

- [AllStorages] borrow failed.
- Storage borrow failed.
- Unique storage did not exist.

### Example
```
use shipyard::{Component, EntitiesView, View, ViewMut, World};

#[derive(Component)]
struct U32(u32);

#[derive(Component)]
struct USIZE(usize);

let world = World::new();

let u32s = world.borrow::<View<U32>>().unwrap();
let (entities, mut usizes) = world
    .borrow::<(EntitiesView, ViewMut<USIZE>)>()
    .unwrap();
```
[AllStorages]: crate::AllStorages
[EntitiesView]: crate::Entities
[EntitiesViewMut]: crate::Entities
[AllStoragesViewMut]: crate::AllStorages
[World]: crate::World
[View]: crate::View
[ViewMut]: crate::ViewMut
[UniqueView]: crate::UniqueView
[UniqueViewMut]: crate::UniqueViewMut"]
    #[cfg_attr(feature = "thread_local", doc = "[NonSend]: crate::NonSend")]
    #[cfg_attr(feature = "thread_local", doc = "[NonSync]: crate::NonSync")]
    #[cfg_attr(feature = "thread_local", doc = "[NonSendSync]: crate::NonSendSync")]
    pub fn borrow<'s, V: IntoBorrow>(&'s self) -> Result<V, error::GetStorage>
    where
        V::Borrow: Borrow<'s, View = V>,
    {
        let current = self.get_current();
        V::Borrow::borrow(self, None, current)
    }
    #[doc = "Borrows the requested storages and runs the function.  
Data can be passed to the function, this always has to be a single type but you can use a tuple if needed.

You can use:
* [View]\\<T\\> for a shared access to `T` storage
* [ViewMut]\\<T\\> for an exclusive access to `T` storage
* [EntitiesView] for a shared access to the entity storage
* [EntitiesViewMut] for an exclusive reference to the entity storage
* [AllStoragesViewMut] for an exclusive access to the storage of all components, ⚠️ can't coexist with any other storage borrow
* [UniqueView]\\<T\\> for a shared access to a `T` unique storage
* [UniqueViewMut]\\<T\\> for an exclusive access to a `T` unique storage
* `Option<V>` with one or multiple views for fallible access to one or more storages"]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "* <span style=\"display: table;color: #2f2f2f;background-color: #C4ECFF;border-width: 1px;border-style: solid;border-color: #7BA5DB;padding: 3px;margin-bottom: 5px; font-size: 90%\">This is supported on <strong><code style=\"background-color: #C4ECFF\">feature=\"thread_local\"</code></strong> only:</span>"
    )]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "    * [NonSend]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send`
    * [NonSend]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send`  
[NonSend] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send` unique storage."
    )]
    #[cfg_attr(
        all(feature = "thread_local", not(docsrs)),
        doc = "* [NonSend]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send`
* [NonSend]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send`  
[NonSend] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send` unique storage."
    )]
    #[cfg_attr(
        not(feature = "thread_local"),
        doc = "* NonSend: must activate the *thread_local* feature"
    )]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "    * [NonSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Sync`
    * [NonSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Sync`  
[NonSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Sync` unique storage."
    )]
    #[cfg_attr(
        all(feature = "thread_local", not(docsrs)),
        doc = "* [NonSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Sync`
* [NonSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Sync`  
[NonSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Sync` unique storage."
    )]
    #[cfg_attr(
        not(feature = "thread_local"),
        doc = "* NonSync: must activate the *thread_local* feature"
    )]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "    * [NonSendSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send` nor `Sync`
    * [NonSendSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send` nor `Sync`  
[NonSendSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send + !Sync` unique storage."
    )]
    #[cfg_attr(
        all(feature = "thread_local", not(docsrs)),
        doc = "* [NonSendSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send` nor `Sync`
* [NonSendSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send` nor `Sync`  
[NonSendSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send + !Sync` unique storage."
    )]
    #[cfg_attr(
        not(feature = "thread_local"),
        doc = "* NonSendSync: must activate the *thread_local* feature"
    )]
    #[doc = "
### Borrows

- [AllStorages] (exclusive) when requesting [AllStoragesViewMut]
- [AllStorages] (shared) + storage (exclusive or shared) for all other views

### Errors

- [AllStorages] borrow failed.
- Storage borrow failed.
- Unique storage did not exist.
- Error returned by user.

### Example
```
use shipyard::{Component, EntityId, Get, ViewMut, World};

#[derive(Component)]
struct Position([f32; 2]);

fn sys1((entity, [x, y]): (EntityId, [f32; 2]), mut positions: ViewMut<Position>) {
    if let Ok(mut pos) = (&mut positions).get(entity) {
        pos.0 = [x, y];
    }
}

let world = World::new();

world.run_with_data(sys1, (EntityId::dead(), [0., 0.])).unwrap();
```
[AllStorages]: crate::AllStorages
[EntitiesView]: crate::Entities
[EntitiesViewMut]: crate::Entities
[AllStoragesViewMut]: crate::AllStorages
[World]: crate::World
[View]: crate::View
[ViewMut]: crate::ViewMut
[UniqueView]: crate::UniqueView
[UniqueViewMut]: crate::UniqueViewMut"]
    #[cfg_attr(feature = "thread_local", doc = "[NonSend]: crate::NonSend")]
    #[cfg_attr(feature = "thread_local", doc = "[NonSync]: crate::NonSync")]
    #[cfg_attr(feature = "thread_local", doc = "[NonSendSync]: crate::NonSendSync")]
    pub fn run_with_data<'s, Data, B, R, S: crate::system::System<'s, (Data,), B, R>>(
        &'s self,
        system: S,
        data: Data,
    ) -> Result<R, error::Run> {
        system.run((data,), self).map_err(error::Run::GetStorage)
    }
    #[doc = "Borrows the requested storages and runs the function.

You can use:
* [View]\\<T\\> for a shared access to `T` storage
* [ViewMut]\\<T\\> for an exclusive access to `T` storage
* [EntitiesView] for a shared access to the entity storage
* [EntitiesViewMut] for an exclusive reference to the entity storage
* [AllStoragesViewMut] for an exclusive access to the storage of all components, ⚠️ can't coexist with any other storage borrow
* [UniqueView]\\<T\\> for a shared access to a `T` unique storage
* [UniqueViewMut]\\<T\\> for an exclusive access to a `T` unique storage
* `Option<V>` with one or multiple views for fallible access to one or more storages"]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "* <span style=\"display: table;color: #2f2f2f;background-color: #C4ECFF;border-width: 1px;border-style: solid;border-color: #7BA5DB;padding: 3px;margin-bottom: 5px; font-size: 90%\">This is supported on <strong><code style=\"background-color: #C4ECFF\">feature=\"thread_local\"</code></strong> only:</span>"
    )]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "    * [NonSend]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send`
    * [NonSend]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send`  
[NonSend] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send` unique storage."
    )]
    #[cfg_attr(
        all(feature = "thread_local", not(docsrs)),
        doc = "* [NonSend]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send`
* [NonSend]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send`  
[NonSend] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send` unique storage."
    )]
    #[cfg_attr(
        not(feature = "thread_local"),
        doc = "* NonSend: must activate the *thread_local* feature"
    )]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "    * [NonSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Sync`
    * [NonSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Sync`  
[NonSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Sync` unique storage."
    )]
    #[cfg_attr(
        all(feature = "thread_local", not(docsrs)),
        doc = "* [NonSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Sync`
* [NonSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Sync`  
[NonSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Sync` unique storage."
    )]
    #[cfg_attr(
        not(feature = "thread_local"),
        doc = "* NonSync: must activate the *thread_local* feature"
    )]
    #[cfg_attr(
        all(feature = "thread_local", docsrs),
        doc = "    * [NonSendSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send` nor `Sync`
    * [NonSendSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send` nor `Sync`  
[NonSendSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send + !Sync` unique storage."
    )]
    #[cfg_attr(
        all(feature = "thread_local", not(docsrs)),
        doc = "* [NonSendSync]<[View]\\<T\\>> for a shared access to a `T` storage where `T` isn't `Send` nor `Sync`
* [NonSendSync]<[ViewMut]\\<T\\>> for an exclusive access to a `T` storage where `T` isn't `Send` nor `Sync`  
[NonSendSync] and [UniqueView]/[UniqueViewMut] can be used together to access a `!Send + !Sync` unique storage."
    )]
    #[cfg_attr(
        not(feature = "thread_local"),
        doc = "* NonSendSync: must activate the *thread_local* feature"
    )]
    #[doc = "
### Borrows

- [AllStorages] (exclusive) when requesting [AllStoragesViewMut]
- [AllStorages] (shared) + storage (exclusive or shared) for all other views

### Errors

- [AllStorages] borrow failed.
- Storage borrow failed.
- Unique storage did not exist.
- Error returned by user.

### Example
```
use shipyard::{Component, View, ViewMut, World};

#[derive(Component)]
struct I32(i32);

#[derive(Component)]
struct USIZE(usize);

#[derive(Component)]
struct U32(u32);

fn sys1(i32s: View<I32>) -> i32 {
    0
}

let world = World::new();

world
    .run(|usizes: View<USIZE>, mut u32s: ViewMut<U32>| {
        // -- snip --
    })
    .unwrap();

let i = world.run(sys1).unwrap();
```
[AllStorages]: crate::AllStorages
[EntitiesView]: crate::Entities
[EntitiesViewMut]: crate::Entities
[AllStoragesViewMut]: crate::AllStorages
[World]: crate::World
[View]: crate::View
[ViewMut]: crate::ViewMut
[UniqueView]: crate::UniqueView
[UniqueViewMut]: crate::UniqueViewMut"]
    #[cfg_attr(feature = "thread_local", doc = "[NonSend]: crate::NonSend")]
    #[cfg_attr(feature = "thread_local", doc = "[NonSync]: crate::NonSync")]
    #[cfg_attr(feature = "thread_local", doc = "[NonSendSync]: crate::NonSendSync")]
    pub fn run<'s, B, R, S: crate::system::System<'s, (), B, R>>(
        &'s self,
        system: S,
    ) -> Result<R, error::Run> {
        system.run((), self).map_err(error::Run::GetStorage)
    }
    /// Modifies the current default workload to `name`.
    ///
    /// ### Borrows
    ///
    /// - Scheduler (exclusive)
    ///
    /// ### Errors
    ///
    /// - Scheduler borrow failed.
    /// - Workload did not exist.
    pub fn set_default_workload(
        &self,
        name: impl Into<Cow<'static, str>>,
    ) -> Result<(), error::SetDefaultWorkload> {
        self.scheduler
            .borrow_mut()
            .map_err(|_| error::SetDefaultWorkload::Borrow)?
            .set_default(name.into())
    }
    /// Changes the name of a workload if it exists.
    ///
    /// ### Borrows
    ///
    /// - Scheduler (exclusive)
    ///
    /// ### Errors
    ///
    /// - Scheduler borrow failed.
    pub fn rename_workload(
        &self,
        old_name: impl Into<Cow<'static, str>>,
        new_name: impl Into<Cow<'static, str>>,
    ) -> Result<(), error::Borrow> {
        self.scheduler
            .borrow_mut()?
            .rename(old_name.into(), new_name.into());

        Ok(())
    }
    /// Runs the `name` workload.
    ///
    /// ### Borrows
    ///
    /// - Scheduler (shared)
    /// - Systems' borrow as they are executed
    ///
    /// ### Errors
    ///
    /// - Scheduler borrow failed.
    /// - Workload did not exist.
    /// - Storage borrow failed.
    /// - User error returned by system.
    pub fn run_workload(&self, name: impl AsRef<str>) -> Result<(), error::RunWorkload> {
        let scheduler = self
            .scheduler
            .borrow()
            .map_err(|_| error::RunWorkload::Scheduler)?;

        let batches = scheduler.workload(name.as_ref())?;

        self.run_batches(
            &scheduler.systems,
            &scheduler.system_names,
            batches,
            #[cfg(feature = "tracing")]
            name.as_ref(),
        )
    }
    /// Returns `true` if the world contains the `name` workload.
    ///
    /// ### Borrows
    ///
    /// - Scheduler (shared)
    ///
    /// ### Errors
    ///
    /// - Scheduler borrow failed.
    ///
    /// ### Example
    /// ```
    /// use shipyard::{Workload, World};
    ///
    /// let world = World::new();
    ///
    /// Workload::builder("foo").add_to_world(&world).unwrap();
    ///
    /// assert!(world.contains_workload("foo").unwrap());
    /// assert!(!world.contains_workload("bar").unwrap());
    /// ```
    pub fn contains_workload(&self, name: impl AsRef<str>) -> Result<bool, error::Borrow> {
        Ok(self.scheduler.borrow()?.contains_workload(name.as_ref()))
    }
    #[allow(clippy::type_complexity)]
    pub(crate) fn run_batches(
        &self,
        systems: &[Box<dyn Fn(&World) -> Result<(), error::Run> + Send + Sync + 'static>],
        system_names: &[&'static str],
        batches: &Batches,
        #[cfg(feature = "tracing")] workload_name: &str,
    ) -> Result<(), error::RunWorkload> {
        // Check for empty first to not borrow AllStorages unnecessarily
        if !batches.skip_if.is_empty() {
            // If impossible to check for empty storage, let the workload run and fail later
            if let Ok(all_storages) = self.borrow::<crate::view::AllStoragesView<'_>>() {
                for skip_if in &batches.skip_if {
                    if skip_if(all_storages.clone()) {
                        return Ok(());
                    }
                }
            }
        }

        #[cfg(feature = "tracing")]
        let parent_span = tracing::info_span!("run_workload", %workload_name);

        #[cfg(feature = "parallel")]
        {
            for batch in &batches.parallel {
                let mut result = Ok(());

                rayon::in_place_scope(|scope| {
                    if let Some(index) = batch.0 {
                        scope.spawn(|_| {
                            if batch.1.len() == 1 {
                                result = systems[batch.1[0]](self).map_err(|err| {
                                    error::RunWorkload::Run((system_names[batch.1[0]], err))
                                });
                            } else {
                                use rayon::prelude::*;

                                result = batch.1.par_iter().try_for_each(|&index| {
                                    (systems[index])(self).map_err(|err| {
                                        error::RunWorkload::Run((system_names[index], err))
                                    })
                                });
                            }
                        });

                        #[cfg(feature = "tracing")]
                        {
                            let system_name = system_names[index];

                            tracing::info_span!(parent: parent_span.clone(), "run_system", %system_name).in_scope(|| {
                                systems[index](self).map_err(|err| {
                                    error::RunWorkload::Run((system_name, err))
                                })
                            })?;
                        }

                        #[cfg(not(feature = "tracing"))]
                        {
                            systems[index](self).map_err(|err| {
                                error::RunWorkload::Run((system_names[index], err))
                            })?;
                        }
                    } else if batch.1.len() == 1 {
                        #[cfg(feature = "tracing")]
                        {
                            let system_name = system_names[batch.1[0]];

                            result = tracing::info_span!(parent: parent_span.clone(), "run_system", %system_name).in_scope(|| {
                                systems[batch.1[0]](self).map_err(|err| {
                                error::RunWorkload::Run((system_names[batch.1[0]], err))
                            })});
                        }

                        #[cfg(not(feature = "tracing"))]
                        {
                            result = systems[batch.1[0]](self).map_err(|err| {
                                error::RunWorkload::Run((system_names[batch.1[0]], err))
                            });
                        }
                    } else {
                        use rayon::prelude::*;

                        result = batch.1.par_iter().try_for_each(|&index| {
                            #[cfg(feature = "tracing")]
                            {
                                let system_name = system_names[index];

                                tracing::info_span!(parent: parent_span.clone(), "run_system", %system_name).in_scope(|| {
                                    (systems[index])(self)
                                        .map_err(|err| error::RunWorkload::Run((system_name, err)))
                                })
                            }

                            #[cfg(not(feature = "tracing"))]
                            {
                                (systems[index])(self).map_err(|err| {
                                    error::RunWorkload::Run((system_names[index], err))
                                })
                            }
                        });
                    }

                    Ok(())
                })?;

                result?;
            }

            Ok(())
        }
        #[cfg(not(feature = "parallel"))]
        {
            batches.sequential.iter().try_for_each(|&index| {
                #[cfg(feature = "tracing")]
                {
                    let system_name = system_names[index];

                    tracing::info_span!(parent: parent_span.clone(), "run_system", %system_name)
                        .in_scope(|| {
                            (systems[index])(self)
                                .map_err(|err| error::RunWorkload::Run((system_name, err)))
                        })
                }

                #[cfg(not(feature = "tracing"))]
                {
                    (systems[index])(self)
                        .map_err(|err| error::RunWorkload::Run((system_names[index], err)))
                }
            })
        }
    }
    /// Run the default workload if there is one.
    ///
    /// ### Borrows
    ///
    /// - Scheduler (shared)
    /// - Systems' borrow as they are executed
    ///
    /// ### Errors
    ///
    /// - Scheduler borrow failed.
    /// - Storage borrow failed.
    /// - User error returned by system.
    pub fn run_default(&self) -> Result<(), error::RunWorkload> {
        let scheduler = self
            .scheduler
            .borrow()
            .map_err(|_| error::RunWorkload::Scheduler)?;

        if !scheduler.is_empty() {
            self.run_batches(
                &scheduler.systems,
                &scheduler.system_names,
                scheduler.default_workload(),
                #[cfg(feature = "tracing")]
                &scheduler.default,
            )?
        }
        Ok(())
    }
    /// Returns a `Ref<&AllStorages>`, used to implement custom storages.  
    /// To borrow `AllStorages` you should use `borrow` or `run` with `AllStoragesViewMut`.
    ///
    /// ### Errors
    ///
    /// - `AllStorages` is already borrowed.
    pub fn all_storages(&self) -> Result<Ref<'_, &'_ AllStorages>, error::Borrow> {
        self.all_storages.borrow()
    }
    /// Returns a `RefMut<&mut AllStorages>`, used to implement custom storages.  
    /// To borrow `AllStorages` you should use `borrow` or `run` with `AllStoragesViewMut`.
    ///
    /// ### Errors
    ///
    /// - `AllStorages` is already borrowed.
    pub fn all_storages_mut(&self) -> Result<RefMut<'_, &'_ mut AllStorages>, error::Borrow> {
        self.all_storages.borrow_mut()
    }
    /// Inserts a custom storage to the `World`.
    ///
    /// ### Errors
    ///
    /// - `AllStorages` is already borrowed exclusively.
    pub fn add_custom_storage<S: 'static + Storage + Send + Sync>(
        &self,
        storage_id: StorageId,
        storage: S,
    ) -> Result<(), error::Borrow> {
        let _ = self
            .all_storages
            .borrow()?
            .custom_storage_or_insert_by_id(storage_id, || storage);

        Ok(())
    }

    #[inline]
    pub(crate) fn get_current(&self) -> u32 {
        self.counter
            .fetch_add(1, core::sync::atomic::Ordering::Acquire)
    }

    /// Returns a timestamp used to clear tracking information.
    pub fn get_tracking_timestamp(&self) -> crate::TrackingTimestamp {
        crate::TrackingTimestamp(self.counter.load(core::sync::atomic::Ordering::Acquire))
    }
}

impl World {
    /// Creates a new entity with the components passed as argument and returns its `EntityId`.  
    /// `component` must always be a tuple, even for a single component.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// #[derive(Component)]
    /// struct USIZE(usize);
    ///
    /// let mut world = World::new();
    ///
    /// let entity0 = world.add_entity((U32(0),));
    /// let entity1 = world.add_entity((U32(1), USIZE(11)));
    /// ```
    #[inline]
    pub fn add_entity<C: TupleAddComponent>(&mut self, component: C) -> EntityId {
        self.all_storages.get_mut().add_entity(component)
    }
    /// Creates multiple new entities and returns an iterator yielding the new `EntityId`s.  
    /// `source` must always yield a tuple, even for a single component.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// #[derive(Component)]
    /// struct USIZE(usize);
    ///
    /// let mut world = World::new();
    ///
    /// let new_entities = world.bulk_add_entity((10..20).map(|i| (U32(i as u32), USIZE(i))));
    /// ```
    #[inline]
    pub fn bulk_add_entity<T: BulkAddEntity>(&mut self, source: T) -> BulkEntityIter<'_> {
        self.all_storages.get_mut().bulk_add_entity(source)
    }
    /// Adds components to an existing entity.  
    /// If the entity already owned a component it will be replaced.  
    /// `component` must always be a tuple, even for a single component.  
    ///
    /// ### Panics
    ///
    /// - `entity` is not alive.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// #[derive(Component)]
    /// struct USIZE(usize);
    ///
    /// let mut world = World::new();
    ///
    /// // make an empty entity
    /// let entity = world.add_entity(());
    ///
    /// world.add_component(entity, (U32(0),));
    /// // entity already had a `U32` component so it will be replaced
    /// world.add_component(entity, (U32(1), USIZE(11)));
    /// ```
    #[track_caller]
    #[inline]
    pub fn add_component<C: TupleAddComponent>(&mut self, entity: EntityId, component: C) {
        self.all_storages.get_mut().add_component(entity, component)
    }
    /// Deletes components from an entity. As opposed to `remove`, `delete` doesn't return anything.  
    /// `C` must always be a tuple, even for a single component.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, World};
    ///
    /// #[derive(Component, Debug, PartialEq, Eq)]
    /// struct U32(u32);
    ///
    /// #[derive(Component)]
    /// struct USIZE(usize);
    ///
    /// let mut world = World::new();
    ///
    /// let entity = world.add_entity((U32(0), USIZE(1)));
    ///
    /// world.delete_component::<(U32,)>(entity);
    /// ```
    #[inline]
    pub fn delete_component<C: TupleDelete>(&mut self, entity: EntityId) {
        self.all_storages.get_mut().delete_component::<C>(entity)
    }
    /// Removes components from an entity.  
    /// `C` must always be a tuple, even for a single component.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, World};
    ///
    /// #[derive(Component, Debug, PartialEq, Eq)]
    /// struct U32(u32);
    ///
    /// #[derive(Component)]
    /// struct USIZE(usize);
    ///
    /// let mut world = World::new();
    ///
    /// let entity = world.add_entity((U32(0), USIZE(1)));
    ///
    /// let (i,) = world.remove::<(U32,)>(entity);
    /// assert_eq!(i, Some(U32(0)));
    /// ```
    #[inline]
    pub fn remove<C: TupleRemove>(&mut self, entity: EntityId) -> C::Out {
        self.all_storages.get_mut().remove::<C>(entity)
    }
    /// Deletes an entity with all its components. Returns true if the entity were alive.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// #[derive(Component)]
    /// struct USIZE(usize);
    ///
    /// let mut world = World::new();
    ///
    /// let entity = world.add_entity((U32(0), USIZE(1)));
    ///
    /// assert!(world.delete_entity(entity));
    /// ```
    #[inline]
    pub fn delete_entity(&mut self, entity: EntityId) -> bool {
        self.all_storages.get_mut().delete_entity(entity)
    }
    /// Deletes all components of an entity without deleting the entity.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// #[derive(Component)]
    /// struct USIZE(usize);
    ///
    /// let mut world = World::new();
    ///
    /// let entity = world.add_entity((U32(0), USIZE(1)));
    ///
    /// world.strip(entity);
    /// ```
    #[inline]
    pub fn strip(&mut self, entity: EntityId) {
        self.all_storages.get_mut().strip(entity);
    }
    /// Deletes all entities with any of the given components.  
    /// The storage's type has to be used and not the component.  
    /// `SparseSet` is the default storage.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, SparseSet, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// #[derive(Component)]
    /// struct USIZE(usize);
    ///
    /// #[derive(Component)]
    /// struct STR(&'static str);
    ///
    /// let mut world = World::new();
    ///
    /// let entity0 = world.add_entity((U32(0),));
    /// let entity1 = world.add_entity((USIZE(1),));
    /// let entity2 = world.add_entity((STR("2"),));
    ///
    /// // deletes `entity2`
    /// world.delete_any::<SparseSet<STR>>();
    /// // deletes `entity0` and `entity1`
    /// world.delete_any::<(SparseSet<U32>, SparseSet<USIZE>)>();
    /// ```
    #[inline]
    pub fn delete_any<S: TupleDeleteAny>(&mut self) {
        self.all_storages.get_mut().delete_any::<S>();
    }
    /// Deletes all components of an entity except the ones passed in `S`.  
    /// The storage's type has to be used and not the component.  
    /// `SparseSet` is the default storage.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::{Component, SparseSet, World};
    ///
    /// #[derive(Component)]
    /// struct U32(u32);
    ///
    /// #[derive(Component)]
    /// struct USIZE(usize);
    ///
    /// let mut world = World::new();
    ///
    /// let entity = world.add_entity((U32(0), USIZE(1)));
    ///
    /// world.retain::<SparseSet<U32>>(entity);
    /// ```
    #[inline]
    pub fn retain<S: TupleRetain>(&mut self, entity: EntityId) {
        self.all_storages.get_mut().retain::<S>(entity);
    }
    /// Same as `retain` but uses `StorageId` and not generics.  
    /// You should only use this method if you use a custom storage with a runtime id.
    #[inline]
    pub fn retain_storage(&mut self, entity: EntityId, excluded_storage: &[StorageId]) {
        self.all_storages
            .get_mut()
            .retain_storage(entity, excluded_storage);
    }
    /// Deletes all entities and components in the `World`.
    ///
    /// ### Example
    ///
    /// ```
    /// use shipyard::World;
    ///
    /// let mut world = World::new();
    ///
    /// world.clear();
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        self.all_storages.get_mut().clear();
    }
    /// Clear all deletion and removal tracking data.
    pub fn clear_all_removed_or_deleted(&mut self) {
        self.all_storages.get_mut().clear_all_removed_or_deleted()
    }
    /// Clear all deletion and removal tracking data older than some timestamp.
    pub fn clear_all_removed_or_deleted_older_than_timestamp(
        &mut self,
        timestamp: crate::TrackingTimestamp,
    ) {
        self.all_storages
            .get_mut()
            .clear_all_removed_or_deleted_older_than_timestamp(timestamp)
    }
    /// Make the given entity alive.  
    /// Does nothing if an entity with a greater generation is already at this index.  
    /// Returns `true` if the entity is successfully spawned.
    #[inline]
    pub fn spawn(&mut self, entity: EntityId) -> bool {
        self.all_storages.get_mut().spawn(entity)
    }
    /// Displays storages memory information.
    pub fn memory_usage(&self) -> WorldMemoryUsage<'_> {
        WorldMemoryUsage(self)
    }
    /// Returns a list of workloads, their systems and which storages these systems borrow.
    pub fn workloads_type_usage(&mut self) -> WorkloadsTypeUsage {
        let mut workload_type_info = hashbrown::HashMap::new();

        let scheduler = self.scheduler.get_mut();

        for (workload_name, batches) in &scheduler.workloads {
            workload_type_info.insert(
                workload_name.clone(),
                batches
                    .sequential
                    .iter()
                    .map(|system_index| {
                        let system_name = scheduler.system_names[*system_index];
                        let mut system_storage_borrowed = Vec::new();

                        scheduler.system_generators[*system_index](&mut system_storage_borrowed);

                        (system_name, system_storage_borrowed)
                    })
                    .collect(),
            );
        }

        WorkloadsTypeUsage(workload_type_info)
    }
}

impl core::fmt::Debug for World {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut debug_struct = f.debug_tuple("World");

        if let Ok(all_storages) = self.all_storages.borrow() {
            debug_struct.field(&*all_storages);
        } else {
            debug_struct.field(&"Could not borrow AllStorages");
        }

        if let Ok(scheduler) = self.scheduler.borrow() {
            debug_struct.field(&*scheduler);
        } else {
            debug_struct.field(&"Could not borrow Scheduler");
        }

        debug_struct.finish()
    }
}

impl core::fmt::Debug for WorldMemoryUsage<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Ok(all_storages) = self.0.all_storages.borrow() {
            all_storages.memory_usage().fmt(f)
        } else {
            f.write_str("Could not borrow AllStorages")
        }
    }
}
