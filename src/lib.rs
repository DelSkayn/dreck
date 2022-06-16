pub mod cell;
pub use cell::CellOwner;

mod ptr;
pub use ptr::Gc;

mod root;
pub use root::{Root, Tracer};

mod impl_trait;

#[macro_export]
macro_rules! rebind {
    ($arena:expr, $value:expr) => {{
        let v = $value;
        unsafe {
            // Detach from arena's lifetime.
            let v = $crate::gc::rebind(v);
            // Ensure that the $arena is an arena.
            let a: &$crate::gc::Arena = $arena;
            // Bind to the lifetime of the arena.
            $crate::gc::rebind_to(a, v)
        }
    }};
}

#[macro_export]
macro_rules! root {
    ($arena:expr, $value:ident) => {
        let $value = unsafe { $crate::gc::rebind($value) };
        let __guard = $arena._root_gc($value);
        #[allow(unused_unsafe)]
        let $value = unsafe { $crate::gc::rebind_to(&__guard, $value) };
    };
}

/// A trait for marking gc live pointers.
///
/// # Safety
///
/// This trait is very unsafe and will cause UB if not correctly implemented, as such, one should be very carefull to
/// implement this trait correctly.
///
/// The implementation must uphold the following guarentees.
///
/// - `needs_trace` returns true if the type can contain `Gc` pointers.
/// - `trace` marks all pointers contained in the type and calls `trace` all types contained in this type which implement `Trace`.
/// - The type must not dereferences a `Gc` pointer in its drop implementation.
///
pub unsafe trait Trace {
    /// Returns whether the type can contain any `Gc` pointers and needs to be traced.
    ///
    /// The value returned is used as an optimization.
    /// Returning true when a type cannot contain any `Gc` pointers is completely safe.
    fn needs_trace() -> bool
    where
        Self: Sized;

    /// Traces the type for any gc pointers.
    fn trace(&self, trace: Tracer);
}

/// Gc values who's lifetimes can be rebind
///
/// # Safety
///
/// Implementor must ensure that the output only changes the lifetime which signifies the gc
/// lifetime.
pub unsafe trait Rebind<'a> {
    type Output;
}

/// Rebind a value to the lifetime of a given borrow.
///
/// # Safety
///
/// See [`rebind()`]
#[inline(always)] // this should compile down to nothing
pub unsafe fn rebind_to<'rt, R, T>(_: &'rt R, v: T) -> T::Output
where
    T: Rebind<'rt>,
{
    rebind(v)
}

/// Rebinds a value to a arbitray lifetime.
///
/// # Safety
///
/// This method is wildly unsafe if not used correctly.
/// Rebinding is essentially a somewhat more restrictive [`std::mem::transmute`] and should be treated as
/// such.
///
/// In the context of the garbage collections rebind can be used to change the lifetime of a gc value
/// to some other gc value or to the lifetime of a borrowed arena.
///
/// Rebinding to a borrowed arena is always safe to do as holding onto an arena borrow prevents on
/// from running garbage collection. While possible to do with this function prefer the [`rebind!`]
///
/// The primary used for this macro is to change the lifetime of a gc'd value when inserting in a
/// traced collection.
///
#[inline(always)] // this should compile down to nothing
pub unsafe fn rebind<'rt, T>(v: T) -> T::Output
where
    T: Rebind<'rt>,
{
    use std::mem::ManuallyDrop;

    //TODO: compiler error using static assertions?
    if std::mem::size_of::<T>() != std::mem::size_of::<T::Output>() {
        panic!(
            "type `{}` implements rebind but its `Output` is a different size. `{}` is {} bytes in size but `{}` is {} bytes",
            std::any::type_name::<T>(),
            std::any::type_name::<T>(),
            std::mem::size_of::<T>(),
            std::any::type_name::<T::Output>(),
            std::mem::size_of::<T::Output>(),
        );
    }
    union Transmute<T, U> {
        a: ManuallyDrop<T>,
        b: ManuallyDrop<U>,
    }

    ManuallyDrop::into_inner(
        (Transmute {
            a: ManuallyDrop::new(v),
        })
        .b,
    )
}
