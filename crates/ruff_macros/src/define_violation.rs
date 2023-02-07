use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, Ident, Lit, LitStr, Meta, Result, Token};

fn parse_attr<const LEN: usize>(path: [&'static str; LEN], attr: &Attribute) -> Option<LitStr> {
    if let Meta::NameValue(name_value) = attr.parse_meta().ok()? {
        let path_idents = name_value
            .path
            .segments
            .iter()
            .map(|segment| &segment.ident);

        if itertools::equal(path_idents, path) {
            if let Lit::Str(lit) = name_value.lit {
                return Some(lit);
            }
        }
    }

    None
}

pub struct LintMeta {
    explanation: String,
    name: Ident,
}

impl Parse for LintMeta {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;

        let mut in_code = false;
        let mut explanation = String::new();
        for attr in &attrs {
            if let Some(lit) = parse_attr(["doc"], attr) {
                let value = lit.value();
                let line = value.strip_prefix(' ').unwrap_or(&value);
                if line.starts_with("```") {
                    explanation += line;
                    explanation.push('\n');
                    in_code = !in_code;
                } else if !(in_code && line.starts_with("# ")) {
                    explanation += line;
                    explanation.push('\n');
                }
            } else {
                return Err(Error::new_spanned(attr, "unexpected attribute"));
            }
        }

        input.parse::<Token![pub]>()?;
        input.parse::<Token![struct]>()?;
        let name = input.parse()?;

        // Ignore the rest of the input.
        input.parse::<TokenStream>()?;

        Ok(Self { explanation, name })
    }
}

pub fn define_violation(input: &TokenStream, meta: LintMeta) -> TokenStream {
    let LintMeta { explanation, name } = meta;
    if explanation.is_empty() {
        quote! {
            #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
            #input
        }
    } else {
        quote! {
            #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
            #input

            impl #name {
                pub fn explanation() -> Option<&'static str> {
                    Some(#explanation)
                }
            }
        }
    }
}
