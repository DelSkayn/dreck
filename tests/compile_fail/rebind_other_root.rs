use dreck::*;

fn main() {
    new_root!(owner1, root1);
    new_root!(owner2, root2);

    let ptr = root1.add(0);
    rebind!(&root2, ptr);
}
