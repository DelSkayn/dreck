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

type GcContainer<'gc, 'own> = Gc<'gc, 'own, Container<'gc, 'own>>;

fn coerce_same<'gc, 'own>(_a: GcContainer<'gc, 'own>, _b: GcContainer<'gc, 'own>) {}

#[test]
fn coerce_same_container() {
    dreck!(_owner, arena);

    let ptr = arena.add(Container(None));
    let ptr = arena.add(Container(Some(ptr)));
    let ptr_rooted = arena.add(Container(None));
    let ptr_rooted = arena.add(Container(Some(ptr_rooted)));

    let guard = pin!(RootGuard::new());
    let ptr_rooted = root!(&arena, guard, ptr_rooted);

    coerce_same(ptr, ptr_rooted);
}
