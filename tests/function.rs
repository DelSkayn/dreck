use dreck::*;

fn alloc_mut<'gc, 'cell>(arena: &'gc mut Root<'cell>) -> Gc<'gc, 'cell, u32> {
    arena.add(0 as u32)
}

#[test]
fn allocate() {
    new_root!(owner, root);

    let a = root.add(1i32);

    *a.borrow_mut_untraced(&mut owner) += 1;

    assert_eq!(*a.borrow(&owner), 2);
}

#[test]
fn collect() {
    new_root!(owner, root);

    let a = root.add(1i32);

    *a.borrow_mut_untraced(&mut owner) += 1;

    assert_eq!(*a.borrow(&owner), 2);
    root.collect_full(&mut owner);
}

#[test]
fn root() {
    new_root!(owner, root);

    let a = root.add(1i32);
    root!(&root, a);
    let b = a;

    *b.borrow_mut_untraced(&mut owner) += 1;

    root.collect_full(&mut owner);

    assert_eq!(*b.borrow(&owner), 2);
}

#[test]
fn rebind_root() {
    new_root!(owner, root);

    let a = alloc_mut(&mut root);
    let a = rebind!(&root, a);
    let b = root.add(1u32);

    *a.borrow_mut_untraced(&mut owner) += 1;

    assert_eq!(*a.borrow(&owner), *b.borrow(&owner));
}

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

#[test]
fn container_trace() {
    new_root!(owner, root);

    let mut container = Container(None);

    for _ in 0..20 {
        let alloc = root.add(container);
        container = Container(Some(alloc))
    }
    let alloc = root.add(container);
    root!(&root, alloc);

    root.collect_full(&mut owner);

    assert!(alloc.borrow(&owner).0.is_some())
}
