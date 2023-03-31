use crate::{AllStorages, Component, EntityId, StorageId, World};
use std::collections::HashMap;

pub type CopyEntityFn = fn(EntityId, &mut AllStorages, EntityId, &mut AllStorages);

pub fn move_component<C: Component + Send + Sync>(
    source_entity: EntityId,
    source: &mut AllStorages,
    dest_entity: EntityId,
    dest: &mut AllStorages,
) {
    // let mut view = source.borrow::<ViewMut<'_, C>>().unwrap();
    // if let Some(component) = view.dyn_remove(source_entity, source.get_current()) {
    //     dest.add_component(dest_entity, component);
    // }

    if let Some(component) = source.remove::<C>(source_entity) {
        dest.add_component(dest_entity, component);
    }
}

// pub fn move_component_non_send<C: Component + Sync>(
//     source_entity: EntityId,
//     source: &AllStorages,
//     dest_entity: EntityId,
//     dest: &mut AllStorages,
// ) {
//     // ERROR - 'C' cannot be sent between threads safely
//     if let Some(component) = source.remove::<C>(source_entity) {
//         dest.add_component(dest_entity, component);
//     }
// }

// pub fn move_component_non_sync<C: Component + Send>(
//     source_entity: EntityId,
//     source: &AllStorages,
//     dest_entity: EntityId,
//     dest: &mut AllStorages,
// ) {
//     // 'C' cannot be shared between threads safely
// }

// pub fn move_component_non_send_sync<C: Component>(
//     source_entity: EntityId,
//     source: &AllStorages,
//     dest_entity: EntityId,
//     dest: &mut AllStorages,
// ) {
//     // 'C' cannot be sent between threads safely
// }

/// A registry of components that can be moved between worlds.
/// Components must be Send + Sync for this to work.
pub struct Registry {
    comps: HashMap<StorageId, CopyEntityFn>,
    // constructors: HashMap<T, (StorageId, fn(&mut EntityLayout))>,
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            comps: HashMap::new(),
        }
    }

    pub fn register<C: Component + Send + Sync>(&mut self) {
        let id = StorageId::of::<C>();
        self.comps.insert(id, move_component::<C>);
    }

    // pub fn register_non_send<C: Component + Sync>(&mut self) {
    //     let id = StorageId::of::<C>();
    //     self.comps.insert(id, move_component_non_send::<C>);
    // }

    // pub fn register_non_sync<C: Component + Send>(&mut self) {
    //     let id = StorageId::of::<C>();
    //     self.comps.insert(id, move_component_non_sync::<C>);
    // }

    // pub fn register_non_send_sync<C: Component + Sync>(&mut self) {
    //     let id = StorageId::of::<C>();
    //     self.comps.insert(id, move_component_non_send_sync::<C>);
    // }

    pub fn iter(&self) -> impl Iterator<Item = (&StorageId, &CopyEntityFn)> {
        self.comps.iter()
    }
}

/// Moves all of an entity's components from one world to another.
/// Deletes the entity from the source world.
pub fn move_entity(entity: EntityId, from_world: &mut World, to_world: &mut World) -> EntityId {
    let new_entity = to_world.add_entity(());

    let mut from_storage = from_world.all_storages_mut().unwrap();
    let mut to_storage = to_world.all_storages_mut().unwrap();

    let registry = from_storage.comp_registry.write().take().unwrap();

    for (_id, move_fn) in registry.iter() {
        move_fn(entity, &mut *from_storage, new_entity, &mut *to_storage);
    }

    *from_storage.comp_registry.write() = Some(registry);

    from_storage.delete_entity(entity);

    new_entity
}
