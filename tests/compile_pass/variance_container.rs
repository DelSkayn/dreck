use dreck::*;

pub struct Container<'gc, 'own>(Option<Gc<'gc, 'own, Container<'gc, 'own>>>);

unsafe impl<'gc, 'own> Trace<'own> for Container<'gc, 'own> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace<'r>(&self, trace: Tracer<'r,'own>) {
        self.0.trace(trace)
    }
}

unsafe impl<'from, 'to, 'own> Bound<'to> for Container<'from, 'own> {
    type Rebound = Container<'to, 'own>;
}

fn foo<'own>(root: &Root<'own>, a: Gc<'_, 'own, Container<'_, 'own>>) {
    let b_inner = root.add(Container(None));
    let b = root.add(Container(Some(b_inner)));
    coerce_same(a, b);
}

fn coerce_same<'gc, 'own>(
    _: Gc<'gc, 'own, Container<'gc, 'own>>,
    _: Gc<'gc, 'own, Container<'gc, 'own>>,
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
