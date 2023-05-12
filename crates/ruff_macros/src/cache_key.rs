use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Fields};

pub(crate) fn derive_cache_key(item: &DeriveInput) -> syn::Result<TokenStream> {
    let fields = match &item.data {
        Data::Enum(item_enum) => {
            let arms = item_enum.variants.iter().enumerate().map(|(i, variant)| {
                let variant_name = &variant.ident;

                match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names: Vec<_> = fields
                            .named
                            .iter()
                            .map(|field| field.ident.clone().unwrap())
                            .collect();

                        let fields_code = field_names
                            .iter()
                            .map(|field| quote!(#field.cache_key(key);));

                        quote! {
                            Self::#variant_name{#(#field_names),*} => {
                                key.write_usize(#i);
                                #(#fields_code)*
                            }
                        }
                    }
                    Fields::Unnamed(fields) => {
                        let field_names: Vec<_> = fields
                            .unnamed
                            .iter()
                            .enumerate()
                            .map(|(i, _)| format_ident!("field_{i}"))
                            .collect();

                        let fields_code = field_names
                            .iter()
                            .map(|field| quote!(#field.cache_key(key);));

                        quote! {
                            Self::#variant_name(#(#field_names),*) => {
                                key.write_usize(#i);
                                #(#fields_code)*
                            }
                        }
                    }
                    Fields::Unit => {
                        quote! {
                            Self::#variant_name => {
                                key.write_usize(#i);
                            }
                        }
                    }
                }
            });

            quote! {
                match self {
                    #(#arms)*
                }
            }
        }

        Data::Struct(item_struct) => {
            let fields = item_struct.fields.iter().enumerate().map(|(i, field)| {
                let field_attr = match &field.ident {
                    Some(ident) => quote!(self.#ident),
                    None => {
                        let index = syn::Index::from(i);
                        quote!(self.#index)
                    }
                };

                quote!(#field_attr.cache_key(key);)
            });

            quote! {#(#fields)*}
        }

        Data::Union(_) => {
            return Err(Error::new(
                item.span(),
                "CacheKey does not support unions. Only structs and enums are supported",
            ))
        }
    };

    let name = &item.ident;
    let (impl_generics, ty_generics, where_clause) = &item.generics.split_for_impl();

    Ok(quote!(
        impl #impl_generics ruff_cache::CacheKey for #name #ty_generics #where_clause {
            fn cache_key(&self, key: &mut ruff_cache::CacheKeyHasher) {
                use std::hash::Hasher;
                use ruff_cache::CacheKey;
                #fields
            }
        }
    ))
}
