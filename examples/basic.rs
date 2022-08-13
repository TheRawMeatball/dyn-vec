#![feature(generic_associated_types)]

fn main() {
    let mut dynvec = DynVec::<dyn Example>::new();
    dynvec.push(0i32);
    dynvec.push(13u128);
    for r in dynvec.iter() {
        r.uses_ref();
    }
    for v in dynvec.drain() {
        v.takes_ownership();
    }
}

pub trait Example {
    fn uses_ref(&self);
    fn takes_ownership(self);
}

impl Example for i32 {
    fn uses_ref(&self) {
        dbg!(self);
    }

    fn takes_ownership(self) {
        dbg!(self);
    }
}
impl Example for u128 {
    fn uses_ref(&self) {
        dbg!(self);
    }

    fn takes_ownership(self) {
        dbg!(self);
    }
}

// <to be generated via macro>

mod private {
    use std::ptr::NonNull;

    use super::Example;
    use bevy_ptr::{OwningPtr, Ptr, PtrMut};
    use dyn_vec::{DynVecStorable, DynVecStorageTrait, Vtable};
    pub struct ExampleVtable {
        as_trait_obj: unsafe fn(Ptr) -> &dyn Example,
        as_mut_trait_obj: unsafe fn(PtrMut) -> &mut dyn Example,
        drop_fn: unsafe fn(OwningPtr),
        takes_ownership: unsafe fn(OwningPtr),
    }

    pub struct ExampleDynVecDrainReturn<'a> {
        vtable: &'static ExampleVtable,
        owning_ptr: OwningPtr<'a>,
    }

    impl ExampleDynVecDrainReturn<'_> {
        pub fn takes_ownership(self) {
            unsafe {
                let owning = OwningPtr::new(NonNull::new_unchecked(self.owning_ptr.as_ptr()));
                let vtable = self.vtable;
                std::mem::forget(self);
                (vtable.takes_ownership)(owning)
            }
        }
    }

    impl Drop for ExampleDynVecDrainReturn<'_> {
        fn drop(&mut self) {
            unsafe {
                let owning = OwningPtr::new(NonNull::new_unchecked(self.owning_ptr.as_ptr()));
                let vtable = self.vtable;
                (vtable.drop_fn)(owning);
            }
        }
    }

    impl DynVecStorageTrait for dyn Example {
        type VTable = ExampleVtable;
    }

    impl Vtable for ExampleVtable {
        type TraitObj<'a> = &'a dyn Example;
        type MutTraitObj<'a> = &'a mut dyn Example;
        type DrainReturn<'a> = ExampleDynVecDrainReturn<'a>;

        fn as_trait_obj(&self) -> for<'a> unsafe fn(Ptr<'a>) -> Self::TraitObj<'a> {
            self.as_trait_obj
        }

        fn as_mut_trait_obj(&self) -> for<'a> unsafe fn(PtrMut<'a>) -> Self::MutTraitObj<'a> {
            self.as_mut_trait_obj
        }

        fn pack_drain_return<'a>(
            &'static self,
            owning_ptr: OwningPtr<'a>,
        ) -> Self::DrainReturn<'a> {
            ExampleDynVecDrainReturn {
                vtable: self,
                owning_ptr,
            }
        }

        fn drop_fn(&self) -> unsafe fn(OwningPtr) {
            self.drop_fn
        }
    }

    impl<T: Example + 'static> DynVecStorable<dyn Example> for (T,) {
        fn get_vtable() -> &'static ExampleVtable {
            &ExampleVtable {
                as_trait_obj: |ptr| unsafe { ptr.deref::<T>() },
                as_mut_trait_obj: |ptr| unsafe { ptr.deref_mut::<T>() },
                drop_fn: |ptr| unsafe { ptr.drop_as::<T>() },
                takes_ownership: |ptr| unsafe { T::takes_ownership(ptr.read()) },
            }
        }
    }
}

use dyn_vec::DynVec;
pub use private::{ExampleDynVecDrainReturn, ExampleVtable};

// </to be generated via macro>
