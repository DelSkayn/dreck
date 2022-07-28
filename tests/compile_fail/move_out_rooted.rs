use dreck::*;

pub struct Container<'gc, 'own>(Option<Gc<'gc, 'own, Container<'gc, 'own>>>);

unsafe impl<'gc, 'own> Trace<'own> for Container<'gc, 'own> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace<'t>(&self, trace: Tracer<'t,'own>) {
        self.0.trace(trace)
    }
}

unsafe impl<'from,'to,'own> Bound<'to> for Container<'from, 'own> {
    type Rebound = Container<'to,'own>;
}

fn main() {
    new_root!(owner, root);

    let mut container = Container(None);
    let ptr = root.add(container);
    container = Container(Some(ptr));
    let ptr = root.add(container);

    root!(&root, ptr);

    root.collect(owner);

    let v = ptr.borrow_mut(owner, &root).0.take().unwrap();
    root.collect_full(owner);

    assert!(v.borrow(owner).0.is_none());
}
