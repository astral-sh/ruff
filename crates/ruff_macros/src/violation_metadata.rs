use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, DeriveInput, Error, Lit, LitStr, Meta};

pub(crate) fn violation_metadata(input: DeriveInput) -> syn::Result<TokenStream> {
    let docs = get_docs(&input.attrs)?;

    let Some(group) = get_rule_status(&input.attrs)? else {
        return Err(Error::new_spanned(
            input,
            "Missing required rule group metadata",
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

            fn group() -> crate::codes::RuleGroup {
                crate::codes::#group
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

/// Extract the rule status attribute.
///
/// These attributes look like:
///
/// ```ignore
/// #[violation_metadata(stable_since = "1.2.3")]
/// struct MyRule;
/// ```
///
/// The result is returned as a `TokenStream` so that the version string literal can be combined
/// with the proper `RuleGroup` variant, e.g. `RuleGroup::Stable` for `stable_since` above.
fn get_rule_status(attrs: &[Attribute]) -> syn::Result<Option<TokenStream>> {
    let mut group = None;
    for attr in attrs {
        if attr.path().is_ident("violation_metadata") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("stable_since") {
                    let lit: LitStr = meta.value()?.parse()?;
                    group = Some(quote!(RuleGroup::Stable { since: #lit }));
                    return Ok(());
                } else if meta.path.is_ident("preview_since") {
                    let lit: LitStr = meta.value()?.parse()?;
                    group = Some(quote!(RuleGroup::Preview { since: #lit }));
                    return Ok(());
                } else if meta.path.is_ident("deprecated_since") {
                    let lit: LitStr = meta.value()?.parse()?;
                    group = Some(quote!(RuleGroup::Deprecated { since: #lit }));
                    return Ok(());
                } else if meta.path.is_ident("removed_since") {
                    let lit: LitStr = meta.value()?.parse()?;
                    group = Some(quote!(RuleGroup::Removed { since: #lit }));
                    return Ok(());
                }
                Err(Error::new_spanned(
                    attr,
                    "unimplemented violation metadata option",
                ))
            })?;
        }
    }
    Ok(group)
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
