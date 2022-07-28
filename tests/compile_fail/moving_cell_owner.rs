use dreck::*;

struct DerefDrop<'gc, 'cell> {
    ptr: Gc<'gc, 'cell, i32>,
    owner: CellOwner<'cell>,
}

unsafe impl<'gc, 'cell> Trace for DerefDrop<'gc, 'cell> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace(&self, trace: Tracer) {
        trace.mark(self.ptr);
    }
}

unsafe impl<'r, 'rcell, 'gc, 'cell> Rebind<'r, 'rcell> for DerefDrop<'gc, 'cell> {
    type Gc = DerefDrop<'r, 'cell>;
    type Cell = DerefDrop<'gc, 'rcell>;
}

impl<'gc, 'cell> Drop for DerefDrop<'gc, 'cell> {
    fn drop(&mut self) {
        // Accessing ptr in drop, very unsafe.
        self.ptr.borrow(&self.owner);
    }
}

fn main() {
    new_root!(owner, root);
    let ptr = root.add(0);
    let v = DerefDrop { ptr, owner: *owner };
    root.add(v);
    // ptr is collected here which will acces its contained pointer
    // which is UB.
}
