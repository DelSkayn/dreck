use dreck::*;

fn use_root<'cell>(owner: &CellOwner<'cell>, root: &mut Root<'cell>, _: Gc<'_, 'cell, i32>) {
    root.collect_full(owner);
}

fn main() {
    new_root!(owner, root);

    let v = root.add(0);
    use_root(&owner, &mut root, v);
}
