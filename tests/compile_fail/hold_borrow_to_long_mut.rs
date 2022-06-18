use dreck::*;

fn main() {
    new_cell_owner!(owner);
    let a = {
        let root = unsafe { Root::new(owner) };
        let a = root.add(0);
        a.borrow_mut(owner)
    };
    assert_eq!(*a, 0);
}
