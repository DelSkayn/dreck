use std::{
    alloc,
    cell::{Cell, RefCell},
    marker::PhantomData,
    mem,
    ptr::NonNull,
};

use crate::{
    marker::Invariant,
    ptr::{Color, GcBox, GcBoxHead, DynGcBoxPtr},
    Bound, Gc, Owner, Trace,
};

/// Object passed to [`Trace::trace`] method, used to mark pointers.
#[derive(Clone, Copy)]
pub struct Tracer<'gc, 'own>(&'gc Root<'own>);

impl<'gc, 'own> Tracer<'gc, 'own> {
    /// Mark a pointer as alive.
    pub fn mark<T: Trace<'own>>(self, ptr: Gc<'_, 'own, T>) {
        unsafe {
            if ptr.ptr.as_ref().head.color.get() != Color::White {
                return;
            }
            ptr.ptr.as_ref().head.color.set(Color::Gray);

            if T::needs_trace() {
                let ptr: DynGcBoxPtr<'own, '_> = ptr.ptr.cast::<GcBox<'own, T>>();
                let ptr: DynGcBoxPtr<'own, 'static> = mem::transmute(ptr);
                self.0.grays.borrow_mut().push(ptr);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Sleep,
    Wake,
    Mark,
    Sweep,
}

/// A guard which will keep a value on the stack rooted for as long as it is alive.
///
/// This struct should not be used in safe code.
pub struct RootGuard<'own>(*const Root<'own>);

impl<'own> RootGuard<'own> {
    /// Rebind a value to the lifetime of the root guard.
    /// # Safety
    /// This method should only ever be called with the object this root guard was created in
    /// [`Root::root_gc`].
    pub unsafe fn bind<R: Bound<'own>>(&self, r: R) -> R::Rebound {
        crate::rebind(r)
    }
}

impl<'cell> Drop for RootGuard<'cell> {
    fn drop(&mut self) {
        unsafe {
            (*self.0).roots.borrow_mut().pop();
        }
    }
}

/// Root of the dreck GC, used for allocating [`Gc`] pointers and run collection.
pub struct Root<'own> {
    roots: RefCell<Vec<DynGcBoxPtr<'own, 'static>>>,

    grays: RefCell<Vec<DynGcBoxPtr<'own, 'static>>>,
    grays_again: RefCell<Vec<DynGcBoxPtr<'own, 'static>>>,

    sweep: Option<DynGcBoxPtr<'own, 'static>>,
    sweep_prev: Cell<Option<DynGcBoxPtr<'own, 'static>>>,

    all: Cell<Option<DynGcBoxPtr<'own, 'static>>>,

    total_allocated: Cell<usize>,
    remembered_size: usize,
    wakeup_total: usize,
    allocation_debt: Cell<f64>,

    phase: Cell<Phase>,

    _own: Invariant<'own>,
}

impl<'own> Root<'own> {
    const PAUSE_FACTOR: f64 = 0.5;
    const TIMING_FACTOR: f64 = 1.5;
    const MIN_SLEEP: usize = 4096;

    /// Create a new `Root`.
    ///
    /// This function is unsafe as creating two roots with the same `'own` lifetime is unsound. Prefer 
    /// the use of the [`new_root!`](crate::new_root!) macro.
    ///
    /// # Safety
    /// The `'own` lifetime must be different from all other [`Root`] objects created.
    pub unsafe fn new(owner: &Owner<'own>) -> Self {
        Root {
            roots: RefCell::new(Vec::new()),

            grays: RefCell::new(Vec::new()),
            grays_again: RefCell::new(Vec::new()),

            sweep: None,
            sweep_prev: Cell::new(None),

            all: Cell::new(None),

            total_allocated: Cell::new(0),
            remembered_size: 0,
            wakeup_total: Self::MIN_SLEEP,
            allocation_debt: Cell::new(0.0),

            phase: Cell::new(Phase::Sleep),
            _own: owner.0,
        }
    }

    pub(crate) unsafe fn add_raw<T: Trace<'own>>(&self, v: T) -> NonNull<GcBox<'own, T>> {
        let layout = alloc::Layout::new::<GcBox<T>>();
        let ptr = NonNull::new(alloc::alloc(layout).cast::<GcBox<T>>()).unwrap();

        ptr.as_ptr().write(GcBox {
            head: GcBoxHead {
                color: Cell::new(Color::White),
                next: Cell::new(self.all.get()),
            },
            value: v,
        });

        self.total_allocated
            .set(self.total_allocated.get() + layout.size());

        if self.phase.get() == Phase::Sleep && self.total_allocated.get() > self.wakeup_total {
            self.phase.set(Phase::Wake);
        }

        if self.phase.get() != Phase::Sleep {
            self.allocation_debt.set(
                self.allocation_debt.get()
                    + layout.size() as f64
                    + layout.size() as f64 / Self::TIMING_FACTOR,
            );
        }

        let dyn_ptr: DynGcBoxPtr = ptr;
        self.all.set(Some(mem::transmute(dyn_ptr)));

        if self.phase.get() == Phase::Sweep && self.sweep_prev.get().is_none() {
            self.sweep_prev.set(self.all.get());
        }

        ptr
    }


