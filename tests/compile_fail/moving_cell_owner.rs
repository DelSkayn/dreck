use dreck::*;

struct DerefDrop<'gc, 'own> {
    ptr: Gc<'gc, 'own, i32>,
    owner: Owner<'own>,
}

unsafe impl<'gc, 'own> Trace<'own> for DerefDrop<'gc, 'own> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace<'t>(&self, trace: Tracer<'t,'own>) {
        trace.mark(self.ptr);
    }
}

unsafe impl<'from,'to, 'own> Bound<'to> for DerefDrop<'from, 'own> {
    type Rebound = DerefDrop<'to, 'own>;
}

impl<'gc, 'own> Drop for DerefDrop<'gc, 'own> {
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
