use std::{
    cell::Cell,
    marker::PhantomData,
    ptr::{addr_of, NonNull},
};

use crate::{marker::Invariant, Bound, Owner, Root, Trace};

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Status {
    Untraced,
    Marked,
    MarkedWeak,
    Traced,
    Removed,
}

/// The pointer type contained within a [`Gc`] pointer.
pub(crate) type DynGcBoxPtr<'own, 'v> = NonNull<GcBox<'own, dyn Trace<'own> + 'v>>;

#[derive(Clone,Copy)]
pub struct RootPtr<'own>(DynGcBoxPtr<'own,'static>);

/// The pointer type contained within a [`Gc`] pointer.
pub type GcBoxPtr<'own, T> = NonNull<GcBox<'own, T>>;

pub(crate) struct GcBoxHead<'own> {
    pub next: Cell<Option<DynGcBoxPtr<'own, 'static>>>,
    pub color: Cell<Status>,
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

impl<'r, 'gc: 'r, 'own, T: Trace<'own> + Sized> Gc<'gc, 'own, T> {
    pub fn borrow(self, _owner: &'r Owner<'own>) -> &'r T {
        unsafe { &(*self.ptr.as_ptr().cast::<GcBox<T>>()).value }
    }


    pub fn is_removed(self) -> bool{
        unsafe{
            self.ptr.as_ref().head.color.get() == Status::Removed
        }
    }
}

impl<'gc, 'own, T: Trace<'own>> Gc<'gc, 'own, T> {
    pub fn ptr_eq(first: Self, second: Gc<'_, 'own, T>) -> bool {
        std::ptr::eq(first.ptr.as_ptr(), second.ptr.as_ptr())
    }
}

impl<'r, 'gc: 'r, 'own, T> Gc<'gc, 'own, T>
where
    T: Trace<'own> + Bound<'r> + Sized + 'r,
{


    pub fn borrow_mut(self, _owner: &'r mut Owner<'own>, root: &Root<'own>) -> &'r mut T::Rebound {
        root.write_barrier(self);
        unsafe { crate::rebind(&mut (*self.ptr.as_ptr().cast::<GcBox<T>>()).value) }
    }

    pub fn borrow_mut_untraced(self, _owner: &'r mut Owner<'own>) -> &'r mut T::Rebound {
        assert!(
            !T::needs_trace(),
            "called borrow_mut_untraced on pointer which requires tracing ({}::needs_trace() returns true)",
            std::any::type_name::<T>()
        );
        unsafe { crate::rebind(&mut (*self.ptr.as_ptr().cast::<GcBox<T>>()).value) }
    }

    pub unsafe fn unsafe_borrow_mut(self, _owner: &'r mut Owner<'own>) -> &'r mut T::Rebound {
        crate::rebind(&mut (*self.ptr.as_ptr().cast::<GcBox<T>>()).value)
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

    pub unsafe fn into_ptr(this: Self) -> GcBoxPtr<'own, T> {
        this.ptr
    }

    pub unsafe fn from_ptr(ptr: GcBoxPtr<'own, T>) -> Gc<'gc, 'own, T> {
        Gc {
            ptr,
            _gc: PhantomData,
            _own: Invariant::new(),
        }
    }
}

// TODO: Possibly remove trace requirement on WeakGc
pub struct WeakGc<'gc,'own, T: Trace<'own>>(Gc<'gc,'own,T>);

impl<'gc,'own,T: Trace<'own>> Copy for WeakGc<'gc,'own,T>{
}

impl<'gc,'own,T: Trace<'own>> Clone for WeakGc<'gc,'own,T>{
    fn clone(&self) -> Self {
        *self
    }
}

impl<'r, 'gc: 'r, 'own, T: Trace<'own> + Sized> WeakGc<'gc, 'own, T> {
    pub fn new(gc: Gc<'gc,'own,T>) -> Self{
        WeakGc(gc)
    }

    pub fn is_removed(self) -> bool{
        self.0.is_removed()
    }

    pub fn borrow(self, owner: &'r Owner<'own>) -> Option<&'r T> {
        if self.is_removed(){
            None
        }else{
            Some(self.0.borrow(owner))
        }
    }

    pub fn upgrade(ptr: Self) -> Result<Gc<'gc,'own,T>,WeakGc<'gc,'own,T>>{
        if ptr.is_removed(){
            Err(ptr)
        }else{
            Ok(ptr.0)
        }
    }

}

impl<'r, 'gc: 'r, 'own, T> WeakGc<'gc, 'own, T>
where
    T: Trace<'own> + Bound<'r> + Sized + 'r,
{
    pub fn borrow_mut(self, owner: &'r mut Owner<'own> ) -> Option<&'r mut T::Rebound> {
        // No need to write barrier because a weak pointer does not keep referenced pointers alive.
        unsafe{
            if self.is_removed(){
                None
            }else{
                Some(self.0.unsafe_borrow_mut(owner))
            }
        }
    }
}

unsafe impl<'gc,'own, T: Trace<'own>> Trace<'own> for WeakGc<'gc,'own,T>{
    fn needs_trace() -> bool
    where
            Self: Sized {
        true
    }

    fn trace<'a>(&self, tracer: crate::Tracer<'a, 'own>) {
        unsafe{
            tracer.mark_weak(self.0)
        }
    }
}

unsafe impl<'from,'to,'own, T> Bound<'to> for WeakGc<'from,'own,T>
where T: Bound<'to> + Trace<'own>,
      T::Rebound: Trace<'own>
{
    type Rebound = WeakGc<'to,'own,T::Rebound>;
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
