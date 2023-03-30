#![allow(clippy::missing_safety_doc)]

pub mod marker;
pub use marker::{Invariant, Owner};

mod arena;
pub use arena::{Arena, Marker, RootGuard};

mod ptr;
pub use ptr::Gc;

mod trace;
pub use trace::Trace;

pub mod sys;

pub mod scoped;

/// Create a new safe arena and owner.
///
/// # Usage
/// ```
/// # use dreck::*;
/// dreck!(owner,arena);
///
/// let ptr = arena.add(3);
/// assert_eq!(*ptr.borrow(&owner),3)
/// ```
#[macro_export]
macro_rules! dreck {
    ($owner:ident,$arena:ident) => {
        let _pin = ();
        let invariant = $crate::marker::Invariant::new_ref(&_pin);
        let _lifetime_constrainer;
        if false {
            struct KeepTillScopeDrop<'a, 'inv>(&'a $crate::marker::Invariant<'inv>);
            impl<'a, 'inv> Drop for KeepTillScopeDrop<'a, 'inv> {
                fn drop(&mut self) {}
            }
            _lifetime_constrainer = KeepTillScopeDrop(&invariant);
        }

        let (mut $owner, mut $arena) = unsafe {
            let owner = $crate::Owner::from_invariant(invariant);
            let arena = $crate::Arena::new(&owner);
            (owner, arena)
        };
    };
}

/// Rebind a GC pointer back to a arena.
///
/// # Usage
/// ```
/// # use std::pin::pin;
/// # use dreck::*;
/// dreck!(owner,arena);
///
/// let ptr = arena.add(3);
/// let ptr = {
///     let guard = pin!(RootGuard::new());
///     let ptr = root!(&arena,guard,ptr);
///
///     arena.collect(&owner);
///     rebind!(&arena,ptr)
///     // Guard dropped here. which would also drop the pointer without rebinding.
/// };
///
/// assert_eq!(*ptr.borrow(&owner),3)
#[macro_export]
macro_rules! rebind {
    ($arena:expr,$value:expr) => {{
        let value = unsafe {
            // detach from any existing lifetime
            $value.rebind()
        };
        $crate::Arena::rebind_to($arena, value)
    }};
}

/// Root a GC pointer to be kept alive for the duration of the given guard.
///
/// # Usage
/// ```
/// # use std::pin::pin;
/// # use dreck::*;
/// dreck!(owner,arena);
///
/// let ptr = arena.add(3);
/// let guard = pin!(RootGuard::new());
/// let ptr = root!(&arena,guard,ptr);
///
/// arena.collect(&owner);
///
/// assert_eq!(*ptr.borrow(&owner),3)
#[macro_export]
macro_rules! root {
    ($arena:expr,$guard:expr,$value:expr) => {{
        let value = unsafe { Trace::rebind($value) };
        $crate::Arena::root($arena, value, $guard)
    }};
}
