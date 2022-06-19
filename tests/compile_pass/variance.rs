use dreck::*;

fn coerce_same<'gc, 'cell, A, B>(_: Gc<'gc, 'cell, A>, _: Gc<'gc, 'cell, B>) {}

fn main() {
    new_root!(owner, root);

    let a = root.add(0u32);
    {
        let b = root.add(1u32);
        root!(&root, b);
        coerce_same(a, b);
    }
}
