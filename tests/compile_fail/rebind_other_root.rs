use dreck::*;

fn main() {
    new_root!(owner1, root1);
    new_root!(owner2, root2);

    let ptr = root1.add(0);

    let ptr = unsafe{ rebind(ptr) };
    let ptr = root2.rebind_to(ptr);

    root1.collect_full(owner1);
    assert_eq!(*ptr.borrow(owner2),0);
}
