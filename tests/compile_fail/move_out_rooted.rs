use dreck::*;
use std::pin::pin;

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

fn main() {
    dreck!(owner, arena);

    let mut container = Container(None);
    let ptr = arena.add(container);
    container = Container(Some(ptr));
    let ptr = arena.add(container);

    let guard = pin!(RootGuard::new());
    let ptr = root!(&arena, guard, ptr);

    arena.collect(&owner);

    // Container is moved out of the pointer.
    // Its lifetime should still be tied to `ptr` lifetime.
    let v = ptr.borrow_mut(&mut owner, &arena).0.take().unwrap();
    // `ptr` and the container could be collected here.
    arena.collect(&owner);

    // Container is then used.
    assert!(v.borrow(&owner).0.is_none());
}
