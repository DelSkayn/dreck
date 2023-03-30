//! A safe arena implemention which roots all created gc pointers until the end of a specific scope.

use std::{pin::pin, ptr::NonNull};

use crate::{
    sys::{GcBox, UnsafeArena, UnsafeRootGuard, UnsafeTrace},
    Invariant, Owner, Trace,
};

struct ScopedGuards(Vec<NonNull<GcBox<()>>>);

unsafe impl UnsafeTrace for ScopedGuards {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace(&self, marker: crate::sys::UnsafeMarker) {
        for v in self.0.iter().copied() {
            unsafe {
                marker.mark_erased(v);
            }
        }
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Gc<'own, T> {
    ptr: NonNull<GcBox<T>>,
    _invariant: Invariant<'own>,
}

impl<'own, T: Trace<'own>> Gc<'own, T> {
    pub fn borrow<'a>(self, owner: &'a Owner<'own>) -> &'a T {
        let _owner = owner;
        unsafe { &(*self.ptr.as_ref().value.get()) }
    }

    pub fn borrow_mut<'a>(self, owner: &'a mut Owner<'own>, arena: &ArenaScope<'own>) -> &'a mut T {
        let _owner = owner;
        unsafe { arena.arena.arena.write_barrier(self.ptr) }
        unsafe { &mut (*self.ptr.as_ref().value.get()) }
    }
}

pub struct ScopedArena {
    roots: GcBox<ScopedGuards>,
    arena: UnsafeArena,
}

#[repr(transparent)]
pub struct ArenaScope<'own> {
    arena: ScopedArena,
    _invariant: Invariant<'own>,
}

impl<'own> ArenaScope<'own> {
    pub fn add<T: Trace<'own>>(&self, value: T) -> Gc<'own, T> {
        unsafe {
            let ptr = self.arena.arena.add(value);
            (*self.arena.roots.value.get()).0.push(ptr.cast());
            Gc {
                ptr,
                _invariant: Invariant::new(),
            }
        }
    }

    pub fn collect(&self) {
        unsafe { self.arena.arena.collect() }
    }
    pub fn collect_full(&self) {
        unsafe { self.arena.arena.collect() }
    }
}

impl ScopedArena {
    pub fn new() -> Self {
        unsafe {
            let roots = GcBox::new(ScopedGuards(Vec::new()));
            ScopedArena {
                roots,
                arena: UnsafeArena::new(),
            }
        }
    }

    pub fn with<R, F: for<'own> FnOnce(&mut Owner<'own>, &ArenaScope<'own>) -> R>(
        &mut self,
        f: F,
    ) -> R {
        let guard = pin!(UnsafeRootGuard::new());
        let len = unsafe { (*self.roots.value.get()).0.len() };
        let roots = NonNull::from(&self.roots);

        unsafe {
            self.arena.root(guard, roots);
        }

        let scope: &ArenaScope = unsafe { std::mem::transmute(&*self) };
        let mut owner = unsafe { Owner::new() };

        let res = f(&mut owner, scope);

        unsafe {
            (*self.roots.value.get()).0.drain(..len);
        }

        res
    }
}

impl Default for ScopedArena {
    fn default() -> Self {
        Self::new()
    }
}
