use std::collections::HashSet;

use proc_macro2::TokenStream;
use syn::{Attribute, parse::Parse, spanned::Spanned, visit::Visit};

use crate::kw;
use crate::xform::{ChangeLt, ChangeSelfPath};

pub(crate) enum ManualRetentionProof {
    Conditional(Vec<syn::WherePredicate>),
    Unconditional,
}

struct CheckedField<'a> {
    ty: &'a syn::Type,
    proof: Option<ManualRetentionProof>,
}

pub(crate) fn salsa_value_derive(input: syn::DeriveInput) -> syn::Result<TokenStream> {
    if let syn::Data::Union(union) = &input.data {
        return Err(syn::Error::new_spanned(
            union.union_token,
            "`derive(SalsaValue)` does not support `union`",
        ));
    }

    reject_salsa_value_attributes(&input.attrs, "type")?;

    let mut declared_lifetimes = input.generics.lifetimes();
    let declared_lifetime = declared_lifetimes
        .next()
        .map(|parameter| parameter.lifetime.clone());
    if declared_lifetimes.next().is_some() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "`derive(SalsaValue)` supports at most one lifetime parameter",
        ));
    }
    let mut checked_fields = Vec::new();

    match &input.data {
        syn::Data::Struct(data) => {
            collect_checked_field_types(&data.fields, &mut checked_fields)?;
        }
        syn::Data::Enum(data) => {
            for variant in &data.variants {
                reject_salsa_value_attributes(&variant.attrs, "variant")?;
                collect_checked_field_types(&variant.fields, &mut checked_fields)?;
            }
        }
        syn::Data::Union(_) => unreachable!(),
    }

    let ident = &input.ident;
    let (_, type_generics, _) = input.generics.split_for_impl();
    let derived_type = syn::parse_quote!(#ident #type_generics);
    let zalsa = quote::format_ident!("__salsa_plumbing");

    let generics = add_field_bounds(input.generics.clone(), &checked_fields, &derived_type);
    let (impl_generics, _, where_clause) = generics.split_for_impl();

    let implementation = quote! {
        #[automatically_derived]
        // SAFETY: The generated bounds and assertions verify every field without
        // an explicit manual retention proof.
        unsafe impl #impl_generics ::salsa::SalsaValue for #derived_type #where_clause {}
    };

    let Some(assertion_lifetime) = declared_lifetime else {
        return Ok(crate::debug::dump_tokens(&input.ident, implementation));
    };

    let field_assertions = checked_fields.iter().map(|CheckedField { ty, proof }| {
        let ty = replace_self_type(ty, &derived_type);

        assert_salsa_value_field(&assertion_lifetime, &zalsa, &ty, proof.is_some())
    });

    let tokens = quote! {
        #implementation

        const _: () = {
            use ::salsa::plumbing as #zalsa;
            use #zalsa::{SalsaValueDispatch, SalsaValueFallback as _};

            #[allow(dead_code)]
            fn _assert_fields_are_salsa_values #impl_generics () #where_clause {
                #(#field_assertions)*
            }
        };
    };

    Ok(crate::debug::dump_tokens(&input.ident, tokens))
}

fn add_field_bounds(
    mut generics: syn::Generics,
    checked_fields: &[CheckedField<'_>],
    derived_type: &syn::Type,
) -> syn::Generics {
    let type_params = generics
        .type_params()
        .map(|parameter| parameter.ident.clone())
        .collect::<HashSet<_>>();

    for CheckedField { ty, proof } in checked_fields {
        match proof {
            Some(ManualRetentionProof::Conditional(predicates)) => {
                generics.make_where_clause().predicates.extend(
                    predicates
                        .iter()
                        .map(|predicate| replace_self_in_predicate(predicate, derived_type)),
                );
            }
            Some(ManualRetentionProof::Unconditional) => {}
            None if type_uses_type_param(ty, &type_params) => {
                generics
                    .make_where_clause()
                    .predicates
                    .push(syn::parse_quote!(#ty: ::salsa::SalsaValue));
            }
            None => {}
        }
    }

    generics
}

fn type_uses_type_param(ty: &syn::Type, type_params: &HashSet<syn::Ident>) -> bool {
    struct UsesTypeParam<'a> {
        type_params: &'a HashSet<syn::Ident>,
        result: bool,
    }

    impl<'ast> Visit<'ast> for UsesTypeParam<'_> {
        fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
            if path.qself.is_none() {
                if let Some(segment) = path.path.segments.first() {
                    self.result |= self.type_params.contains(&segment.ident);
                }
            }

            if !self.result {
                syn::visit::visit_type_path(self, path);
            }
        }
    }

    let mut visitor = UsesTypeParam {
        type_params,
        result: false,
    };
    visitor.visit_type(ty);
    visitor.result
}

