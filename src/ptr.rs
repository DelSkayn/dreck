use std::{mem::ManuallyDrop, ptr::NonNull};

use crate::{arena::Marker, marker::Covariant, sys::GcBox, Arena, Invariant, Owner, Trace};

/// A safe pointer to a GC allocated value.
#[repr(transparent)]
pub struct Gc<'gc, 'own, T> {
    ptr: NonNull<GcBox<T>>,
    _gc_marker: Covariant<'gc>,
    _cell_marker: Invariant<'own>,
}

impl<'gc, 'own, T> Clone for Gc<'gc, 'own, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'gc, 'own, T> Copy for Gc<'gc, 'own, T> {}

unsafe impl<'gc, 'own, T: Trace<'own>> Trace<'own> for Gc<'gc, 'own, T> {
    type Gc<'a> = Gc<'a, 'own, T::Gc<'a>>;

    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace(&self, marker: Marker<'own, '_>) {
        marker.mark(*self);
    }
}

impl<'gc, 'own, T> Gc<'gc, 'own, T> {
    pub unsafe fn from_gc_box(ptr: NonNull<GcBox<T>>) -> Self {
        Gc {
            ptr,
            _gc_marker: Covariant::new(),
            _cell_marker: Invariant::new(),
        }
    }

    pub fn into_gc_box(self) -> NonNull<GcBox<T>> {
        self.ptr
    }

    /// Borrow the contained value.
    pub fn borrow<'a>(self, owner: &'a Owner<'own>) -> &'a T {
        let _owner = owner;

        unsafe { &(*self.ptr.as_ref().value.get()) }
    }
}

impl<'gc, 'own, T: Trace<'own>> Gc<'gc, 'own, T> {
    pub fn borrow_mut<'a>(
        self,
        owner: &'a mut Owner<'own>,
        arena: &Arena<'own>,
    ) -> &'a mut T::Gc<'a> {
        let _owner = owner;
        arena.write_barrier(self);
        unsafe {
            let ptr = self
                .ptr
                .as_ref()
                .value
                .get()
                .cast::<ManuallyDrop<T::Gc<'a>>>();

            &mut (*ptr)
        }
    }

    pub fn borrow_mut_untraced<'a>(self, owner: &'a mut Owner<'own>) -> &'a mut T::Gc<'a> {
        let _owner = owner;
        assert!(
            !T::needs_trace(),
            "called `borrow_mut_untraced` on a pointer to a type which needs tracing"
        );
        unsafe {
            let ptr = self
                .ptr
                .as_ref()
                .value
                .get()
                .cast::<ManuallyDrop<T::Gc<'a>>>();

            &mut (*ptr)
        }
    }

    pub unsafe fn borrow_mut_no_barrier<'a>(self, owner: &'a mut Owner<'own>) -> &'a mut T::Gc<'a> {
        let _owner = owner;
        let ptr = self
            .ptr
            .as_ref()
            .value
            .get()
            .cast::<ManuallyDrop<T::Gc<'a>>>();

        &mut (*ptr)
    }
}
