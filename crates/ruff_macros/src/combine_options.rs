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
                ident: type_ident,
                arguments,
            }) if type_ident == "Option" => {
                // Given `Option<FxHashMap<_>>`, combine the maps by merging. In TOML, a hash map is
                // represented as a table, so merging the maps is the correct behavior.
                if let syn::PathArguments::AngleBracketed(args) = arguments {
                    let inner_type_ident = args
                        .args
                        .first()
                        .and_then(|arg| match arg {
                            syn::GenericArgument::Type(Type::Path(TypePath {
                                path: Path { segments, .. },
                                ..
                            })) => segments.first().map(|seg| &seg.ident),
                            _ => None,
                        })
                        .ok_or_else(|| {
                            syn::Error::new(
                                ident.span(),
                                "Expected `Option<_>` with a single type argument.",
                            )
                        })?;
                    if inner_type_ident == "HashMap"
                        || inner_type_ident == "BTreeMap"
                        || inner_type_ident == "FxHashMap"
                    {
                        return Ok(quote_spanned!(
                            ident.span() => #ident: match (self.#ident, other.#ident) {
                                (Some(mut m1), Some(m2)) => {
                                    m1.extend(m2);
                                    Some(m1)
                                },
                                (None, Some(m)) | (Some(m), None) => Some(m),
                                (None, None) => None,
                            }
                        ));
                    }
                }

                Ok(quote_spanned!(
                    ident.span() => #ident: self.#ident.or(other.#ident)
                ))
            }

            _ => Err(syn::Error::new(
                ident.span(),
                "Expected `Option<_>` as type.",
            )),
        },
        _ => Err(syn::Error::new(ident.span(), "Expected type.")),
    }
}
