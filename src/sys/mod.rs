//! The unsafe implementation used to implement the safe API.

mod arena;
pub use arena::*;

mod ptr;
pub use ptr::*;

use crate::{arena::Marker, Trace};

/// The lifetime erased version of [`Trace`] used in the unsafe API.
///
/// Automatically implemented for any type that implements [`Trace`].
pub unsafe trait UnsafeTrace {
    /// Wether this object can contain other GC pointers and thus needs to be traced.
    ///
    /// It is safe to return true it the implementing object contains no pointers but this function
    /// must never return false if it could contain pointers.
    fn needs_trace() -> bool
    where
        Self: Sized;

    /// Trace the object marking all GC pointers contained in the implementing object.
    fn trace(&self, marker: UnsafeMarker);
}

unsafe impl<'own, T: Trace<'own>> UnsafeTrace for T {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        <Self as Trace<'own>>::needs_trace()
    }

    fn trace(&self, marker: UnsafeMarker) {
        <Self as Trace<'own>>::trace(self, unsafe { Marker::from_unsafe(marker) })
    }
}
