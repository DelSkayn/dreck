use std::{
    alloc::Layout,
    cell::{Cell, RefCell, UnsafeCell},
    mem::{ManuallyDrop, MaybeUninit},
    pin::Pin,
    ptr::{addr_of_mut, NonNull},
};

use super::{GcBox, GcDataPtr, Status, UnsafeTrace};

/// The object for marking GC pointers used while tracing objects.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct UnsafeMarker<'a>(&'a UnsafeArena);

impl<'a> UnsafeMarker<'a> {
    /// Mark a GC pointer as alive.
    ///
    /// # Safety
    /// Caller must ensure that the pointer is a valid, alive, GC object allocated by the same arena
    /// that initiated the tracing with this marker.
    pub unsafe fn mark<T: UnsafeTrace>(self, ptr: NonNull<GcBox<T>>) {
        if ptr.as_ref().data_ptr.status() != Status::Untraced {
            return;
        }
        ptr.as_ref().data_ptr.set_status(Status::Marked);
        //println!("marking: {:?}", ptr.as_ptr());

        if T::needs_trace() {
            self.0.grays.borrow_mut().push(ptr.cast::<GcBox<()>>());
        }
    }

    /// Mark a GC pointer as alive for a type erased GC pointer.
    ///
    /// # Safety
    /// Caller must ensure that the pointer is a valid, alive, GC object allocated by the same arena
    /// that initiated the tracing with this marker.
    pub unsafe fn mark_erased(self, ptr: NonNull<GcBox<()>>) {
        if ptr.as_ref().data_ptr.status() != Status::Untraced {
            return;
        }
        ptr.as_ref().data_ptr.set_status(Status::Marked);
        //println!("marking: {:?}", ptr.as_ptr());

        self.0.grays.borrow_mut().push(ptr.cast::<GcBox<()>>());
    }
}

/// A link of an intrusive list.
#[repr(C)]
pub struct ListLink<T> {
    next: Cell<Option<NonNull<ListLink<()>>>>,
    prev: Cell<Option<NonNull<ListLink<()>>>>,
    value: MaybeUninit<T>,
}

impl<T> ListLink<T> {
    /// Put this link after the link given, and before the next link after the link given.
    /// # Safety
    /// Caller must ensure that the link is a valid member of a valid list.
    unsafe fn link<L>(self: Pin<&Self>, after: Pin<&ListLink<L>>) {
        let ptr = NonNull::from(self.get_ref()).cast::<ListLink<()>>();
        let next = after.next.replace(Some(ptr));
        let prev = NonNull::from(after.get_ref()).cast::<ListLink<()>>();
        self.prev.set(Some(prev));
        self.next.set(next);
        if let Some(next) = next {
            next.as_ref().prev.set(Some(ptr));
        }
    }

    /// Remove pointers to the next and previous links
    unsafe fn clear(&self) {
        self.next.set(None);
        self.prev.set(None);
    }

    /// Returns the next link after this link.
    unsafe fn next(&self) -> Option<NonNull<ListLink<()>>> {
        self.next.get()
    }
}

impl<T> Drop for ListLink<T> {
    fn drop(&mut self) {
        let prev = self.prev.get();
        let next = self.next.get();

        if let Some(next) = next {
            unsafe {
                next.as_ref().prev.set(prev);
            }
        }
        if let Some(prev) = prev {
            unsafe {
                prev.as_ref().next.set(next);
            }
        }
    }
}

/// A guard keeping a pointer alive for the duration of guards lifetime.
#[repr(transparent)]
pub struct UnsafeRootGuard(ListLink<NonNull<GcBox<()>>>);

impl UnsafeRootGuard {
    pub fn new() -> Self {
        Self(ListLink {
            next: Cell::new(None),
            prev: Cell::new(None),
            value: MaybeUninit::uninit(),
        })
    }
}

impl Default for UnsafeRootGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Phase {
    Sleep,
    Wake,
    Trace,
    Sweep,
}

/// The arena for garbage collected pointers.
/// This struct is in charge allocating, freeing, and rooting garbage collected pointers.
///
/// This is the unsafe version of the arena and all defined methods on this arena are also marked
/// as unsafe. The safe arena's implement a safe API on top of this arena. During normal use prefer
/// the safe implementations over this one.
pub struct UnsafeArena {
    roots: Box<ListLink<()>>,

    grays: RefCell<Vec<NonNull<GcBox<()>>>>,
    grays_again: RefCell<Vec<NonNull<GcBox<()>>>>,

    all: Cell<Option<NonNull<GcBox<()>>>>,

