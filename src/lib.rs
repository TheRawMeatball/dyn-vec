#![cfg_attr(feature = "nightly", feature(inline_const))]
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "nightly")]
mod nightly;

use std::{alloc::Layout, marker::PhantomData, ptr::NonNull};

const DYN_VEC_ALIGN_POW_COUNT: usize = 5;
const DYN_VEC_ALIGN_POW_BASE: usize = 4;

const _: () = {
    if !DYN_VEC_ALIGN_POW_BASE.is_power_of_two() {
        panic!("`DYN_VEC_ALIGN_POW_BASE` must be a power of two");
    }
};

use bevy_ptr::{OwningPtr, Ptr, PtrMut};

pub use dyn_vec_macro::dyn_vec_usable;

pub struct DynVec<S: DynVecStorageTrait + ?Sized> {
    cols: [AlignedCol; DYN_VEC_ALIGN_POW_COUNT],
    metas: [Vec<Meta<S>>; DYN_VEC_ALIGN_POW_COUNT],
}

impl<S: DynVecStorageTrait + ?Sized> Drop for DynVec<S> {
    fn drop(&mut self) {
        self.drain().for_each(|_| {}); // dealloc any other allocations held by inner values
        for (index, buf) in self.cols.iter().enumerate() {
            let align = DYN_VEC_ALIGN_POW_BASE.pow((index + 1) as u32);
            if let Some(ptr) = buf.buf {
                let dealloc_layout = Layout::from_size_align(buf.capacity, align).unwrap();
                unsafe {
                    std::alloc::dealloc(ptr.as_ptr(), dealloc_layout); // dealloc our own allocations
                }
            }
        }
    }
}

struct Meta<S: DynVecStorageTrait + ?Sized> {
    vtable: &'static S::VTable,
    offset: usize,
}

pub trait VtableCompatible<VTable: Vtable> {
    type TrueType;
    fn map_ref(t: &Self::TrueType) -> &VTable::TraitObj;
    fn map_ref_mut(t: &mut Self::TrueType) -> &mut VTable::TraitObj;
}

pub trait Vtable: Sized + 'static {
    type TraitObj: ?Sized;

    fn base(&'static self) -> &'static BaseVtable<Self>;
}

pub type DrainReturn<'a, VTable> = <VTable as VtableDrainReturnBinder<'a>>::DrainReturn;

pub trait VtableDrainReturnBinder<'a>: Vtable {
    type DrainReturn: From<BaseDrainReturn<'a, Self>>;
}

pub const fn get_index_and_align<T>() -> (usize, usize) {
    let align = std::mem::align_of::<T>();
    let mut index_to_test = 0;
    while index_to_test < DYN_VEC_ALIGN_POW_COUNT {
        if align <= DYN_VEC_ALIGN_POW_BASE.pow((index_to_test + 1) as u32) {
            return (
                index_to_test,
                DYN_VEC_ALIGN_POW_BASE.pow((index_to_test + 1) as u32),
            );
        }
        index_to_test += 1;
    }
    panic!("This type isn't supported in DynVec");
}

impl<S: DynVecStorageTrait + ?Sized> DynVec<S> {
    pub fn new() -> Self {
        Self {
            cols: std::array::from_fn(|_| AlignedCol::new()),
            metas: std::array::from_fn(|_| Default::default()),
        }
    }
    pub fn push<T>(&mut self, val: T)
    where
        (T,): DynVecStorable<S>,
    {
        #[cfg(feature = "nightly")]
        let (index, align) = nightly::get_index_and_align::<T>();
        #[cfg(not(feature = "nightly"))]
        let (index, align) = get_index_and_align::<T>();

        let correct_col = &mut self.cols[index];
        let correct_meta = &mut self.metas[index];
        // Safe: align is calculated via const fns and size is directly fed
        OwningPtr::make(val, |ptr| unsafe {
            let offset = correct_col.push(ptr, std::mem::size_of::<T>(), align);
            correct_meta.push(Meta {
                vtable: <(T,)>::VTABLE,
                offset,
            });
        });
    }

    pub fn iter(&self) -> impl Iterator<Item = &<S::VTable as Vtable>::TraitObj> {
        self.metas
            .iter()
            .zip(self.cols.iter())
            .flat_map(|(metas, col)| unsafe {
                let buf = col.buf;
                metas.iter().map(move |meta| {
                    // if there's meta s, buffer mustn't be null
                    let buf = buf.unwrap_unchecked().as_ptr();
                    let ptr = buf.add(meta.offset);
                    // we know its not null
                    let ptr = Ptr::new(NonNull::new_unchecked(ptr));
                    (meta.vtable.base().as_trait_obj)(ptr)
                })
            })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut <S::VTable as Vtable>::TraitObj> {
        self.metas
            .iter()
            .zip(self.cols.iter_mut())
            .flat_map(|(metas, col)| unsafe {
                let buf = col.buf;
                metas.iter().map(move |meta| {
                    // if there's meta s, buffer mustn't be null
                    let buf = buf.unwrap_unchecked().as_ptr();
                    let ptr = buf.add(meta.offset);
                    // we know its not null
                    let ptr = PtrMut::new(NonNull::new_unchecked(ptr));
                    (meta.vtable.base().as_mut_trait_obj)(ptr)
                })
            })
    }

    pub fn drain(&mut self) -> impl Iterator<Item = DrainReturn<'_, S::VTable>> {
        self.metas
            .iter_mut()
            .zip(self.cols.iter_mut())
            .flat_map(|(metas, col)| unsafe {
                let buf = col.buf;
                col.cursor = 0;
                metas.drain(..).map(move |meta| {
                    // if there's meta s, buffer mustn't be null
                    let buf = buf.unwrap_unchecked().as_ptr();
                    let ptr = buf.add(meta.offset);
                    // we know its not null
                    let ptr = OwningPtr::new(NonNull::new_unchecked(ptr));
                    let base = BaseDrainReturn {
                        vtable: meta.vtable,
                        ptr,
                    };
                    base.into()
                })
            })
    }
}

