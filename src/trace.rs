use crate::arena::Marker;

/// A trait for a type which can be GC allocated. It essential that this trait is implemented
/// correctly for safe use of this library.
///
/// # Safety
/// TODO
pub unsafe trait Trace<'own> {
    /// The type with a different gc lifetime.
    type Gc<'gc>;

    /// Wether this object can contain other GC pointers and thus needs to be traced.
    ///
    /// It is safe to return true it the implementing object contains no pointers but this function
    /// must never return false if it could contain pointers.
    fn needs_trace() -> bool
    where
        Self: Sized;

    /// Trace the object marking all GC pointers contained in the implementing object.
    fn trace(&self, marker: Marker<'own, '_>);

    /// An object for changing the Gc lifetime of a gc allocated object.
    /// This is essentially [`std::mem::transmute`] but only for a single lifetime.
    unsafe fn rebind<'gc>(self) -> Self::Gc<'gc>
    where
        Self: Sized,
    {
        use std::mem::ManuallyDrop;
        union Transmute<T, U> {
            a: ManuallyDrop<T>,
            b: ManuallyDrop<U>,
        }

        //TODO: compiler error using static assertions?
        assert_eq!(
            std::mem::size_of::<Self>(),
            std::mem::size_of::<Self::Gc<'gc>>(),
            "type `{}` implements rebind but its `Rebound` ({}) is a different size",
            std::any::type_name::<Self>(),
            std::any::type_name::<Self::Gc<'gc>>(),
        );

        ManuallyDrop::into_inner(
            (Transmute {
                a: ManuallyDrop::new(self),
            })
            .b,
        )
    }
}

macro_rules! impl_primitive {
    ($($name:ty),*$(,)*) => {
        $(
            unsafe impl<'own> Trace<'own> for $name {
                type Gc<'gc> = $name;

                fn needs_trace() -> bool
                where
                    Self: Sized{
                    false
                }

                fn trace(&self,_marker: Marker<'own,'_>){}
            }
        )*
    };
}

macro_rules! impl_generic{
    ($name:ident<$($gen:ident),*>) => {
        unsafe impl<'own,$($gen: Trace<'own>,)*>  Trace<'own> for $name<$($gen,)*> {
                type Gc<'gc> = $name<$($gen::Gc<'gc>,)*>;


                fn needs_trace() -> bool
                where
                    Self: Sized{
                    false $(|| $gen::needs_trace())*
                }

                fn trace(&self,marker: Marker<'own,'_>){
                    #[allow(non_snake_case)]
                    for ($($gen,)*) in self.iter(){
                        $($gen.trace(marker);)*
                    }
                }
        }
    };
}

macro_rules! impl_list {
    ($name:ident<$gen:ident>) => {
        unsafe impl<'own, $gen: Trace<'own>> Trace<'own> for $name<$gen> {
            type Gc<'gc> = $name<$gen::Gc<'gc>>;

            fn needs_trace() -> bool
            where
                Self: Sized,
            {
                $gen::needs_trace()
            }

            fn trace(&self, marker: Marker<'own, '_>) {
                for v in self.iter() {
                    v.trace(marker);
                }
            }
        }
    };
}

impl_primitive!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, char, bool, String);

impl_list!(Option<T>);
impl_list!(Vec<T>);

mod collection {
    use super::*;
    use std::collections::*;

    impl_list!(HashSet<K>);
    impl_list!(BTreeSet<K>);
    impl_list!(LinkedList<V>);
    impl_list!(BinaryHeap<V>);
    impl_list!(VecDeque<V>);

    impl_generic!(HashMap<K,V>);
    impl_generic!(BTreeMap<K,V>);
}

unsafe impl<'own, K: Trace<'own>, V: Trace<'own>> Trace<'own> for Result<K, V> {
    type Gc<'gc> = Result<K::Gc<'gc>, V::Gc<'gc>>;

    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        K::needs_trace() || V::needs_trace()
    }

    fn trace(&self, marker: Marker<'own, '_>) {
        match *self {
            Ok(ref x) => x.trace(marker),
            Err(ref x) => x.trace(marker),
        }
    }
}

unsafe impl<'a, 'own, T: Trace<'own>> Trace<'own> for &'a T
where
    for<'gc> T::Gc<'gc>: 'a,
{
    type Gc<'gc> = &'a T::Gc<'gc>;

    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        T::needs_trace()
    }

    fn trace(&self, marker: Marker<'own, '_>) {
        (**self).trace(marker)
    }
}

unsafe impl<'a, 'own, T: Trace<'own>> Trace<'own> for &'a mut T
where
    for<'gc> T::Gc<'gc>: 'a,
{
    type Gc<'gc> = &'a mut T::Gc<'gc>;

    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        T::needs_trace()
    }

    fn trace(&self, marker: Marker<'own, '_>) {
        (**self).trace(marker)
    }
}
