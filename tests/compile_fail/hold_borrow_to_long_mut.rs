use dreck::*;

fn main() {
    let mut owner = unsafe{ Owner::new(marker::Invariant::new())};
    let a = {
        let root = unsafe { Root::new(&owner) };
        let a = root.add(0);
        a.borrow_mut_untraced(&mut owner)
    };
    assert_eq!(*a, 0);
}
