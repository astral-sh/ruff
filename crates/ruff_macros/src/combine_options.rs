use quote::{quote, quote_spanned};
use syn::{Data, DataStruct, DeriveInput, Field, Fields, Path, PathSegment, Type, TypePath};

pub(crate) fn derive_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveInput { ident, data, .. } = input;

    match data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => {
            let output = fields
                .named
                .iter()
                .map(handle_field)
                .collect::<Result<Vec<_>, _>>()?;

            Ok(quote! {
                #[automatically_derived]
                impl crate::configuration::CombinePluginOptions for #ident {
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

fn handle_field(field: &Field) -> syn::Result<proc_macro2::TokenStream> {
    let ident = field
        .ident
        .as_ref()
        .expect("Expected to handle named fields");

    match &field.ty {
        Type::Path(TypePath {
            path: Path { segments, .. },
            ..
        }) => match segments.first() {
            Some(PathSegment {
                ident: type_ident, ..
            }) if type_ident == "Option" => Ok(quote_spanned!(
                ident.span() => #ident: self.#ident.or(other.#ident)
            )),
            _ => Err(syn::Error::new(
                ident.span(),
                "Expected `Option<_>` or `Vec<_>` as type.",
            )),
        },
        _ => Err(syn::Error::new(ident.span(), "Expected type.")),
    }
}
