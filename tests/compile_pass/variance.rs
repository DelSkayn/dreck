use dreck::*;

fn coerce_same<'gc, 'own, A: Trace<'own>, B: Trace<'own>>(_: Gc<'gc, 'own, A>, _: Gc<'gc, 'own, B>) {}

fn main() {
    new_root!(owner, root);

    let a = root.add(0u32);
    {
        let b = root.add(1u32);
        root!(&root, b);
        coerce_same(a, b);
    }
}