    /// Allocated a value as a garbage collected pointer.
    #[must_use]
    pub fn add<'gc, T, R>(&'gc self, v: T) -> Gc<'gc, 'own, R>
    where
        T: Bound<'gc, Rebound = R>,
        R: Trace<'own>,
    {
        unsafe {
            let ptr = self.add_raw(crate::rebind(v));
            Gc {
                ptr,
                _gc: PhantomData,
                _own: Invariant::new(),
            }
        }
    }

    /// Indicate a point at which garbage collection can run.
    ///
    /// The GC will only run if enough objects have been allocated.
    /// As the GC is incremental it will also only run only a part of the collection cycle.
    pub fn collect(&mut self, _owner: &Owner<'own>) {
        unsafe { self.inner_collect() };
    }


    /// Run a full cycle of the garbage collection.
    ///
    /// Unlike [`Root::collect`] this method will allways collect all unreachable Gc'd objects.
    pub fn collect_full(&mut self, _owner: &Owner<'own>) {
        self.allocation_debt.set(f64::INFINITY);
        self.phase.set(Phase::Wake);
        unsafe { self.inner_collect() };
    }

    /// Mark a pointer value as possibly containing new [`Gc`] pointers.
    ///
    /// In safe code you should never have to call this method as the [`Gc`] struct will manage
    /// write barriers for you.
    ///
    /// If a type has an unsafe trace implementation and could ever contain new GC'd values within
    /// itself, One must call this function on objects of that type before running collection, everytime that object could
    /// possibly contain new GC'd values.
    #[inline]
    pub fn write_barrier<T: Trace<'own>>(&self, gc: Gc<'_, 'own, T>) {
        if !T::needs_trace() {
            return;
        }
        unsafe {
            if self.phase.get() == Phase::Mark && gc.ptr.as_ref().head.color.get() == Color::Black {
                gc.ptr.as_ref().head.color.set(Color::Gray);
                let ptr: DynGcBoxPtr<'own, '_> = gc.ptr.cast::<GcBox<T>>();
                let ptr: DynGcBoxPtr<'own, 'static> = mem::transmute(ptr);
                self.grays_again.borrow_mut().push(ptr);
            }
        }
    }

    /// Rebind a pointer to the lifetime of this root guard.
    ///
    /// On should prefer the [`rebind!`](crate::rebind!) macro instead of this function as it is more permissive
    /// with which pointers it allows rebinding.
    pub fn rebind_to<'a, T: Trace<'own> + Bound<'a> + 'a>(
        &'a self,
        t: Gc<'_, 'own, T>,
    ) -> Gc<'a, 'own, T::Rebound>
    where
        T::Rebound: Trace<'own>,
    {
        unsafe { crate::rebind(t) }
    }

    /// Root a gc pointer for the duration of root guard's lifetime.
    /// Prefer the use of the [`root!`](crate::root!) macro.
    ///
    /// # Safety
    /// - The `Root` object must outlife the returned `RootGuard`
    /// - All `RootGuard`'s must be dropped in the reverse order of which they where created.
    pub unsafe fn root_gc<T: Trace<'own>>(&self, t: Gc<'_, 'own, T>) -> RootGuard<'own> {
        let ptr: DynGcBoxPtr<'own, '_> = t.ptr.cast::<GcBox<T>>();
        let ptr: DynGcBoxPtr<'own, 'static> = std::mem::transmute(ptr);
        self.roots.borrow_mut().push(ptr);
        RootGuard(self)
    }

    unsafe fn inner_collect(&mut self) {
        if self.phase.get() == Phase::Sleep {
            return;
        }

        let work = self.allocation_debt.get();
        let mut work_done = 0usize;

        while work > work_done as f64 {
            match self.phase.get() {
                Phase::Wake => {
                    self.sweep_prev.set(None);

                    for root in self.roots.borrow().iter() {
                        root.as_ref().head.color.set(Color::Black);
                    }

                    for root in self.roots.borrow().iter() {
                        root.as_ref().value.trace(Tracer(self));
                        work_done += mem::size_of_val(root.as_ref());
                    }

                    self.phase.set(Phase::Mark);
                }
                Phase::Mark => {
                    let ptr = self.grays.borrow_mut().pop();
                    if let Some(ptr) = ptr {
                        work_done += mem::size_of_val(ptr.as_ref());
                        ptr.as_ref().value.trace(Tracer(self));
                        ptr.as_ref().head.color.set(Color::Black);
                    } else if let Some(ptr) = self.grays_again.borrow_mut().pop() {
                        ptr.as_ref().value.trace(Tracer(self));
                        ptr.as_ref().head.color.set(Color::Black);
                    } else {
                        self.phase.set(Phase::Sweep);
                        self.sweep = self.all.get();
                        self.remembered_size = 0;
                    }
                }
                Phase::Sweep => {
                    if let Some(ptr) = self.sweep {
                        self.sweep = ptr.as_ref().head.next.get();
                        let layout = alloc::Layout::for_value(ptr.as_ref());

                        if ptr.as_ref().head.color.get() == Color::White {
                            if let Some(prev) = self.sweep_prev.get() {
                                prev.as_ref().head.next.set(ptr.as_ref().head.next.get());
                            } else {
                                self.all.set(ptr.as_ref().head.next.get());
                            }

                            self.total_allocated
                                .set(self.total_allocated.get() - layout.size());
                        } else {
                            self.remembered_size += layout.size();
                            ptr.as_ref().head.color.set(Color::White);
                            self.sweep_prev.set(Some(ptr));
                        }
                    } else {
                        self.phase.set(Phase::Sleep);
                        self.allocation_debt.set(0.0);
                        self.wakeup_total = self.total_allocated.get()
                            + ((self.remembered_size as f64 * Self::PAUSE_FACTOR)
                                .round()
                                .min(usize::MAX as f64) as usize)
                                .max(Self::MIN_SLEEP);
                        return;
                    }
                }
                Phase::Sleep => break,
            }
        }
    }
}
