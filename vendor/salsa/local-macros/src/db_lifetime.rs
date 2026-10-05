//! Helper functions for working with fns, structs, and other generic things
//! that are allowed to have a `'db` lifetime.

use proc_macro2::Span;
use syn::spanned::Spanned;

/// Normally we try to use whatever lifetime parameter the user gave us
/// to represent `'db`; but if they didn't give us one, we need to use a default
/// name. We choose `'db`.
pub(crate) fn default_db_lifetime(span: Span) -> syn::Lifetime {
    syn::Lifetime {
        apostrophe: span,
        ident: syn::Ident::new("db", span),
    }
}

/// Require that either there are no generics or exactly one lifetime parameter.
pub(crate) fn require_optional_db_lifetime(generics: &syn::Generics) -> syn::Result<()> {
    if generics.params.is_empty() {
        return Ok(());
    }

    require_db_lifetime(generics)?;

    Ok(())
}

/// Require that either there is exactly one lifetime parameter.
pub(crate) fn require_db_lifetime(generics: &syn::Generics) -> syn::Result<()> {
    if generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            generics,
            "this definition must have a `'db` lifetime",
        ));
    }

    for (param, index) in generics.params.iter().zip(0..) {
        let error = match param {
            syn::GenericParam::Lifetime(_) => index > 0,
            syn::GenericParam::Type(_) | syn::GenericParam::Const(_) => true,
        };

        if error {
            return Err(syn::Error::new_spanned(
                param,
                "only a single lifetime parameter is accepted",
            ));
        }
    }

    Ok(())
}

/// Return the `'db` lifetime given by the user, or a default.
/// The generics ought to have been checked with `require_db_lifetime` already.
pub(crate) fn db_lifetime(generics: &syn::Generics) -> syn::Lifetime {
    if let Some(lt) = generics.lifetimes().next() {
        lt.lifetime.clone()
    } else {
        default_db_lifetime(generics.span())
    }
}

pub(crate) fn require_no_generics(generics: &syn::Generics) -> syn::Result<()> {
    if let Some(param) = generics.params.iter().next() {
        return Err(syn::Error::new_spanned(
            param,
            "generic parameters not allowed here",
        ));
    }

    Ok(())
}
