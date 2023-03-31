use shipyard::*;

struct USIZE(usize);
impl Component for USIZE {}

struct U32(u32);
impl Component for U32 {}

#[test]
fn move_entity() {
    let mut world_a = World::default();
    let mut world_b = World::default();

    let entity = world_a.add_entity((USIZE(1), U32(2)));

    {
        let (entities, vm_usize, vm_u32) = world_a
            .borrow::<(EntitiesView, View<USIZE>, View<U32>)>()
            .unwrap();
        assert!(entities.is_alive(entity));
        assert!(vm_usize.get(entity).is_ok());
        assert!(vm_u32.get(entity).is_ok());
    }

    let new_entity = shipyard::move_entity(entity, &mut world_a, &mut world_b);

    let (entities, vm_usize, vm_u32) = world_b
        .borrow::<(EntitiesView, View<USIZE>, View<U32>)>()
        .unwrap();
    assert!(entities.is_alive(new_entity));
    assert!(vm_usize.get(new_entity).is_ok());
    assert!(vm_u32.get(new_entity).is_ok());

    let (entities, vm_usize, vm_u32) = world_a
        .borrow::<(EntitiesView, View<USIZE>, View<U32>)>()
        .unwrap();
    assert!(!entities.is_alive(entity));
    assert!(vm_usize.get(entity).is_err());
    assert!(vm_u32.get(entity).is_err());
}
