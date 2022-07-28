use dreck::*;

fn same_root<'own>(_: &Root<'own>, _: &Root<'own>) {}

fn main() {
    new_root!(_owner1, root1);
    new_root!(_owner2, root2);

    same_root(root1, root2);
}
