use dreck::*;

fn same_root<'cell>(_: &Root<'cell>, _: &Root<'cell>) {}

fn main() {
    new_root!(_owner1, root1);
    new_root!(_owner2, root2);

    same_root(&root1, &root2);
}