pub trait DynVecStorageTrait {
    type VTable: for<'a> VtableDrainReturnBinder<'a>;
}

pub trait DynVecStorable<StoredFor: DynVecStorageTrait + ?Sized> {
    const VTABLE: &'static StoredFor::VTable;
}

#[repr(C)]
struct AlignedCol {
    buf: Option<NonNull<u8>>,
    cursor: usize,
    capacity: usize,
}

impl AlignedCol {
    fn new() -> Self {
        Self {
            buf: None,
            cursor: 0,
            capacity: 0,
        }
    }

    fn allocate_space_for(&mut self, size: usize, align: usize) {
        let remaining_space = self.capacity - self.cursor;
        let required_extra = size as isize - remaining_space as isize;
        if required_extra <= 0 {
            return;
        }
        let required_extra = required_extra as usize; // known positive
        let new_space = (self.capacity + required_extra)
            .max(align)
            .next_power_of_two();
        assert_ne!(new_space, 0);
        unsafe {
            let alloc_layout = Layout::from_size_align(new_space, align).unwrap();
            let new_buf = std::alloc::alloc(alloc_layout);
            if new_buf.is_null() {
                std::alloc::handle_alloc_error(alloc_layout);
            }

            if self.capacity > 0 {
                let dealloc_layout = Layout::from_size_align(self.capacity, align).unwrap();
                // Safe: buf isn't null if capacity > 0
                let old_buf = self.buf.unwrap_unchecked().as_ptr();
                std::ptr::copy(old_buf, new_buf, self.cursor);
                std::alloc::dealloc(old_buf, dealloc_layout);
            }
            // check done earlier
            self.buf = Some(NonNull::new_unchecked(new_buf));
            self.capacity = alloc_layout.size();
        }
    }

    /// Returns the byte offset at which the value is located
    ///
    /// # Safety:
    ///
    /// The align of `val` must be less than or equal to `ALIGN`
    /// `size` must be equal to the size of the value in `val`
    unsafe fn push(&mut self, val: OwningPtr, size: usize, align: usize) -> usize {
        if size == 0 {
            return 0;
        }
        self.allocate_space_for(size, align);

        unsafe {
            // Safe: buffer allocated just now
            let buf = self.buf.unwrap_unchecked().as_ptr();
            let write_start = buf.add(self.cursor);
            let val = val.as_ptr();
            std::ptr::copy_nonoverlapping(val, write_start, size);
        }
        let cursor = self.cursor;
        let diff = size % align;
        self.cursor += size;
        if diff != 0 {
            self.cursor += align - diff;
        }
        cursor
    }
}

pub struct BaseVtable<VTable: Vtable> {
    as_trait_obj: unsafe fn(Ptr) -> &VTable::TraitObj,
    as_mut_trait_obj: unsafe fn(PtrMut) -> &mut VTable::TraitObj,
    drop_fn: unsafe fn(OwningPtr),
}

pub struct BaseVtableConstructor<VTable, T>(PhantomData<(VTable, T)>);
impl<VTable, T> BaseVtableConstructor<VTable, T>
where
    VTable: Vtable,
    (T,): VtableCompatible<VTable, TrueType = T> + 'static,
{
    pub const VTABLE: BaseVtable<VTable> = BaseVtable {
        as_trait_obj: |ptr| unsafe { <(T,)>::map_ref(ptr.deref::<T>()) },
        as_mut_trait_obj: |ptr| unsafe { <(T,)>::map_ref_mut(ptr.deref_mut::<T>()) },
        drop_fn: |ptr| unsafe { ptr.drop_as::<T>() },
    };
}

pub struct BaseDrainReturn<'a, VTable: Vtable> {
    vtable: &'static VTable,
    ptr: OwningPtr<'a>,
}

impl<VTable: Vtable> Drop for BaseDrainReturn<'_, VTable> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            let owning = OwningPtr::new(NonNull::new_unchecked(self.ptr.as_ptr()));
            let vtable = self.vtable;
            (vtable.base().drop_fn)(owning);
        }
    }
}

impl<'a, VTable: Vtable> BaseDrainReturn<'a, VTable> {
    // this is necessary because rust doesn't allow any other way to bypass a drop impl while destructuring
    pub fn destruct(self) -> (&'static VTable, OwningPtr<'a>) {
        unsafe {
            let vtable = self.vtable;
            let owning = NonNull::new_unchecked(self.ptr.as_ptr());
            std::mem::forget(self);
            (vtable, OwningPtr::new(owning))
        }
    }

    pub fn as_dyn_ref(&self) -> &VTable::TraitObj {
        unsafe {
            (self.vtable.base().as_trait_obj)(Ptr::new(NonNull::new_unchecked(self.ptr.as_ptr())))
        }
    }

    pub fn as_mut_dyn_ref(&mut self) -> &mut VTable::TraitObj {
        unsafe {
            (self.vtable.base().as_mut_trait_obj)(PtrMut::new(NonNull::new_unchecked(
                self.ptr.as_ptr(),
            )))
        }
    }
}
