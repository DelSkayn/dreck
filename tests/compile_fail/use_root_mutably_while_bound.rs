use dreck::*;

fn use_root<'own>(owner: &Owner<'own>, root: &mut Root<'own>, _: Gc<'_, 'own, i32>) {
    root.collect_full(owner);
}

fn main() {
    new_root!(owner, root);

    let v = root.add(0);
    use_root(&owner, root, v);
}
