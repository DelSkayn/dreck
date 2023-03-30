fn main() {
    let ptr = {
        dreck::dreck!(_owner, arena);

        arena.add(3)
    };
    std::mem::drop(ptr);
}
