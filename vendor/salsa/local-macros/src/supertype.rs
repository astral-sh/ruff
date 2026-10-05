use proc_macro2::TokenStream;
use syn::visit_mut::VisitMut;

use crate::token_stream_with_error;

/// The implementation of the `supertype` macro.
///
/// For an entity enum `Foo` with variants `Variant1, ..., VariantN`, we generate
/// mappings between the variants and their corresponding supertypes.
pub(crate) fn supertype(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let enum_item = parse_macro_input!(input as syn::ItemEnum);
    match enum_impl(enum_item) {
        Ok(v) => v.into(),
        Err(e) => token_stream_with_error(input, e),
    }
}

fn enum_impl(enum_item: syn::ItemEnum) -> syn::Result<TokenStream> {
    let enum_name = enum_item.ident.clone();
    let mut variant_names = Vec::new();
    let mut variant_types = Vec::new();
    if enum_item.variants.is_empty() {
        return Err(syn::Error::new(
            enum_item.enum_token.span,
            "empty enums are not permitted",
        ));
    }
    for variant in &enum_item.variants {
        let valid = match &variant.fields {
            syn::Fields::Unnamed(fields) => {
                variant_names.push(variant.ident.clone());
                variant_types.push(fields.unnamed[0].ty.clone());
                fields.unnamed.len() == 1
            }
            syn::Fields::Unit | syn::Fields::Named(_) => false,
        };
        if !valid {
            return Err(syn::Error::new(
                variant.ident.span(),
                "the only form allowed is `Variant(SalsaStruct)`",
            ));
        }
    }

    let (impl_generics, type_generics, where_clause) = enum_item.generics.split_for_impl();

    let as_id = quote! {
        impl #impl_generics zalsa::AsId for #enum_name #type_generics
        #where_clause {
            #[inline]
            fn as_id(&self) -> zalsa::Id {
                match self {
                    #( Self::#variant_names(__v) => zalsa::AsId::as_id(__v), )*
                }
            }
        }
    };

    let from_id = quote! {
        impl #impl_generics zalsa::FromIdWithDb for #enum_name #type_generics
        #where_clause {
            #[inline]
            fn from_id(__id: zalsa::Id, zalsa: &zalsa::Zalsa) -> Self {
                let __type_id = zalsa.lookup_page_type_id(__id);
                <Self as zalsa::SalsaStructInDb>::cast(__id, __type_id).expect("invalid enum variant")
            }
        }
    };

    // Build variant types with all enum lifetime params replaced by 'static,
    // for use in const context (where generic lifetime params are not available).
    let enum_lifetimes: Vec<syn::Lifetime> = enum_item
        .generics
        .lifetimes()
        .map(|lt| lt.lifetime.clone())
        .collect();
    let variant_types_static: Vec<syn::Type> = variant_types
        .iter()
        .map(|ty| replace_lifetimes_with_static(ty, &enum_lifetimes))
        .collect();

    // Generate variant index identifiers for the const concatenation
    let variant_indices: Vec<syn::Ident> = variant_names
        .iter()
        .enumerate()
        .map(|(i, _)| quote::format_ident!("__VARIANT_{}", i))
        .collect();

    let salsa_struct_in_db = quote! {
        impl #impl_generics zalsa::SalsaStructInDb for #enum_name #type_generics
        #where_clause {
            type MemoIngredientMap = zalsa::MemoIngredientIndices;

            const LEAF_TYPE_IDS: &'static [zalsa::ConstTypeId] = {
                // Get each variant's leaf type IDs as consts.
                // We use the 'static version of variant types since consts
                // cannot reference generic lifetime parameters.
                #(
                    const #variant_indices: &[zalsa::ConstTypeId] =
                        <#variant_types_static as zalsa::SalsaStructInDb>::LEAF_TYPE_IDS;
                )*

                // Total number of leaf type IDs across all variants
                const __N: usize = #( #variant_indices.len() + )* 0;

                // Build concatenated array
                const __IDS: [zalsa::ConstTypeId; __N] = {
                    let mut __result = [zalsa::ConstTypeId::of::<()>(); __N];
                    let mut __dst: usize = 0;
                    #(
                        {
                            let mut __i: usize = 0;
                            while __i < #variant_indices.len() {
                                __result[__dst] = #variant_indices[__i];
                                __dst += 1;
                                __i += 1;
                            }
                        }
                    )*
                    __result
                };
                &__IDS
            };

            #[inline]
            fn lookup_ingredient_index(__zalsa: &zalsa::Zalsa) -> zalsa::IngredientIndices {
                zalsa::assert_supertype_no_overlap(
                    stringify!(#enum_name),
                    &[#( <#variant_types_static as zalsa::SalsaStructInDb>::LEAF_TYPE_IDS ),*],
                    &[#( stringify!(#variant_names) ),*],
                );
                zalsa::IngredientIndices::merge([ #( <#variant_types as zalsa::SalsaStructInDb>::lookup_ingredient_index(__zalsa) ),* ])
            }

             fn entries(
                zalsa: &zalsa::Zalsa
            ) -> impl Iterator<Item = zalsa::DatabaseKeyIndex> + '_ {
                 std::iter::empty()
                     #( .chain(<#variant_types as zalsa::SalsaStructInDb>::entries(zalsa)) )*
             }

            #[inline]
            fn cast(id: zalsa::Id, type_id: ::core::any::TypeId) -> Option<Self> {
                #(
                    // Subtle: the ingredient can be missing, but in this case the id cannot come
                    // from it - because it wasn't initialized yet.
                    if let Some(result) = <#variant_types as zalsa::SalsaStructInDb>::cast(id, type_id) {
                        Some(Self::#variant_names(result))
                    } else
                )*
                {
                    None
                }
            }

            #[inline]
            unsafe fn memo_table(
                zalsa: &zalsa::Zalsa,
                id: zalsa::Id,
                current_revision: zalsa::Revision,
            ) -> zalsa::MemoTableWithTypes<'_> {
                // Note that we need to use `dyn_memos` here, as the `Id` could map to any variant
                // of the supertype enum.
                //
                // SAFETY: Guaranteed by caller.
                unsafe { zalsa.table().dyn_memos(id, current_revision) }
            }
        }
    };

    let all_impls = quote! {
        const _: () = {
            use salsa::plumbing as zalsa;

            #as_id
            #from_id
            #salsa_struct_in_db
        };
    };
    Ok(all_impls)
}

/// Replace all occurrences of the given lifetime parameters with `'static`.
///
/// This is needed because associated consts cannot reference generic lifetime
/// parameters from the enclosing impl block.
fn replace_lifetimes_with_static(ty: &syn::Type, lifetimes: &[syn::Lifetime]) -> syn::Type {
    struct LifetimeReplacer<'a>(&'a [syn::Lifetime]);

    impl VisitMut for LifetimeReplacer<'_> {
        fn visit_lifetime_mut(&mut self, lt: &mut syn::Lifetime) {
            if self.0.iter().any(|param_lt| param_lt.ident == lt.ident) {
                *lt = syn::Lifetime::new("'static", lt.apostrophe);
            }
        }
    }

    let mut ty = ty.clone();
    LifetimeReplacer(lifetimes).visit_type_mut(&mut ty);
    ty
}
