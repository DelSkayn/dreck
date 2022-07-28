use dreck::*;

pub struct Container<'gc, 'cell>(Option<Gc<'gc, 'cell, Container<'gc, 'cell>>>);

unsafe impl<'gc, 'cell> Trace for Container<'gc, 'cell> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace(&self, trace: dreck::Tracer) {
        self.0.trace(trace)
    }
}

unsafe impl<'r, 'rcell, 'gc, 'cell> Rebind<'r, 'rcell> for Container<'gc, 'cell> {
    type Gc = Container<'r, 'cell>;
    type Cell = Container<'gc, 'rcell>;
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
