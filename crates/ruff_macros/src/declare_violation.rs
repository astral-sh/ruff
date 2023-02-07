use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Attribute, Error, Ident, Lit, LitStr, Meta, Result, Token};

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
                    explanation += "```\n";
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

        Ok(Self { explanation, name })
    }
}

pub fn declare_violation(input: TokenStream) -> TokenStream {
    // let x: proc_macro::TokenStream = input.into();
    // let LintMeta { explanation, name } = parse_macro_input!(x as LintMeta);

    // let mut category = category.to_string();
    //
    // let level = format_ident!(
    //     "{}",
    //     match category.as_str() {
    //         "correctness" => "Deny",
    //         "style" | "suspicious" | "complexity" | "perf" | "internal_warn" => "Warn",
    //         "pedantic" | "restriction" | "cargo" | "nursery" | "internal" => "Allow",
    //         _ => panic!("unknown category {category}"),
    //     },
    // );
    //
    // let info = if category == "internal_warn" {
    //     None
    // } else {
    //     let info_name = format_ident!("{name}_INFO");
    //
    //     (&mut category[0..1]).make_ascii_uppercase();
    //     let category_variant = format_ident!("{category}");
    //
    //     Some(quote! {
    //         pub(crate) static #info_name: &'static crate::LintInfo = &crate::LintInfo {
    //             lint: &#name,
    //             category: crate::LintCategory::#category_variant,
    //             explanation: #explanation,
    //         };
    //     })
    // };

    // Just add derives to the struct.
    let output = quote! {
        #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #input

        // impl #name {
        //     pub fn explanation() -> Option<String> {
        //         Some(#explanation.to_string())
        //     }
        // }
    };

    TokenStream::from(output)
}