    sweep: Cell<Option<NonNull<GcBox<()>>>>,
    sweep_prev: Cell<Option<NonNull<GcBox<()>>>>,

    total_allocated: Cell<usize>,
    remembered_size: Cell<usize>,
    wakeup_total: Cell<usize>,
    allocation_debt: Cell<f64>,

    phase: Cell<Phase>,
}

impl UnsafeArena {
    const PAUSE_FACTOR: f64 = 0.5;
    const TIMING_FACTOR: f64 = 1.5;
    const MIN_SLEEP: usize = 4096;

    /// Create a new unsafe arena.
    ///
    /// # Safety.
    /// It is completely save to create an unsafe arena and not use it.
    /// This method is marked unsafe to not deviate from the pattern that all UnsafeArena methods
    /// are unsafe.
    pub unsafe fn new() -> Self {
        UnsafeArena {
            all: Cell::new(None),
            roots: Box::new(ListLink {
                next: Cell::new(None),
                prev: Cell::new(None),
                value: MaybeUninit::uninit(),
            }),

            grays: RefCell::new(Vec::new()),
            grays_again: RefCell::new(Vec::new()),

            sweep: Cell::new(None),
            sweep_prev: Cell::new(None),

            total_allocated: Cell::new(0),
            remembered_size: Cell::new(0),
            wakeup_total: Cell::new(Self::MIN_SLEEP),
            allocation_debt: Cell::new(0.0),

            phase: Cell::new(Phase::Sweep),
        }
    }

    /// Allocate a new GC pointer into the arena with a given value.
    ///
    /// # Safety
    /// Save as long a [`UnsafeTrace`] is implemented correctly and the pointer is never used. To use
    /// the pointer implementer must ensured that the pointer was either rooted, or traced from a
    /// root during any previous garbage collection cycles..
    ///
    /// # Panic
    /// Will panic if the allocation of a pointer fails.
    pub unsafe fn add<T: UnsafeTrace>(&self, value: T) -> NonNull<GcBox<T>> {
        let layout = Layout::new::<GcBox<T>>();
        let ptr = std::alloc::alloc(layout).cast::<GcBox<T>>();
        //println!("allocated: {:?}", ptr);
        let ptr = NonNull::new(ptr).expect("allocation failed");
        let next = self.all.replace(Some(ptr.cast::<GcBox<()>>()));

        let data_ptr = GcDataPtr::new::<T>();
        //println!("v_table: {:?}", data_ptr.v_table() as *const _);

        addr_of_mut!((*ptr.as_ptr()).next).write(Cell::new(next));
        addr_of_mut!((*ptr.as_ptr()).data_ptr).write(data_ptr);
        addr_of_mut!((*ptr.as_ptr()).value).write(UnsafeCell::new(ManuallyDrop::new(value)));

        self.total_allocated
            .set(self.total_allocated.get() + layout.size());

        if self.phase.get() == Phase::Sleep && self.total_allocated.get() > self.wakeup_total.get()
        {
            self.phase.set(Phase::Wake);
        }

        if self.phase.get() != Phase::Sleep {
            self.allocation_debt.set(
                self.allocation_debt.get()
                    + layout.size() as f64
                    + layout.size() as f64 / Self::TIMING_FACTOR,
            )
        }

        if self.phase.get() == Phase::Sweep && self.sweep_prev.get().is_none() {
            self.sweep_prev.set(self.all.get())
        }

        ptr
    }

    /// Run a full collection cycle.
    ///
    /// This function is the same as [`UnsafeArena::collect`] except it will always collect all unrooted
    /// and unreachable GC pointers.
    ///
    /// # Safety
    /// This methods could possibly collect all pointers which are not rooted or traced from a
    /// root. Implementor must ensure that GC pointers that where not rooted or traced before
    /// calling this method are no longer used after calling this method.
    pub unsafe fn collect_full(&self) {
        self.phase.set(Phase::Wake);
        self.allocation_debt.set(f64::INFINITY);
        self.collect()
    }