fn collect_checked_field_types<'a>(
    fields: &'a syn::Fields,
    checked_fields: &mut Vec<CheckedField<'a>>,
) -> syn::Result<()> {
    for field in fields {
        checked_fields.push(CheckedField {
            ty: &field.ty,
            proof: field_manual_retention_proof(field)?,
        });
    }

    Ok(())
}

fn field_manual_retention_proof(field: &syn::Field) -> syn::Result<Option<ManualRetentionProof>> {
    let mut attrs = field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("salsa_value"));
    let Some(attr) = attrs.next() else {
        return Ok(None);
    };

    if attrs.next().is_some() {
        return Err(syn::Error::new_spanned(
            field,
            "multiple `#[salsa_value]` attributes on field",
        ));
    }

    parse_manual_retention_proof(attr).map(Some)
}

pub(crate) fn parse_manual_retention_proof(attr: &Attribute) -> syn::Result<ManualRetentionProof> {
    attr.parse_args_with(|input: syn::parse::ParseStream<'_>| {
        if input.peek(kw::prove) {
            return Err(input.error("`prove(...)` must be wrapped in `unsafe(...)`"));
        }

        let _: syn::Token![unsafe] = input.parse()?;
        let content;
        syn::parenthesized!(content in input);

        let proof = if content.peek(kw::prove) {
            content.parse::<kw::prove>()?;
            let predicates;
            syn::parenthesized!(predicates in content);
            if predicates.is_empty() {
                return Err(predicates.error("`prove(...)` requires at least one predicate"));
            }

            ManualRetentionProof::Conditional(
                predicates
                    .parse_terminated(syn::WherePredicate::parse, syn::Token![,])?
                    .into_iter()
                    .collect(),
            )
        } else {
            content.parse::<kw::prove_safe_to_retain_manually>()?;
            ManualRetentionProof::Unconditional
        };

        if content.is_empty() {
            Ok(proof)
        } else {
            Err(content.error("unexpected token"))
        }
    })
}

