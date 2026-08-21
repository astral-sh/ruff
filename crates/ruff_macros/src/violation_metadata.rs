use std::sync::LazyLock;

use proc_macro2::TokenStream;
use quote::quote;
use regex::Regex;
use syn::{Attribute, DeriveInput, Error, Lit, LitStr, Meta, Path, meta::ParseNestedMeta};

pub(crate) fn violation_metadata(input: DeriveInput) -> syn::Result<TokenStream> {
    let docs = get_docs(&input.attrs)?;

    let metadata = get_metadata(&input.attrs)?;

    let Some(status) = metadata.status else {
        return Err(Error::new_spanned(
            &input,
            "Missing required rule status metadata",
        ));
    };

    let Some(category) = metadata.category else {
        return Err(Error::new_spanned(
            &input,
            "Missing required rule category metadata",
        ));
    };

    let name = input.ident;

    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();

    Ok(quote! {
        #[automatically_derived]
        #[expect(deprecated)]
        impl #impl_generics crate::ViolationMetadata for #name #ty_generics #where_clause {
            fn rule() -> crate::registry::Rule {
                crate::registry::Rule::#name
            }

            fn explain() -> Option<&'static str> {
                Some(#docs)
            }

            fn status() -> crate::codes::RuleStatus {
                crate::codes::#status
            }

            fn category() -> crate::codes::Category {
                #category
            }

            fn file() -> &'static str {
                file!()
            }

            fn line() -> u32 {
                line!()
            }
        }
    })
}

/// Collect all doc comment attributes into a string
fn get_docs(attrs: &[Attribute]) -> syn::Result<String> {
    let mut explanation = String::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Some(lit) = parse_attr(["doc"], attr) {
                let value = lit.value();
                // `/// ` adds
                let line = value.strip_prefix(' ').unwrap_or(&value);
                explanation.push_str(line);
                explanation.push('\n');
            } else {
                return Err(Error::new_spanned(attr, "unimplemented doc comment style"));
            }
        }
    }
    Ok(explanation)
}

/// Extract the rule metadata attributes.
///
/// These attributes look like:
///
/// ```ignore
/// #[violation_metadata(stable_since = "1.2.3", category = Category::Correctness)]
/// struct MyRule;
/// ```
///
/// The rule status is stored as a `TokenStream` so that the version string literal can be combined
/// with the proper `RuleStatus` variant, e.g. `RuleStatus::Stable` for `stable_since` above.
fn get_metadata(attrs: &[Attribute]) -> syn::Result<Metadata> {
    let mut metadata = Metadata::default();
    for attr in attrs {
        if attr.path().is_ident("violation_metadata") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("stable_since") {
                    let lit: LitStr = parse_version(&meta)?;
                    metadata.status = Some(quote!(RuleStatus::Stable { since: #lit }));
                    return Ok(());
                } else if meta.path.is_ident("preview_since") {
                    let lit: LitStr = parse_version(&meta)?;
                    metadata.status = Some(quote!(RuleStatus::Preview { since: #lit }));
                    return Ok(());
                } else if meta.path.is_ident("deprecated_since") {
                    let lit: LitStr = parse_version(&meta)?;
                    metadata.status = Some(quote!(RuleStatus::Deprecated { since: #lit }));
                    return Ok(());
                } else if meta.path.is_ident("removed_since") {
                    let lit: LitStr = parse_version(&meta)?;
                    metadata.status = Some(quote!(RuleStatus::Removed { since: #lit }));
                    return Ok(());
                } else if meta.path.is_ident("category") {
                    metadata.category = Some(meta.value()?.parse()?);
                    return Ok(());
                }
                Err(Error::new_spanned(
                    attr,
                    "unimplemented violation metadata option",
                ))
            })?;
        }
    }
    Ok(metadata)
}

#[derive(Default)]
struct Metadata {
    status: Option<TokenStream>,
    category: Option<Path>,
}

fn parse_attr<'a, const LEN: usize>(
    path: [&'static str; LEN],
    attr: &'a Attribute,
) -> Option<&'a LitStr> {
    if let Meta::NameValue(name_value) = &attr.meta {
        let path_idents = name_value
            .path
            .segments
            .iter()
            .map(|segment| &segment.ident);

        if itertools::equal(path_idents, path) {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: Lit::Str(lit), ..
            }) = &name_value.value
            {
                return Some(lit);
            }
        }
    }

    None
}

fn parse_version(meta: &ParseNestedMeta) -> syn::Result<LitStr> {
    /// Match either a semantic version with an optional `v` prefix for versions before 0.5.0
    /// (`v0.2.3`, `0.12.34`) or the special `NEXT_RUFF_VERSION` placeholder that is updated by
    /// rooster in releases.
    static VERSION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(v0.[0-4].\d+|\d+\.\d+\.\d+|NEXT_RUFF_VERSION)$").unwrap());

    let lit: LitStr = meta.value()?.parse()?;
    let value = lit.value();

    if VERSION.is_match(&value) {
        Ok(lit)
    } else {
        Err(Error::new_spanned(
            lit,
            format_args!("Unknown version specifier `{value}`"),
        ))
    }
}
