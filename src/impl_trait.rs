use super::{Bound, Gc, Trace, Tracer};
macro_rules! impl_trace_primitive{
($($ty:ident,)*) => {
    $(
        unsafe impl<'own> Trace<'own> for $ty{
            fn needs_trace() -> bool{
                false
            }

            fn trace(&self, _t: Tracer){}
        }

        unsafe impl<'to> Bound<'to> for $ty {
            type Rebound = $ty;
        }
    )*
};
}

impl_trace_primitive!(
    bool, char, u8, u16, u32, u64, usize, i8, i16, i32, i64, f32, f64, isize, String,
);

macro_rules! impl_trace_tuple{
    ($($ty:ident),*) => {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            unsafe impl<'own,$($ty:Trace<'own>,)*> Trace<'own> for ($($ty,)*){
                fn needs_trace() -> bool{
                    false $(|| $ty::needs_trace())*
                }

                fn trace<'a>(&self, t: Tracer<'a,'own>){
                    let ($(ref $ty,)*) = self;
                    $($ty.trace(t);)*
                }
            }

            unsafe impl<'to,$($ty: Bound<'to>,)*> Bound<'to> for ($($ty,)*){
                type Rebound = ($($ty::Rebound,)*);
            }
    };
}

impl_trace_tuple!();
impl_trace_tuple!(A);
impl_trace_tuple!(A, B);
impl_trace_tuple!(A, B, C);
impl_trace_tuple!(A, B, C, D);
impl_trace_tuple!(A, B, C, D, E);
impl_trace_tuple!(A, B, C, D, E, F);
impl_trace_tuple!(A, B, C, D, E, F, G);
impl_trace_tuple!(A, B, C, D, E, F, G, H);

unsafe impl<'gc, 'own, T: Trace<'own>> Trace<'own> for Gc<'gc, 'own, T> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }

    fn trace<'a>(&self, t: Tracer<'a, 'own>) {
        t.mark(*self)
    }
}

unsafe impl<'to, 'from, 'own, T> Bound<'to> for Gc<'from, 'own, T>
where
    T: Trace<'own> + Bound<'to>,
    T::Rebound: Trace<'own> + 'to,
{
    type Rebound = Gc<'to, 'own, T::Rebound>;
}

unsafe impl<'own, T: Trace<'own>> Trace<'own> for Vec<T> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        T::needs_trace()
    }

    fn trace<'a>(&self, t: Tracer<'a, 'own>) {
        for v in self {
            v.trace(t);
        }
    }
}

unsafe impl<'to, T: Bound<'to>> Bound<'to> for Vec<T> {
    type Rebound = Vec<T::Rebound>;
}

unsafe impl<'own, T: Trace<'own>> Trace<'own> for Option<T> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        T::needs_trace()
    }

    fn trace(&self, t: Tracer<'_, 'own>) {
        if let Some(v) = self.as_ref() {
            v.trace(t);
        }
    }
}

unsafe impl<'to, T: Bound<'to>> Bound<'to> for Option<T> {
    type Rebound = Option<T::Rebound>;
}

unsafe impl<'own, R: Trace<'own>, E: Trace<'own>> Trace<'own> for Result<R, E> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        R::needs_trace() || E::needs_trace()
    }

    fn trace(&self, t: Tracer<'_, 'own>) {
        match *self {
            Ok(ref r) => r.trace(t),
            Err(ref e) => e.trace(t),
        }
    }
}

unsafe impl<'to, R: Bound<'to>, E: Bound<'to>> Bound<'to> for Result<R, E> {
    type Rebound = Result<R::Rebound, E::Rebound>;
}

unsafe impl<'a, 'to, T, R> Bound<'to> for &'a mut T
where
    T: Bound<'to, Rebound = R>,
    R: 'a,
{
    type Rebound = &'a mut R;
}
