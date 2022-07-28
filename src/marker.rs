//! Marker types used by dreck.

use core::marker::PhantomData;

/// A struct which allows marking a lifetime as invariant.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Invariant<'inv>(PhantomData<&'inv mut &'inv fn(&'inv ()) -> &'inv ()>);

impl<'inv> Invariant<'inv> {
    pub fn new() -> Self {
        Invariant(PhantomData)
    }
}
