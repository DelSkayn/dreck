pub mod cell;
pub use cell::CellOwner;

mod ptr;
pub use ptr::Gc;

pub mod root;
pub use root::{Root, Tracer};

mod impl_trait;

/// Rebind a [`Gc`] object to a [`Root`].
///
/// # Example
/// ```
/// use dreck::{Gc,Root,CellOwner,rebind, root};
///
/// fn use_and_collect<'r,'cell>(
///     owner: &CellOwner<'cell>,
///     root: &'r mut Root<'cell>,
///     ptr: Gc<'r,'cell,u32>) -> Gc<'r,'cell,u32>{
///
///     if *ptr.borrow(owner) == 0{
///         root!(&root,ptr);
///         // `ptr` is now rooted and its lifetime is as long as the current scope.
///         root.collect(owner);
///         // Rebind ptr to root increasing its lifetime to be the same as root.
///         return rebind!(root,ptr);
///     }
///     ptr
/// }
///
/// ```
#[macro_export]
macro_rules! rebind {
    ($root:expr, $value:expr) => {{
        let v = $value;
        unsafe {
            // Detach from arena's lifetime.
            let v = $crate::rebind(v);
            // Ensure that the $arena is an arena.
            // Bind to the lifetime of the arena.
            $crate::Root::rebind_to($root, v)
        }
    }};
}

/// Root a [`Gc`] struct detaching it from their [`Root`]
///
/// It is not possible to run garbage collection as long as there are dangeling Gc pointers which
/// borrow the [`Root`].
///
/// This macro roots a pointer thus detaching it from the [`Root`] and allowing one to run garbage
/// collection.
///
/// # Example
/// ```
/// use dreck::{new_root,rebind, root};
///
/// fn main(){
///     new_root!(owner,root);
///
///     let ptr = root.add(1);
///
///     // Not allowed as ptr is dangling.
///     // root.collect()
///
///     root!(&root,ptr);
///     // ptr is rooted and detached from root allowing garbage collection.
///     root.collect(owner);
///     assert_eq!(*ptr.borrow(owner),1);
/// }
///
/// ```
#[macro_export]
macro_rules! root {
    ($root:expr, $value:ident) => {
        let $value = unsafe { $crate::rebind($value) };
        let __guard = unsafe { $crate::Root::root_gc($root, $value) };
        #[allow(unused_unsafe)]
        let $value = unsafe { $crate::rebind_to(&__guard, $value) };
    };
}

/// Create a new [`CellOwner`] and [`Root`]
///
/// This macro is the only way to safely create a [`Root`] object.
#[macro_export]
macro_rules! new_root {
    ($owner:ident, $root:ident) => {
        $crate::new_cell_owner!($owner);
        let mut $root = unsafe { $crate::Root::new(&$owner) };
    };
}

/// A trait for marking gc live pointers.
///
/// # Safety
///
/// This trait can result to lots of unpredictable undefined behaviour if not implemented correctly
/// as such, one should be very carefull when implementing this trait.
///
/// The implementation must uphold the following guarentees:
/// - `needs_trace` returns true if the type can contain `Gc` pointers.
/// - `trace` marks all pointers contained in the type and calls `trace` all types contained in this type which implement `Trace`.
///
/// # Example
///
/// ```
/// use dreck::{Gc,Trace, Tracer};
/// pub struct Container<'gc, 'cell> {
///     text: Gc<'gc, 'cell, String>,
///     vec: Vec<Gc<'gc,'cell,i32>>
/// }
///
/// unsafe impl<'gc, 'cell> Trace for Container<'gc, 'cell> {
///     fn needs_trace() -> bool
///     where
///         Self: Sized,
///     {
///         true
///     }
///
///     fn trace(&self, trace: Tracer) {
///         // you can also call `self.text.trace(trace)`.
///         trace.mark(self.text);
///         self.vec.trace(trace);
///     }
/// }
/// ```
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

/// Trait which indicates a Gc'd object who's lifetimes can be rebound.
///
/// # Safety
/// Implementor must ensure that the output only changes the `'gc` lifetime.
///
/// # Example
///
/// ```
/// use dreck::{Gc,Rebind};
/// pub struct Container<'gc,'cell>(Gc<'gc,'cell, String>);
///
/// // Change the 'gc lifetime into 'r
/// unsafe impl<'r,'gc,'cell> Rebind<'r> for Container<'gc,'cell>{
///     type Output = Container<'r,'cell>;
/// }
/// ```
pub unsafe trait Rebind<'a> {
    /// The output type which has its `'gc` lifetime changed to the rebind lifetime.
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
/// In the context of the dreck crate, `rebind` can be used to change the `'gc` lifetime of a object
/// to some other `'gc` lifetime; either the lifetime of an other gc'd object or a [`Root`] borrow.
///
/// Rebinding to a borrowed [`Root`] of the same `'cell` lifetime is always safe to do
/// as holding onto an root borrow prevents one from running garbage collection.
/// One should always prefer the safe [`rebind!`] macro for rebinding a value to a [`Root`] borrow.
///
/// It is also safe to rebind a object to the lifetime of a [`root::RootGuard`] which created with the
/// same object if one ensures that [`Root::root_gc`] is called correctly.
/// One should always prefer the safe [`root!`] macro for rooting a gc'd object.
///
/// The final safe thing to do with this function and its only use case is to implement traced
/// collections.
/// If a collection is traced it is safe to rebind objects which are to be contained to the `'gc` lifetime of
/// the collection.
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
