use dreck::*;

fn same_owner<'own>(_: &Owner<'own>, _: &Owner<'own>) {}

fn main() {
    new_root!(owner1, _root1);
    new_root!(owner2, _root2);

    same_owner(owner1, owner2);
}
