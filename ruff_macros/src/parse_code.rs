use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DataEnum, DeriveInput, Error, Lit, Meta, MetaNameValue};

pub fn derive_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveInput { ident, data: Data::Enum(DataEnum {
        variants, ..
    }), .. } = input else {
        return Err(Error::new(input.ident.span(), "only named fields are supported"));
    };

    let mut parsed = Vec::new();

    for variant in variants {
        let prefix_attrs: Vec<_> = variant
            .attrs
            .iter()
            .filter(|a| a.path.is_ident("prefix"))
            .collect();

        if prefix_attrs.is_empty() {
            return Err(Error::new(
                variant.span(),
                r#"Missing [#prefix = "..."] attribute"#,
            ));
        }

        for attr in prefix_attrs {
            let Ok(Meta::NameValue(MetaNameValue{lit: Lit::Str(lit), ..})) = attr.parse_meta() else {
                return Err(Error::new(attr.span(), r#"expected attribute to be in the form of [#prefix = "..."]"#))
            };
            parsed.push((lit, variant.ident.clone()));
        }
    }

    parsed.sort_by_key(|(prefix, _)| prefix.value().len());

    let mut if_statements = quote!();

    for (prefix, field) in parsed {
        if_statements.extend(quote! {if let Some(rest) = code.strip_prefix(#prefix) {
            return Some((#ident::#field, rest));
        }});
    }

    Ok(quote! {
        impl crate::registry::ParseCode for #ident {
            fn parse_code(code: &str) -> Option<(Self, &str)> {
                #if_statements
                None
            }
        }
    })
}
