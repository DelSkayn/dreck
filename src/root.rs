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

#[derive(Clone, Copy)]
pub struct Tracer<'rt> {
    // Lifetime is static here to avoid needing an extra lifetime on tracer.
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

pub struct RootGuard<'rt, 'cell>(pub &'rt Root<'cell>);

impl<'rt, 'cell> RootGuard<'rt, 'cell> {
    pub unsafe fn bind<'a, R: Rebind<'a>>(&'a self, r: R) -> R::Output {
        crate::rebind(r)
    }
}

impl<'rt, 'cell> Drop for RootGuard<'rt, 'cell> {
    fn drop(&mut self) {
        self.0.roots.borrow_mut().pop();
    }
}

impl<'cell> Root<'cell> {
    const PAUSE_FACTOR: f64 = 0.5;
    const TIMING_FACTOR: f64 = 1.5;
    const MIN_SLEEP: usize = 4096;

    pub unsafe fn new() -> Self {
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
            _marker: Invariant::new(),
        }
    }

    pub unsafe fn root_gc<'rt, T: Trace>(&'rt self, t: Gc<'_, 'cell, T>) -> RootGuard<'rt, 'cell> {
        self.roots.borrow_mut().push(t.as_trace_ptr());
        RootGuard(self)
    }

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

    pub fn collect_full(&mut self, owner: &mut CellOwner<'cell>) {
        self.allocation_debt.set(f64::INFINITY);
        self.phase.set(Phase::Wake);
        self.collect(owner);
    }

    pub fn collect(&mut self, owner: &mut CellOwner<'cell>) {
        unsafe {
            if self.phase.get() == Phase::Sleep {
                return;
            }

            let work = self.allocation_debt.get();
            let mut work_done = 0usize;

            //let mut tmp = HashSet::new();

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
                        if let Some(ptr) = self.grays.borrow_mut().pop() {
                            //assert!(tmp.insert(x.as_ptr()));
                            let size = mem::size_of_val(ptr.as_ref());
                            work_done += size;
                            let borrow = &*self;
                            ptr.as_ref().value.borrow(owner).trace(Tracer {
                                root: mem::transmute(borrow),
                            });

                            ptr.as_ref().color.set(Color::Black);
                        } else if let Some(ptr) = self.grays_again.borrow_mut().pop() {
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
}