fn reject_salsa_value_attributes(attrs: &[Attribute], target: &str) -> syn::Result<()> {
    let errors = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("salsa_value"))
        .map(|attr| {
            syn::Error::new(
                attr.path().span(),
                format!("unexpected `#[salsa_value]` attribute on {target}"),
            )
        });

    match errors.reduce(|mut combined, error| {
        combined.combine(error);
        combined
    }) {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn replace_self_in_predicate(
    predicate: &syn::WherePredicate,
    self_type: &syn::Type,
) -> syn::WherePredicate {
    let mut predicate = predicate.clone();
    syn::visit_mut::VisitMut::visit_where_predicate_mut(
        &mut ChangeSelfPath::new(self_type, None),
        &mut predicate,
    );
    predicate
}

fn replace_self_type(ty: &syn::Type, self_type: &syn::Type) -> syn::Type {
    let mut ty = ty.clone();
    syn::visit_mut::VisitMut::visit_type_mut(&mut ChangeSelfPath::new(self_type, None), &mut ty);
    ty
}

pub(crate) fn assert_salsa_value_or_static(
    db_lt: &syn::Lifetime,
    zalsa: &syn::Ident,
    ty: &syn::Type,
) -> TokenStream {
    // Prefer the direct bound when the return type names the database lifetime,
    // so rustc reports a missing `SalsaValue` impl instead of a failed `'static` fallback.
    if crate::xform::uses_lifetime(ty, db_lt) {
        return assert_tracked_output_is_salsa_value(db_lt, zalsa, ty);
    }

    let assertion = assert_salsa_value_or_static_expr(db_lt, zalsa, ty);
    quote! {
        fn _assert_output_is_salsa_value_or_static() {
            use #zalsa::{SalsaValueDispatch, SalsaValueFallback as _};
            #assertion
        }
        let _ = _assert_output_is_salsa_value_or_static;
    }
}

pub(crate) fn assert_salsa_value_field(
    db_lt: &syn::Lifetime,
    zalsa: &syn::Ident,
    ty: &syn::Type,
    has_manual_retention_proof: bool,
) -> TokenStream {
    if has_manual_retention_proof {
        quote! {}
    } else {
        assert_salsa_value_or_static_expr(db_lt, zalsa, ty)
    }
}

pub(crate) fn assert_salsa_value_field_with_proof(
    db_lt: &syn::Lifetime,
    zalsa: &syn::Ident,
    ty: &syn::Type,
    proof: Option<&ManualRetentionProof>,
    self_type: &syn::Type,
) -> TokenStream {
    match proof {
        None => assert_salsa_value_or_static_expr(db_lt, zalsa, ty),
        Some(ManualRetentionProof::Unconditional) => quote! {},
        Some(ManualRetentionProof::Conditional(predicates)) => {
            let predicates = predicates
                .iter()
                .map(|predicate| replace_self_in_predicate(predicate, self_type));
            quote! {
                {
                    #[allow(unused_lifetimes)]
                    fn _assert_manual_retention_proof<#db_lt>()
                    where
                        #(#predicates,)*
                    {}
                    _assert_manual_retention_proof::<#db_lt>();
                }
            }
        }
    }
}

fn assert_tracked_output_is_salsa_value(
    db_lt: &syn::Lifetime,
    zalsa: &syn::Ident,
    ty: &syn::Type,
) -> TokenStream {
    if is_db_reference(ty, db_lt) {
        return syn::Error::new_spanned(
            ty,
            "a reference tied to the database lifetime does not implement `SalsaValue`; return an owned value instead",
        )
        .into_compile_error();
    }

    let assertion_lifetime = syn::Lifetime::new(&format!("'{}", db_lt.ident), ty.span());
    let ty = ChangeLt::named_to(db_lt, &assertion_lifetime).in_type(ty);
    let assertion = quote_spanned! {ty.span() =>
        #zalsa::assert_salsa_value::<#ty>();
    };
    quote! {
        fn _assert_output_is_salsa_value<#assertion_lifetime>() {
            #assertion
        }
        let _ = _assert_output_is_salsa_value;
    }
}

fn assert_salsa_value_or_static_expr(
    db_lt: &syn::Lifetime,
    zalsa: &syn::Ident,
    ty: &syn::Type,
) -> TokenStream {
    if crate::xform::uses_lifetime(ty, db_lt) {
        if is_db_reference(ty, db_lt) {
            return syn::Error::new_spanned(
                ty,
                "a reference tied to the database lifetime does not implement `SalsaValue`; store owned data or a Salsa struct instead",
            )
            .into_compile_error();
        }

        return quote_spanned! {ty.span() =>
            #zalsa::assert_salsa_value::<#ty>();
        };
    }

    quote_spanned! {ty.span() =>
        let _ = SalsaValueDispatch::<#ty>::assert_salsa_value;
    }
}

fn is_db_reference(ty: &syn::Type, db_lt: &syn::Lifetime) -> bool {
    matches!(
        ty,
        syn::Type::Reference(reference)
            if reference
                .lifetime
                .as_ref()
                .is_some_and(|lifetime| lifetime.ident == db_lt.ident)
    )
}
