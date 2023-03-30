use std::pin::pin;

use dreck::*;

fn coerce_same<'gc, 'own, A, B>(_a: Gc<'gc, 'own, A>, _b: Gc<'gc, 'own, B>) {}

#[test]
fn test_covariant() {
    dreck!(_owner, arena);

    let ptr = arena.add(0);
    let ptr_rooted = arena.add(0);

    let guard = pin!(RootGuard::new());
    let ptr_rooted = root!(&arena, guard, ptr_rooted);

    coerce_same(ptr, ptr_rooted);
}
