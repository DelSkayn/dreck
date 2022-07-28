use dreck::*;

fn main() {
    let owner = unsafe{ Owner::new(marker::Invariant::new())};
    let a = {
        let root = unsafe { Root::new(&owner) };
        let a = root.add(0);
        a.borrow(&owner)
    };
    assert_eq!(*a, 0);
}
