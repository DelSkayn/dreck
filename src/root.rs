use crate::{
    cell::LCell,
    ptr::{Color, GcBox, GcBoxPtr},
    CellOwner, Gc, Rebind,
};

use super::{cell::Invariant, Trace};
use std::{
    alloc::{self, Layout},
    cell::{Cell, RefCell},
    marker::PhantomData,
    mem,
    ptr::{self, NonNull},
};

/// Object passed to [`Trace::trace`] method, used to mark pointers.
#[derive(Clone, Copy)]
pub struct Tracer<'rt> {
    // Lifetime is static here to avoid needing an extra lifetime on
    // both tracer and the Trace macro.
    root: &'rt Root<'static>,
}

impl<'rt> Tracer<'rt> {
    /// Mark a pointer as alive.
    pub fn mark<T: Trace>(self, gc: Gc<'_, '_, T>) {
        unsafe {
            // Pointer is valid as gc can only contain valid pointers.
            let r = gc.ptr.as_ref();
            if r.color.get() != Color::White {
                return;
            }

            r.color.set(Color::Gray);
            if T::needs_trace() {
                let ptr: GcBoxPtr<'_, '_> = gc.ptr;
                // Change the lifetimes to static.
                // Roots implementation ensures that the lifetime constraints are upheld.
                self.root
                    .grays
                    .borrow_mut()
                    .push(mem::transmute::<GcBoxPtr, GcBoxPtr>(ptr));
            }
        }
    }

