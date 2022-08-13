use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{punctuated::Punctuated, Ident, ItemTrait, Token};

#[proc_macro_attribute]
pub fn dyn_vec_usable(_: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemTrait);

    let vis = &input.vis;
    let trait_name = &input.ident;

    let private_module_name = Ident::new(&format!("__private_{}", trait_name), Span::call_site());
    let vtable_name = Ident::new(&format!("{}Vtable", trait_name), Span::call_site());
    let drain_return = Ident::new(&format!("{}DrainReturn", trait_name), Span::call_site());

    let self_methods = input
        .items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Method(m) => Some(&m.sig),
            _ => None,
        })
        .filter(|sig| {
            sig.inputs
                .first()
                .map(|x| match x {
                    syn::FnArg::Receiver(r) => r.reference.is_none(),
                    _ => false,
                })
                .unwrap_or(false)
        });

    let mut vtable_struct_entries = Punctuated::<_, Token![,]>::new();
    let mut vtable_const_entries = Punctuated::<_, Token![,]>::new();
    let mut impls = vec![];

    for sig in self_methods {
        let method_name = &sig.ident;
        let mut arg_names = Punctuated::<_, Token![,]>::new();
        let mut arg_types = Punctuated::<_, Token![,]>::new();
        for (i, arg) in sig.inputs.iter().skip(1).enumerate() {
            match arg {
                syn::FnArg::Typed(typ) => {
                    arg_names.push(match &*typ.pat {
                        syn::Pat::Ident(ident)
                            if ident.by_ref.is_none() && ident.subpat.is_none() =>
                        {
                            ident.ident.clone()
                        }
                        _ => Ident::new(&format!("arg{i}"), Span::call_site()),
                    });
                    arg_types.push(&*typ.ty);
                }
                _ => unreachable!(),
            }
        }

        vtable_struct_entries.push(quote! {
            #method_name: unsafe fn (OwningPtr, #arg_types)
        });

        vtable_const_entries.push(quote! {
            #method_name: |__ptr, #arg_names| unsafe { T::#method_name(__ptr.read(), #arg_names) }
        });

        let names = arg_names.iter();
        let types = arg_types.iter();

        impls.push(quote! {
            pub fn #method_name(self, #(#names: #types),*) {
                let (__vtable, __owning) = self.base.destruct();
                unsafe { (__vtable.takes_ownership)(__owning, #arg_names) }
            }
        });
    }

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
                #vtable_struct_entries
            }

            impl<T: #trait_name + 'static> DynVecStorable<dyn #trait_name> for (T,) {
                const VTABLE: &'static <dyn #trait_name as DynVecStorageTrait>::VTable = &#vtable_name {
                    base: BaseVtableConstructor::<#vtable_name, T>::VTABLE,
                    #vtable_const_entries
                };
            }

            impl #drain_return<'_> {
                pub fn as_dyn_ref(&self) -> &dyn #trait_name {
                    self.base.as_dyn_ref()
                }
            
                pub fn as_mut_dyn_ref(&mut self) -> &mut dyn #trait_name {
                    self.base.as_mut_dyn_ref()
                }

                #(#impls)*
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
