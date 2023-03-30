use std::pin::Pin;

use crate::{
    marker::{Invariant, Owner},
    sys::{UnsafeArena, UnsafeMarker, UnsafeRootGuard},
    Gc, Trace,
};

/// The marker passed to the [`Trace::trace`] method for marking GC pointers.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Marker<'own, 'a> {
    marker: UnsafeMarker<'a>,
    _invariant: Invariant<'own>,
}

impl<'own, 'a> Marker<'own, 'a> {
    /// Mark a Gc pointer.
    pub fn mark<T: Trace<'own>>(self, ptr: Gc<'_, 'own, T>) {
        unsafe { self.marker.mark(Gc::into_gc_box(ptr)) }
    }

    /// Create the marker from the unsafe variant.
    pub unsafe fn from_unsafe(marker: UnsafeMarker<'a>) -> Self {
        Self {
            marker,
            _invariant: Invariant::new(),
        }
    }
}

// Must remain repr(transparent) to allow safe transmute
/// A root guard for rooting pointers.
#[repr(transparent)]
pub struct RootGuard(UnsafeRootGuard);

impl RootGuard {
    pub fn new() -> Self {
        Self(UnsafeRootGuard::new())
    }
}

impl Default for RootGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// The arena for garbage collected pointers.
/// This struct is in charge allocating, freeing, and rooting garbage collected pointers.
#[repr(transparent)]
pub struct Arena<'own> {
    arena: UnsafeArena,
    _invariant: Invariant<'own>,
}

impl<'own> Arena<'own> {
    pub unsafe fn new(_owner: &Owner<'own>) -> Self {
        Arena {
            arena: UnsafeArena::new(),
            _invariant: Invariant::new(),
        }
    }

    pub fn add<'gc, T: Trace<'own>>(&'gc self, value: T) -> Gc<'gc, 'own, T> {
        unsafe {
            let ptr = self.arena.add(value);
            Gc::from_gc_box(ptr)
        }
    }

    // Takes an immutable reference to owner so you cant move an pointer out a container and then
    // collect and then reference the container.
    pub fn collect(&mut self, owner: &Owner<'own>) {
        let _owner = owner;
        unsafe {
            self.arena.collect();
        }
    }

    // Takes an immutable reference to owner so you cant move an pointer out a container and then
    // collect and then reference the container.
    pub fn collect_full(&mut self, owner: &Owner<'own>) {
        let _owner = owner;
        unsafe {
            self.arena.collect_full();
        }
    }

    pub fn root<'r, T: Trace<'own>>(
        &self,
        value: Gc<'_, 'own, T>,
        guard: Pin<&'r mut RootGuard>,
    ) -> Gc<'r, 'own, T::Gc<'r>> {
        unsafe {
            self.arena
                .root(std::mem::transmute(guard), Gc::into_gc_box(value));

            value.rebind()
        }
    }

    pub fn rebind_to<'gc, T: Trace<'own>>(&'gc self, value: T) -> T::Gc<'gc> {
        unsafe { value.rebind() }
    }

    pub fn write_barrier<T: Trace<'own>>(&self, ptr: Gc<'_, 'own, T>) {
        if !T::needs_trace() {
            return;
        }
        unsafe { self.arena.write_barrier(Gc::into_gc_box(ptr)) }
    }

    pub fn into_unsafe_arena(self) -> UnsafeArena {
        self.arena
    }

    pub fn unsafe_arena(&self) -> &UnsafeArena {
        &self.arena
    }

    pub fn unsafe_arena_mut(&mut self) -> &mut UnsafeArena {
        &mut self.arena
    }

    pub unsafe fn from_unsafe(arena: UnsafeArena) -> Self {
        Arena {
            arena,
            _invariant: Invariant::new(),
        }
    }

    pub unsafe fn from_unsafe_ref(arena: &UnsafeArena) -> &Self {
        // Safe because arena is transparent over unsafe arean
        std::mem::transmute(arena)
    }

    pub unsafe fn from_unsafe_mut(arena: &mut UnsafeArena) -> &mut Self {
        // Safe because arena is transparent over unsafe arean
        std::mem::transmute(arena)
    }
}
