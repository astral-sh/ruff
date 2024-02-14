use quote::{quote, quote_spanned};
use syn::{Data, DataStruct, DeriveInput, Fields};

pub(crate) fn derive_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveInput { ident, data, .. } = input;

    match data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => {
            let output: Vec<_> = fields
                .named
                .iter()
                .map(|field| {
                    let ident = field
                        .ident
                        .as_ref()
                        .expect("Expected to handle named fields");

                    quote_spanned!(
                        ident.span() => #ident: crate::configuration::CombineOptions::combine(self.#ident, other.#ident)
                    )
                })
                .collect();

            Ok(quote! {
                #[automatically_derived]
                impl crate::configuration::CombineOptions for #ident {
                    fn combine(self, other: Self) -> Self {
                        #[allow(deprecated)]
                        Self {
                        #(
                            #output
                        ),*
                        }
                    }
                }
            })
        }
        _ => Err(syn::Error::new(
            ident.span(),
            "Can only derive CombineOptions from structs with named fields.",
        )),
    }
}
