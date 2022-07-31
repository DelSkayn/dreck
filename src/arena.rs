pub struct Arena<R: Static> {
    owner: CellOwner<'static>,
    root: Root<'static>,
    value: NonNull<GcBox<R>>,
}

impl<R: Static> Arena<R>{
    fn new<F>(f: F) -> Self
        where F: for<'gc,'cell> FnOnce(owner: &mut CellOwner<'cell>,&'gc mut Root<'cell>) -> R
    {
        
        let owner = unsafe{ 
            CellOwner::new() };
        let root = unsafe{ Root::new(&owner) };
        let r = f(&owner,&root);
        let owner = root.add(r);
        Arena{
            owner,
            root,
            value: No

        }
    }
}
