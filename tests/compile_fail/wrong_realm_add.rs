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

fn main() {
    dreck!(owner1, arena1);
    dreck!(_owner2, arena2);

    let mut container = Container(None);
    let ptr = arena1.add(container);
    container = Container(Some(ptr));
    let ptr = arena2.add(container);

    arena1.collect(&owner1);
}
