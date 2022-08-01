mod impl_trait;
pub mod marker;

mod root;
use marker::Invariant;
use ptr::RootPtr;
pub use root::{Root, RootGuard, Tracer};

mod ptr;
pub use ptr::{Gc,WeakGc};

/// Rebind a [`Gc`] object to a [`Root`].
///
/// Gc pointers lifetime can be freely rebound to the lifetime of the arena at any time.
/// This macro has two primary uses:
///
///
/// The first is to rebind a `Gc` pointer returned a function which takes a mutable reference
/// to the arena. A pointer returned by such a function is bound to a mutable arena which prevents
/// one from allocating new bc pointer for as long as the returned value lives. By rebinding
/// the returned `Gc` pointer the pointer will be bound to a immutable borrow which does allow
/// allocating new values.
///
/// The second use is to rebind a `Gc` pointer from a rooted pointer. Once a pointer is rooted the
/// pointer will only remain alive for the duration of the scope in which the pointer is rooted if
/// the pointer needs to escape that lifetime rebinding the rooted pointer again to the arena
/// allows it to life for the lifetime of the arena.
///
///
/// # Example
/// ```
/// use dreck::{Gc,Root,Owner,rebind, root};
///
/// fn use_and_collect<'r,'own>(
///     owner: &Owner<'own>,
///     root: &'r mut Root<'own>,
///     ptr: Gc<'r,'own,u32>) -> Gc<'r,'own,u32>{
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

#[macro_export]
macro_rules! rebind_try {
    ($root:expr, $value:expr) => {{
        let v: Result<_, _> = $value;
        match v {
            Ok(r) => $crate::rebind!($root, r),
            Err(e) => {
                return Err($crate::rebind!($root, e).into());
            }
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
/// new_root!(owner,root);
///
/// let ptr = root.add(1);
///
/// // Not allowed as ptr is dangling.
/// // root.collect()
///
/// root!(&root,ptr);
/// // ptr is rooted and detached from the root borrow allowing garbage collection.
/// root.collect(owner);
/// assert_eq!(*ptr.borrow(owner),1);
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

/// Create a new [`Owner`] and [`Root`]
///
/// This macro is the only way to safely create a [`Root`] object.
#[macro_export]
macro_rules! new_root {
    ($owner:ident, $root:ident) => {
        let tag = unsafe { $crate::marker::Invariant::new() };
        let _cell_owner;
        #[allow(unused_mut)]
        let mut $owner = unsafe { &mut $crate::Owner::new(tag) };
        {
            if false {
                #[allow(non_camel_case_types)]
                struct new_cell_owner<'id>(&'id $crate::marker::Invariant<'id>);
                impl<'id> ::core::ops::Drop for new_cell_owner<'id> {
                    fn drop(&mut self) {}
                }
                _cell_owner = new_cell_owner(&tag);
            }
        }

        let mut __root = unsafe { $crate::Root::new(&$owner) };
        let $root = &mut __root;
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
/// pub struct Container<'gc, 'own> {
///     text: Gc<'gc, 'own, String>,
///     vec: Vec<Gc<'gc,'own,i32>>
/// }
///
/// unsafe impl<'gc, 'own> Trace<'own> for Container<'gc, 'own> {
///     fn needs_trace() -> bool
///     where
///         Self: Sized,
///     {
///         true
///     }
///
///     fn trace<'a>(&self, trace: Tracer<'a,'own>) {
///         // you can also call `self.text.trace(trace)`.
///         trace.mark(self.text);
///         self.vec.trace(trace);
///     }
/// }
/// ```
pub unsafe trait Trace<'own> {
    fn needs_trace() -> bool
    where
        Self: Sized;

    fn trace<'a>(&self, tracer: Tracer<'a, 'own>);
}

/// Owner of the [`Gc`] pointers.
///
/// A borrow to this struct is required to access values contained in Gc pointers.
pub struct Owner<'own>(pub(crate) marker::Invariant<'own>);

impl<'own> Owner<'own> {
    pub unsafe fn new(invariant: Invariant<'own>) -> Self {
        Owner(invariant)
    }
}

/// Trait which indicates a Gc'd object who's lifetimes can be rebound.
///
/// # Safety
/// Implementor must ensure that the associated types only change the approriate lifetimes:
/// `Bound::Rebound` only changes the 'gc.
///
/// # Example
///
/// ```
/// use dreck::{Gc,Bound};
/// pub struct Container<'gc,'own>(Gc<'gc,'own, String>);
///
/// unsafe impl<'from,'own,'to> Bound<'to> for Container<'from,'own>{
///     // Change the 'from lifetime into 'to
///     type Rebound = Container<'to,'own>;
/// }
/// ```
pub unsafe trait Bound<'gc> {
    type Rebound;
}

pub trait Rootable<'gc>{
    fn into_root(&self) -> RootPtr<'gc>;
}

/// Rebind a value to the lifetime of a given borrow.
///
/// # Safety
///
/// See [`rebind()`]
#[inline(always)] // this should compile down to nothing
pub unsafe fn rebind_to<'to, R, T>(_: &'to R, v: T) -> T::Rebound
where
    T: Bound<'to>,
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
/// Rebinding to a borrowed [`Root`] of the same `'own` lifetime is always safe to do
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
///
/// # Panics
///
/// This function will panic if the [`Bound`] trait is implemented in correctly and `T` and
/// `T::Rebound` have different sizes.
#[inline(always)] // this should compile down to nothing
pub unsafe fn rebind<'r, T>(v: T) -> T::Rebound
where
    T: Bound<'r>,
{
    use std::mem::ManuallyDrop;
    union Transmute<T, U> {
        a: ManuallyDrop<T>,
        b: ManuallyDrop<U>,
    }

    //TODO: compiler error using static assertions?
    assert_eq!(
        std::mem::size_of::<T>(),
        std::mem::size_of::<T::Rebound>(),
        "type `{}` implements rebind but its `Reboud` ({}) is a different size",
        std::any::type_name::<T>(),
        std::any::type_name::<T::Rebound>(),
    );

    ManuallyDrop::into_inner(
        (Transmute {
            a: ManuallyDrop::new(v),
        })
        .b,
    )
}
