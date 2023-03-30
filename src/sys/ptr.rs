use std::{
    alloc::Layout,
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
    ptr::NonNull,
};

use super::{UnsafeMarker, UnsafeTrace};

/// A custom v-table for a GC allocated type.
#[repr(align(16))]
pub struct GcVTable {
    /// The layout of the type in the GcBox so if this v-table is for type `T` the layout would be
    /// for `GcBox<T>`
    pub layout: Layout,
    /// The method for tracing the type.
    pub trace: unsafe fn(*mut GcBox<()>, UnsafeMarker),
    /// The method for dropping the type.
    pub drop: unsafe fn(*mut GcBox<()>),
}

unsafe fn trace<T: UnsafeTrace>(ptr: *mut GcBox<()>, marker: UnsafeMarker) {
    //println!("vtable tracing {:?}", ptr);
    (*(*ptr.cast::<GcBox<T>>()).value.get()).trace(marker);
}

unsafe fn drop<T: UnsafeTrace>(ptr: *mut GcBox<()>) {
    ManuallyDrop::drop(&mut (*(*ptr.cast::<GcBox<T>>()).value.get()));
}

impl GcVTable {
    /// Creates a new v-table for this type.
    pub const fn new<T: UnsafeTrace>() -> Self {
        GcVTable {
            layout: Layout::new::<T>(),
            trace: trace::<T>,
            drop: drop::<T>,
        }
    }

    /// Returns a static reference to the v-table for this type.
    pub fn get<T: UnsafeTrace>() -> &'static GcVTable {
        trait HasVTable {
            const V_TABLE: GcVTable;
        }

        impl<T: UnsafeTrace> HasVTable for T {
            const V_TABLE: GcVTable = GcVTable::new::<T>();
        }

        &<T as HasVTable>::V_TABLE
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum Status {
    Untraced = 0,
    Marked = 1,
    MarkedWeak = 2,
    Traced = 3,
}

/// A packad data pointer that encoded both a pointer to a v-table as well as a the tracing status
/// for the pointer.
#[derive(Debug)]
#[repr(transparent)]
pub struct GcDataPtr(Cell<NonNull<GcVTable>>);

impl GcDataPtr {
    /// Creates a new data pointer for a specific type.
    pub fn new<T: UnsafeTrace>() -> Self {
        Self(Cell::new(NonNull::from(GcVTable::get::<T>())))
    }

    fn as_ptr(&self) -> *mut GcVTable {
        ((self.0.get().as_ptr() as usize) & (!0b11usize)) as *mut GcVTable
    }

    /// Returns a reference to the  v-table of the type this pointer was created for.
    pub fn v_table(&self) -> &GcVTable {
        unsafe { &(*self.as_ptr()) }
    }

    /// Returns the packed tracing status.
    pub fn status(&self) -> Status {
        let status = ((self.0.get().as_ptr() as usize) & 0b11) as u8;
        unsafe { std::mem::transmute(status) }
    }

    /// Sets the packed tracing status.
    pub fn set_status(&self, status: Status) {
        let value = (self.0.get().as_ptr() as usize & !0b11usize) | (status as u8 as usize);
        unsafe { self.0.set(NonNull::new_unchecked(value as *mut GcVTable)) }
    }
}

/// A struct containing a GC allocated object.
#[repr(C)]
pub struct GcBox<T: ?Sized> {
    /// Pointer to the next object in the list of all GC allocated objects.
    pub next: Cell<Option<NonNull<GcBox<()>>>>,
    /// A packed pointer containing both tracing information as well as a pointer to the v table of
    /// the contained object.
    pub data_ptr: GcDataPtr,
    /// the contained object itself.
    pub value: UnsafeCell<ManuallyDrop<T>>,
}

impl<T: UnsafeTrace> GcBox<T> {
    pub fn new(value: T) -> Self {
        Self {
            next: Cell::new(None),
            data_ptr: GcDataPtr::new::<T>(),
            value: UnsafeCell::new(ManuallyDrop::new(value)),
        }
    }
}
