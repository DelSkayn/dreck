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
    new_root!(owner1, root1);
    new_root!(owner2, root2);

    let ptr = root1.add(Container(None));
    let v = Container(Some(ptr));

    let v = rebind!(root2,v);

    root1.collect_full(owner1);

    assert!(v.0.unwrap().borrow(owner1).0.is_none());
}
