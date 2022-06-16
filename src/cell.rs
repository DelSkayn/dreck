use core::fmt;
use std::{cell::UnsafeCell, marker::PhantomData};

// -- The Guard from generativity crate

#[doc(hidden)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Invariant<'id>(PhantomData<&'id mut &'id fn(&'id ()) -> &'id ()>);

impl<'id> Invariant<'id> {
    #[doc(hidden)]
    pub unsafe fn new() -> Self {
        Invariant(PhantomData)
    }
}

impl<'id> fmt::Debug for Invariant<'id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Id<'id>").finish()
    }
}

/// Create a new cell owner.
#[macro_export]
macro_rules! new_cell_owner {
    ($name:ident) => {
        let tag = unsafe { $crate::cell::Invariant::new() };
        let _cell_owner;
        #[allow(unused_mut)]
        let mut $name = unsafe { $crate::cell::CellOwner::new(tag) };
        {
            if false {
                #[allow(non_camel_case_types)]
                struct new_cell_owner<'id>(&'id $crate::cell::Invariant<'id>);
                impl<'id> ::core::ops::Drop for new_cell_owner<'id> {
                    fn drop(&mut self) {}
                }
                _cell_owner = new_cell_owner(&tag);
            }
        }
    };
}

// -- The lcell from qcell crate

/// The owner of all values in [`LCell`]'s with the same lifetime.
pub struct CellOwner<'rt>(pub(crate) Invariant<'rt>);

impl<'rt> CellOwner<'rt> {
    // Create a new Cell owener
    pub unsafe fn new(id: Invariant<'rt>) -> Self {
        CellOwner(id)
    }

    /// Shared borrow the value in the cell.
    #[inline(always)]
    pub fn borrow<'a, T: ?Sized>(&'a self, cell: &LCell<'rt, T>) -> &'a T {
        unsafe { &(*cell.value.get()) }
    }

    /// Mutable borrow the value in the cell.
    #[inline(always)]
    pub fn borrow_mut<'a, T: ?Sized>(&'a mut self, cell: &LCell<'rt, T>) -> &'a mut T {
        unsafe { &mut (*cell.value.get()) }
    }
}

/// A cell which can be accessed with a [`CellOwner`] and has no borrow checking overhead.
///
/// # Example
///
/// ```
/// # use dreck::{new_cell_owner,cell::LCell};
/// new_cell_owner!(owner);
/// let c1 = LCell::new(1);
/// *c1.borrow_mut(&mut owner) += 1;
/// assert_eq!(*c1.borrow(&owner), 2);
///
/// ```
pub struct LCell<'rt, T: ?Sized> {
    _id: Invariant<'rt>,
    value: UnsafeCell<T>,
}

impl<'rt, T> LCell<'rt, T> {
    #[inline]
    pub fn new(value: T) -> LCell<'rt, T> {
        unsafe {
            LCell {
                _id: Invariant::new(),
                value: UnsafeCell::new(value),
            }
        }
    }
}

impl<'rt, T: ?Sized> LCell<'rt, T> {
    /// Borrow the contained value.
    pub fn borrow<'a>(&'a self, owner: &'a CellOwner<'rt>) -> &'a T {
        owner.borrow(self)
    }

    /// Mutable borrow the contained value.
    pub fn borrow_mut<'a>(&'a self, owner: &'a mut CellOwner<'rt>) -> &'a mut T {
        owner.borrow_mut(self)
    }

    pub fn get(&self) -> *mut T {
        self.value.get()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn cell() {
        new_cell_owner!(owner);
        let c1 = LCell::new(1);
        *c1.borrow_mut(&mut owner) += 1;
        assert_eq!(*c1.borrow(&owner), 2);
    }
}
