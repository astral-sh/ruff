use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DataStruct, DeriveInput};

pub(crate) fn derive_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveInput { ident, data, .. } = input;

    match data {
        Data::Struct(DataStruct { fields, .. }) => {
            let output: Vec<_> = fields
                .members()
                .map(|member| {

                    quote_spanned!(
                        member.span() => crate::combine::Combine::combine_with(&mut self.#member, other.#member)
                    )
                })
                .collect();

            Ok(quote! {
                #[automatically_derived]
                impl crate::combine::Combine for #ident {
                    fn combine_with(&mut self, other: Self) {
                        #(
                            #output
                        );*
                    }
                }
            })
        }
        _ => Err(syn::Error::new(
            ident.span(),
            "Can only derive Combine from structs.",
        )),
    }
}
