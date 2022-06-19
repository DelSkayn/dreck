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

unsafe impl<'a, 'gc, 'cell> Rebind<'a> for Container<'gc, 'cell> {
    type Output = Container<'a, 'cell>;
}

fn foo<'cell>(root: &Root<'cell>, a: Gc<'_, 'cell, Container<'_, 'cell>>) {
    let b_inner = root.add(Container(None));
    let b = root.add(Container(Some(b_inner)));
    coerce_same(a, b);
}

fn coerce_same<'gc, 'cell>(
    _: Gc<'gc, 'cell, Container<'gc, 'cell>>,
    _: Gc<'gc, 'cell, Container<'gc, 'cell>>,
) {
}

fn main() {
    new_root!(owner, root);

    let a_inner = root.add(Container(None));
    let a = root.add(Container(Some(a_inner)));
    {
        let b_inner = root.add(Container(None));
        let b = root.add(Container(Some(b_inner)));
        coerce_same(a, b);
    }
    foo(&root, a);
}
