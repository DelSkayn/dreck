use dreck::*;

fn same_owner<'cell>(_: &CellOwner<'cell>, _: &CellOwner<'cell>) {}

fn main() {
    new_root!(owner1, _root1);
    new_root!(owner2, _root2);

    same_owner(&owner1, &owner2);
}
