use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{Ident, ItemTrait};

#[proc_macro_attribute]
pub fn dyn_vec_usable(_: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemTrait);

    let vis = &input.vis;
    let trait_name = &input.ident;

    let private_module_name = Ident::new(&format!("__private_{}", trait_name), Span::call_site());
    let vtable_name = Ident::new(&format!("{}Vtable", trait_name), Span::call_site());
    let drain_return = Ident::new(&format!("{}DrainReturn", trait_name), Span::call_site());

    quote! {
        #input

        #[allow(non_snake_case)]
        mod #private_module_name {
            use super::#trait_name;

            use bevy_ptr::OwningPtr;
            use dyn_vec::{
                BaseDrainReturn, BaseVtable, BaseVtableConstructor, DynVecStorable, DynVecStorageTrait,
                Vtable, VtableCompatible, VtableDrainReturnBinder,
            };

            pub struct #vtable_name {
                base: BaseVtable<Self>,
                takes_ownership: unsafe fn(OwningPtr),
            }

            impl<T: #trait_name + 'static> DynVecStorable<dyn #trait_name> for (T,) {
                const VTABLE: &'static <dyn #trait_name as DynVecStorageTrait>::VTable = &#vtable_name {
                    base: BaseVtableConstructor::<#vtable_name, T>::VTABLE,
                    takes_ownership: |__ptr| unsafe { T::takes_ownership(__ptr.read()) },
                };
            }

            impl #drain_return<'_> {
                pub fn takes_ownership(self) {
                    let (__vtable, __owning) = self.base.destruct();
                    unsafe { (__vtable.takes_ownership)(__owning) }
                }
            }

            // rest is pure boilerplate, zero specific code

            pub struct #drain_return<'a> {
                base: BaseDrainReturn<'a, #vtable_name>,
            }

            impl<'a> From<BaseDrainReturn<'a, #vtable_name>> for #drain_return<'a> {
                fn from(base: BaseDrainReturn<'a, #vtable_name>) -> Self {
                    Self { base }
                }
            }

            impl DynVecStorageTrait for dyn #trait_name {
                type VTable = #vtable_name;
            }

            impl Vtable for #vtable_name {
                type TraitObj = dyn #trait_name;

                fn base(&'static self) -> &'static BaseVtable<Self> {
                    &self.base
                }
            }

            impl<'a> VtableDrainReturnBinder<'a> for #vtable_name {
                type DrainReturn = #drain_return<'a>;
            }

            impl<T: #trait_name + 'static> VtableCompatible<#vtable_name> for (T,) {
                type TrueType = T;
                fn map_ref(t: &Self::TrueType) -> &<#vtable_name as Vtable>::TraitObj {
                    t
                }
                fn map_ref_mut(t: &mut Self::TrueType) -> &mut <#vtable_name as Vtable>::TraitObj {
                    t
                }
           }
        }

        #vis use #private_module_name::#drain_return;
    }
    .into()
}
