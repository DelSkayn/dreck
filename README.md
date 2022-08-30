
# Dreck
A experimental, mostly-safe, garbage collection library for rust build around zero cost abstractions.

A hard problem in GC library is the tracking of roots. The gc needs to know which pointers are considered alive.
In languages with builtin GC's like go and javascript the language itself keeps track of the roots by analyzing the program at compile time or by 
the use of a runtime. 
In the case of rust we need to do the work of keep track of roots ourself if we want to use a GC safely. The most use GC library does this 
by manually keep tracking of roots using considerable bookkeeping which might result in large overhead. This library tries to solve the problem of roots 
tracking by using rust's lifetimes to ensure roots are handled correctly.

## Example

```rust
use dreck::*;

// A struct which can contain a GC managed pointer.
pub struct Container<'gc, 'own>(Option<Gc<'gc, 'own, Container<'gc, 'own>>>);

// Implement a tracing for the container
unsafe impl<'gc, 'own> Trace<'own> for Container<'gc, 'own> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace<'t>(&self, trace: Tracer<'t,'own>) {
        self.0.trace(trace)
    }
}

// Allow the gc lifetime of the container change as needed.
unsafe impl<'from,'to,'own> Bound<'to> for Container<'from, 'own> {
    type Rebound = Container<'to,'own>;
}

fn main() {
    new_root!(owner, root);

    // Create a new container
    let mut container = Container(None);
    // Allocate it as a managed GC pointer 
    let ptr = root.add(container);


    // Add pointer to a allocated container
    let container = Container(Some(ptr));
    let ptr = root.add(container);


    // Here collection is not allowed since `ptr` is a dangling root
    // root.collect(owner);

    // Mark the dangling root as rooted.
    root!(&root, ptr);

    // Now we can run garbage collection
    root.collect(owner);

    // Access a pointer using the owner.
    let v = ptr.borrow_mut(owner, &root).0.take().unwrap();
    assert!(v.borrow(owner).0.is_none());
}
```
