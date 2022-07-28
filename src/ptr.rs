use std::{
    cell::Cell,
    marker::PhantomData,
    ptr::{addr_of, NonNull},
};

use crate::{marker::Invariant, Owner, Root, Trace};

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Color {
    White,
    Gray,
    Black,
}

/// The pointer type contained within a [`Gc`] pointer.
pub(crate) type DynGcBoxPtr<'own, 'v> = NonNull<GcBox<'own, dyn Trace<'own> + 'v>>;

/// The pointer type contained within a [`Gc`] pointer.
pub type GcBoxPtr<'own,T> = NonNull<GcBox<'own, T>>;



pub(crate) struct GcBoxHead<'own> {
    pub next: Cell<Option<DynGcBoxPtr<'own, 'static>>>,
    pub color: Cell<Color>,
}

/// The structure containing a gc allocated value.
#[repr(C)]
pub struct GcBox<'own, T: ?Sized> {
    pub(crate) head: GcBoxHead<'own>,
    pub(crate) value: T,
}

/// A pointer to a gc allocated value.
pub struct Gc<'gc, 'own, T: Trace<'own> + ?Sized> {
    // Point to the GcBoxHead instead of GcBox, so T is hidden, to avoid an invariant 'gc
    pub(crate) ptr: NonNull<GcBox<'own, T>>,
    pub(crate) _gc: PhantomData<&'gc ()>,
    pub(crate) _own: Invariant<'own>,
}

impl<'gc, 'own, T: Trace<'own> + ?Sized> Copy for Gc<'gc, 'own, T> {}

impl<'gc, 'own, T: Trace<'own> + ?Sized> Clone for Gc<'gc, 'own, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'gc, 'own, T: Trace<'own> + Sized> Gc<'gc, 'own, T> {
    pub fn borrow<'r>(self, _owner: &'r Owner<'own>) -> &'r T {
        unsafe { &(*self.ptr.as_ptr().cast::<GcBox<T>>()).value }
    }

    pub fn borrow_mut<'r>(self, _owner: &'r mut Owner<'own>, root: &Root<'own>) -> &'r mut T {
        root.write_barrier(self);
        unsafe { &mut (*self.ptr.as_ptr().cast::<GcBox<T>>()).value }
    }

    pub fn borrow_mut_untraced<'r>(self, _owner: &'r mut Owner<'own>) -> &'r mut T {
        assert!(
            !T::needs_trace(),
            "called borrow_mut_untraced on pointer which requires tracing ({}::needs_trace() returns true)",
            std::any::type_name::<T>()
        );
        unsafe { &mut (*self.ptr.as_ptr().cast::<GcBox<T>>()).value }
    }

    pub fn as_raw(this: Gc<'gc, 'own, T>) -> *const T {
        let ptr: *mut GcBox<T> = this.ptr.as_ptr().cast();

        unsafe { addr_of!((*ptr).value) }
    }

    pub unsafe fn from_raw(ptr: *const T) -> Self {
        let offset = value_field_offset::<T>();

        let gc_ptr = (ptr as *mut u8).offset(-offset).cast::<GcBox<T>>();

        Gc {
            ptr: NonNull::new_unchecked(gc_ptr),
            _gc: PhantomData,
            _own: Invariant::new(),
        }
    }

    pub unsafe fn into_ptr(this: Self) -> GcBoxPtr<'own,T> {
        this.ptr
    }

    pub unsafe fn from_ptr(ptr: GcBoxPtr<'own,T>) -> Gc<'gc, 'own, T> {
        Gc {
            ptr,
            _gc: PhantomData,
            _own: Invariant::new(),
        }
    }
}

#[inline]
fn value_field_offset<T: Sized>() -> isize {
    let uninit = std::mem::MaybeUninit::<GcBox<T>>::uninit();
    let field_ptr = unsafe { addr_of!((*uninit.as_ptr()).value) };
    unsafe {
        field_ptr
            .cast::<u8>()
            .offset_from(uninit.as_ptr().cast::<u8>())
    }
}
