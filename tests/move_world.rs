use shipyard::*;

struct USIZE(usize);
impl Component for USIZE {}

struct U32(u32);
impl Component for U32 {}

#[test]
fn move_world() {
    let mut world_a = World::default();
    let mut world_b = World::default();

    println!("Adding entity to world A");

    let entity = {
        let mut entities_a = world_a.borrow::<EntitiesViewMut>().unwrap();
        let mut vm_usize_a = world_a.borrow::<ViewMut<USIZE>>().unwrap();
        let mut vm_u32_a = world_a.borrow::<ViewMut<U32>>().unwrap();

        let entity = entities_a.add_entity((&mut vm_usize_a, &mut vm_u32_a), (USIZE(1), U32(2)));

        assert!(vm_usize_a.get(entity).is_ok());
        assert!(vm_u32_a.get(entity).is_ok());

        entity
    };

    println!("Moving entity to world B");

    let new_entity = shipyard::move_entity(entity, &mut world_a, &mut world_b);

    println!("Checking results");

    let entities_b = world_b.borrow::<EntitiesView>().unwrap();
    let vm_usize_b = world_b.borrow::<View<USIZE>>().unwrap();
    let vm_u32_b = world_b.borrow::<View<U32>>().unwrap();
    assert!(entities_b.is_alive(new_entity));
    assert!(vm_usize_b.get(new_entity).is_ok());
    assert!(vm_u32_b.get(new_entity).is_ok());

    let entities_a = world_a.borrow::<EntitiesView>().unwrap();
    let vm_usize_a = world_a.borrow::<View<USIZE>>().unwrap();
    let vm_u32_a = world_a.borrow::<View<U32>>().unwrap();
    assert!(!entities_a.is_alive(entity));
    assert!(vm_usize_a.get(entity).is_err());
    assert!(vm_u32_a.get(entity).is_err());
}