    /// Allow the arena to collect pointers.
    ///
    /// This arena implements partial collection cycles and sleeping between cycles thus this method
    /// only marks a point where the arena could run garbage collection if nessacry.
    ///
    /// # Safety
    /// This methods could possibly collect all pointers which are not rooted or traced from a
    /// root. Implementor must ensure that GC pointers that where not rooted or traced before
    /// calling this method are no longer used after calling this method.
    pub unsafe fn collect(&self) {
        //println!("=== Collecting ===");
        if self.phase.get() == Phase::Sleep {
            return;
        }

        let work = self.allocation_debt.get();
        let mut work_done = 0usize;

        while work > work_done as f64 {
            match self.phase.get() {
                Phase::Wake => {
                    self.sweep_prev.set(None);

                    let mut cur = self.roots.next();
                    while let Some(x) = cur {
                        let root = x.cast::<UnsafeRootGuard>();
                        let ptr = *root.as_ref().0.value.assume_init_ref();
                        ptr.as_ref().data_ptr.set_status(Status::Marked);
                        //println!("marking root: {:?}", ptr.as_ptr());
                        self.grays.borrow_mut().push(ptr);
                        cur = root.as_ref().0.next();
                    }

                    self.phase.set(Phase::Trace)
                }
                Phase::Trace => {
                    let ptr = self.grays.borrow_mut().pop();
                    if let Some(ptr) = ptr {
                        //println!("tracing: {:?}", ptr.as_ptr());
                        let v_table = ptr.as_ref().data_ptr.v_table();
                        //println!("v table: {:?}", v_table as *const _);
                        work_done += v_table.layout.size();
                        (v_table.trace)(ptr.as_ptr(), UnsafeMarker(self));
                        ptr.as_ref().data_ptr.set_status(Status::Traced);
                    } else if let Some(ptr) = self.grays_again.borrow_mut().pop() {
                        //println!("tracing: {:?}", ptr.as_ptr());
                        let v_table = ptr.as_ref().data_ptr.v_table();
                        (v_table.trace)(ptr.as_ptr(), UnsafeMarker(self));
                        ptr.as_ref().data_ptr.set_status(Status::Traced);
                    } else {
                        self.phase.set(Phase::Sweep);
                        self.sweep.set(self.all.get());
                        self.remembered_size.set(0)
                    }
                }
                Phase::Sweep => {
                    if let Some(ptr) = self.sweep.get() {
                        //println!("sweeping: {:?}", ptr.as_ptr());
                        self.sweep.set(ptr.as_ref().next.get());
                        let v_table = ptr.as_ref().data_ptr.v_table();
                        if ptr.as_ref().data_ptr.status() == Status::Untraced {
                            //println!("freeing: {:?}", ptr.as_ptr());
                            if let Some(prev) = self.sweep_prev.get() {
                                prev.as_ref().next.set(ptr.as_ref().next.get())
                            } else {
                                self.all.set(ptr.as_ref().next.get())
                            }
                            self.total_allocated
                                .set(self.total_allocated.get() - v_table.layout.size());

                            (v_table.drop)(ptr.as_ptr());
                            std::alloc::dealloc(ptr.as_ptr().cast(), v_table.layout);
                        } else {
                            self.remembered_size
                                .set(self.remembered_size.get() + v_table.layout.size());
                            ptr.as_ref().data_ptr.set_status(Status::Untraced);
                            self.sweep_prev.set(Some(ptr))
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
                        return;
                    }
                }
                Phase::Sleep => break,
            }
        }
    }

    /// Root a GC pointer ensuring that it will remain rooted for as long as the lifetime of th
    /// UnsafeRootGuard object,
    ///
    /// # Safety
    /// Caller must ensure that the pointer is a valid, alive, GC pointer allocated by this arena.
    pub unsafe fn root<T>(&self, mut guard: Pin<&mut UnsafeRootGuard>, value: NonNull<GcBox<T>>) {
        //println!("rooting: {:?}", value.as_ptr());
        guard.0.value.as_mut_ptr().write(value.cast::<GcBox<()>>());
        guard
            .into_ref()
            .map_unchecked(|x| &x.0)
            .link(Pin::new(&self.roots));
    }

    /// Mark an object as possibly containing new GC pointers. Any time an object that is allocated
    /// in the GC has recieved new GC pointers marked by its `UnsafeTrace` implemention this method
    /// must be called with the that object before a new call to collect is done.
    ///
    /// # Safety
    /// Caller must ensure that the pointer is a valid, alive, GC pointer allocated by this arena.
    pub unsafe fn write_barrier<T: UnsafeTrace>(&self, value: NonNull<GcBox<T>>) {
        if T::needs_trace() {
            return;
        }
        unsafe {
            if self.phase.get() == Phase::Trace
                && value.as_ref().data_ptr.status() == Status::Traced
            {
                value.as_ref().data_ptr.set_status(Status::Marked);
                self.grays_again
                    .borrow_mut()
                    .push(value.cast::<GcBox<()>>());
            }
        }
    }
}

impl Drop for UnsafeArena {
    fn drop(&mut self) {
        unsafe {
            self.roots.clear();
            self.collect_full();
        }
    }
}
