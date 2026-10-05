use crate::hygiene::Hygiene;
use crate::xform::ChangeLt;

/// Returns a vector of ids representing the function arguments.
/// Prefers to reuse the names given by the user, if possible.
pub fn input_ids(hygiene: &Hygiene, sig: &syn::Signature, skip: usize) -> Vec<syn::Ident> {
    sig.inputs
        .iter()
        .skip(skip)
        .zip(0..)
        .map(|(input, index)| {
            if let syn::FnArg::Typed(typed) = input {
                if let syn::Pat::Ident(ident) = &*typed.pat {
                    return ident.ident.clone();
                }
            }

            hygiene.ident(format!("input{index}"))
        })
        .collect()
}

pub fn input_tys(sig: &syn::Signature, skip: usize) -> syn::Result<Vec<&syn::Type>> {
    sig.inputs
        .iter()
        .skip(skip)
        .map(|input| {
            if let syn::FnArg::Typed(typed) = input {
                Ok(&*typed.ty)
            } else {
                Err(syn::Error::new_spanned(input, "unexpected receiver"))
            }
        })
        .collect()
}

pub fn output_ty(db_lt: Option<&syn::Lifetime>, sig: &syn::Signature) -> syn::Result<syn::Type> {
    match &sig.output {
        syn::ReturnType::Default => Ok(parse_quote!(())),
        syn::ReturnType::Type(_, ty) => match db_lt {
            Some(db_lt) => Ok(ChangeLt::elided_to(db_lt).in_type(ty)),
            None => Ok(syn::Type::clone(ty)),
        },
    }
}