    /// Mark a pointer with a dynamic type as alive.
    pub fn mark_dynamic(self, gc: Gc<'_, '_, dyn Trace>) {
        unsafe {
            // Pointer is valid as gc can only contain valid pointers.
            let r = gc.ptr.as_ref();
            if r.color.get() != Color::White {
                return;
            }

            r.color.set(Color::Gray);
            // Change the lifetimes to static.
            // Roots implementation ensures that the lifetime constraints are upheld.
            self.root
                .grays
                .borrow_mut()
                .push(mem::transmute::<GcBoxPtr, GcBoxPtr>(gc.ptr));
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

/// Root of the dreck GC, used for allocating [`Gc`] pointers and run collection.
pub struct Root<'cell> {
    // All lifetimes are static as the implementation ensures that the values contained in the
    // pointer are never borrowed without a CellOwner and the values are valid as long as the
    // GcBoxPtr exists.
    /// The root values, point to alive values.
    roots: RefCell<Vec<*const dyn Trace>>,

    /// Ptr's which have been marked as alive
    grays: RefCell<Vec<GcBoxPtr<'static, 'cell>>>,
    /// Ptr's which might have new alive values as the value pointed to has been mutated.
    grays_again: RefCell<Vec<GcBoxPtr<'static, 'cell>>>,

    /// Current pointer reached while cleaning up.
    sweep: Cell<Option<GcBoxPtr<'static, 'cell>>>,
    sweep_prev: Cell<Option<GcBoxPtr<'static, 'cell>>>,

    /// The list of all gc allocated pointers.
    all: Cell<Option<GcBoxPtr<'static, 'cell>>>,

    total_allocated: Cell<usize>,
    remembered_size: Cell<usize>,
    wakeup_total: Cell<usize>,
    allocation_debt: Cell<f64>,

    phase: Cell<Phase>,

    _marker: Invariant<'cell>,
}

/// A guard which will keep a value on the stack rooted for as long as it is alive.
///
/// This struct should not be used in safe code.
pub struct RootGuard<'cell>(*const Root<'cell>);

impl<'cell> RootGuard<'cell> {
    /// Rebind a value to the lifetime of the root guard.
    /// # Safety
    /// This method should only ever be called with the object this root guard was created in
    /// [`Root::root_gc`].
    pub unsafe fn bind<'a, R: Rebind<'a>>(&'a self, r: R) -> R::Output {
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

impl<'cell> Root<'cell> {
    const PAUSE_FACTOR: f64 = 0.5;
    const TIMING_FACTOR: f64 = 1.5;
    const MIN_SLEEP: usize = 4096;

    /// Create a new gc root.
    /// Prefer the use of the [`new_root!`] macro.
    ///
    /// # Safety
    /// - It is unsafe to create two roots of with the same `'cell` lifetime.
    /// - The `Root` object must outlife all [`RootGuard`]'s created either by the [`root!`] macro
    /// or by [`Root::root_gc`].
    pub unsafe fn new(owner: &CellOwner<'cell>) -> Self {
        Root {
            roots: RefCell::new(Vec::new()),

            grays: RefCell::new(Vec::new()),
            grays_again: RefCell::new(Vec::new()),

            sweep: Cell::new(None),
            sweep_prev: Cell::new(None),

            all: Cell::new(None),

            total_allocated: Cell::new(0),
            remembered_size: Cell::new(0),
            wakeup_total: Cell::new(Self::MIN_SLEEP),
            allocation_debt: Cell::new(0.0),

            phase: Cell::new(Phase::Sleep),
            _marker: owner.0,
        }
    }

    /// Allocated a value as a garbage collected pointer.
    #[must_use]
    pub fn add<'gc, T>(&'gc self, v: T) -> Gc<'gc, 'cell, T::Output>
    where
        T: Rebind<'gc>,
        T::Output: Trace,
    {
        unsafe {
            let layout = Layout::new::<GcBox<T::Output>>();
            let ptr = NonNull::new(alloc::alloc(layout).cast::<GcBox<T::Output>>()).unwrap();
            ptr.as_ptr().write(GcBox {
                color: Cell::new(Color::White),
                next: Cell::new(self.all.get()),
                value: LCell::new(super::rebind(v)),
            });

            self.total_allocated
                .set(self.total_allocated.get() + layout.size());

            if self.phase.get() == Phase::Sleep
                && self.total_allocated.get() > self.wakeup_total.get()
            {
                self.phase.set(Phase::Wake);
            }

            if self.phase.get() != Phase::Sleep {
                self.allocation_debt.set(
                    self.allocation_debt.get()
                        + layout.size() as f64
                        + layout.size() as f64 / Self::TIMING_FACTOR,
                );
            }

            let dyn_ptr: GcBoxPtr = ptr;
            self.all.set(Some(mem::transmute(dyn_ptr)));

            if self.phase.get() == Phase::Sweep && self.sweep_prev.get().is_none() {
                self.sweep_prev.set(self.all.get());
            }
            Gc {
                ptr,
                marker: PhantomData,
            }
        }
    }

    /// Run a full cycle of the garbage collection.
    ///
    /// Unlike [`Root::collect`] this method will allways collect all unreachable Gc'd objects.
    pub fn collect_full(&mut self, owner: &CellOwner<'cell>) {
        self.allocation_debt.set(f64::INFINITY);
        self.phase.set(Phase::Wake);
        self.collect(owner);
    }

    /// Indicate a point at which garbage collection can run.
    ///
    /// The gc will only run if enough objects have been allocated.
    /// As the gc is incremental it will also only run only a part of the collection cycle.
    pub fn collect(&mut self, owner: &CellOwner<'cell>) {
        unsafe {
            if self.phase.get() == Phase::Sleep {
                return;
            }

            let work = self.allocation_debt.get();
            let mut work_done = 0usize;

            while work > work_done as f64 {
                match self.phase.get() {
                    Phase::Wake => {
                        self.sweep_prev.set(None);

                        let borrow = &*self;
                        self.roots.borrow().iter().copied().for_each(|x| {
                            (*x).trace(Tracer {
                                root: mem::transmute(borrow),
                            });
                            work_done += mem::size_of_val(&(*x));
                        });

                        self.phase.set(Phase::Mark);
                    }
                    Phase::Mark => {
                        let ptr = self.grays.borrow_mut().pop();
                        if let Some(ptr) = ptr {
                            //assert!(tmp.insert(x.as_ptr()));
                            let size = mem::size_of_val(ptr.as_ref());
                            work_done += size;
                            let borrow = &*self;
                            ptr.as_ref().value.borrow(owner).trace(Tracer {
                                root: mem::transmute(borrow),
                            });

                            ptr.as_ref().color.set(Color::Black);
                        } else {
                            let ptr = self.grays.borrow_mut().pop();
                            if let Some(ptr) = ptr {
                                //assert!(!tmp.insert(x.as_ptr()));
                                ptr.as_ref().value.borrow(owner).trace(Tracer {
                                    root: mem::transmute(&*self),
                                });
                                ptr.as_ref().color.set(Color::Black);
                            } else {
                                let borrow = &*self;
                                self.roots.borrow().iter().copied().for_each(|x| {
                                    (*x).trace(Tracer {
                                        root: mem::transmute(borrow),
                                    });
                                });

                                // Found new values in root
                                // Should continue tracing till no more free values are found.
                                if !self.grays.borrow().is_empty() {
                                    continue;
                                }

                                self.phase.set(Phase::Sweep);
                                self.sweep.set(self.all.get());
                            }
                        }
                    }
                    Phase::Sweep => {
                        if let Some(x) = self.sweep.get() {
                            self.sweep.set(x.as_ref().next.get());
                            let layout = Layout::for_value(x.as_ref());

                            if x.as_ref().color.get() == Color::White {
                                if let Some(prev) = self.sweep_prev.get() {
                                    prev.as_ref().next.set(x.as_ref().next.get());
                                } else {
                                    self.all.set(x.as_ref().next.get());
                                }

                                self.total_allocated
                                    .set(self.total_allocated.get() - layout.size());

                                ptr::drop_in_place(x.as_ptr());
                                alloc::dealloc(x.as_ptr().cast(), layout);
                            } else {
                                self.remembered_size
                                    .set(self.remembered_size.get() + layout.size());
                                x.as_ref().color.set(Color::White);
                                self.sweep_prev.set(Some(x));
                            }
                        } else {
                            self.phase.set(Phase::Sleep);
                            self.allocation_debt.set(0.0);
                            self.wakeup_total.set(
                                self.total_allocated.get()
                                    + ((self.remembered_size.get() as f64 * Self::PAUSE_FACTOR)
                                        .round()
                                        .min(usize::MAX as f64)
                                        as usize)
                                        .max(Self::MIN_SLEEP),
                            );
                        }
                    }
                    Phase::Sleep => break,
                }
            }
            self.allocation_debt
                .set((self.allocation_debt.get() - work_done as f64).max(0.0));
        }
    }

    /// Mark a pointer value as possibly containing new gc pointers.
    ///
    /// In safe code you should never have to call this method as the [`Gc`] struct will manage
    /// write barriers for you.
    ///
    /// If a type has an unsafe trace implementation and could ever contain new Gc'd values within
    /// itself, One must call this function on objects of that type before running collection, everytime that object could
    /// possibly contain new Gc'd values.
    #[inline]
    pub fn write_barrier<'a, T: Trace + 'a>(&self, gc: Gc<'a, 'cell, T>) {
        if !T::needs_trace() {
            return;
        }
        unsafe {
            if self.phase.get() == Phase::Mark && gc.ptr.as_ref().color.get() == Color::Black {
                gc.ptr.as_ref().color.set(Color::Gray);
                let ptr: GcBoxPtr<'a, 'cell> = gc.ptr;
                self.grays_again.borrow_mut().push(mem::transmute(ptr));
            }
        }
    }

    /// Root a gc pointer for the duration of root guard's lifetime.
    /// Prefer the use of the [`root!`] macro.
    ///
    /// # Safety
    /// - The `Root` object must outlife the returned `RootGuard`
    /// - All `RootGuard`'s must be dropped in the reverse order of which they where created.
    pub unsafe fn root_gc<T: Trace>(&self, t: Gc<'_, 'cell, T>) -> RootGuard<'cell> {
        self.roots.borrow_mut().push(t.as_trace_ptr());
        RootGuard(self)
    }

    /// Rebind a pointer to the lifetime of this root guard.
    ///
    /// On should prefer the [`rebind!`] macro instead of this function as it is more permissive
    /// with which pointers it allows rebinding.
    pub fn rebind_to<'a, T: Trace + Rebind<'a>>(
        &'a self,
        t: Gc<'_, 'cell, T>,
    ) -> Gc<'a, 'cell, T::Output> {
        unsafe { crate::rebind(t) }
    }
}
