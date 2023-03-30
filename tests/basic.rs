use std::pin::pin;

use dreck::*;

pub struct Container<'gc, 'own>(Option<Gc<'gc, 'own, Container<'gc, 'own>>>);

unsafe impl<'gc, 'own> Trace<'own> for Container<'gc, 'own> {
    type Gc<'to> = Container<'to, 'own>;

    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace(&self, marker: Marker<'own, '_>) {
        self.0.trace(marker)
    }
}

#[test]
fn basic() {
    dreck!(owner, arena);

    let ptr = arena.add(Container(None));
    let ptr = arena.add(Container(Some(ptr)));

    let guard = pin!(RootGuard::new());
    let ptr = root!(&arena, guard, ptr);

    arena.collect_full(&owner);

    assert!(ptr.borrow(&owner).0.is_some());
}
