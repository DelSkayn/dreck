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
    dreck!(owner1, arena1);
    dreck!(owner2, arena2);

    let mut container = Container(None);
    let ptr = arena1.add(container);

    let guard = pin!(RootGuard::new());
    let _ptr = root!(&arena2, guard, ptr);

    arena1.collect(&owner1);
    arena2.collect(&owner2);
}
