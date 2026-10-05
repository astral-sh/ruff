use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse::Nothing;
use syn::visit::Visit;
use syn::visit_mut::VisitMut;

use crate::hygiene::Hygiene;
use crate::tracked_fn::FnArgs;
use crate::xform::{ChangeLt, ChangeSelfPath};

pub(crate) fn tracked_impl(
    args: proc_macro::TokenStream,
    item: syn::ItemImpl,
) -> syn::Result<TokenStream> {
    let hygiene = Hygiene::from2(&item);
    let _: Nothing = syn::parse(args)?;
    let m = Macro { hygiene };
    let generated = m.try_generate(item)?;
    Ok(generated)
}

struct Macro {
    hygiene: Hygiene,
}

struct AssociatedFunctionArguments<'syn> {
    self_token: Option<&'syn syn::token::SelfValue>,
    db_input_index: usize,
    db_ty: &'syn syn::Type,
    db_ident: &'syn syn::Ident,
    db_lt: syn::Lifetime,
    has_explicit_db_lt: bool,
    input_ids: Vec<syn::Ident>,
    input_tys: Vec<syn::Type>,
    output_ty: syn::Type,
}

impl Macro {
    fn try_generate(&self, mut impl_item: syn::ItemImpl) -> syn::Result<TokenStream> {
        let mut member_items = std::mem::take(&mut impl_item.items);
        let member_idents: HashSet<_> = member_items
            .iter()
            .filter_map(|item| match item {
                syn::ImplItem::Const(it) => Some(it.ident.clone()),
                syn::ImplItem::Fn(it) => Some(it.sig.ident.clone()),
                syn::ImplItem::Type(it) => Some(it.ident.clone()),
                syn::ImplItem::Macro(_) => None,
                syn::ImplItem::Verbatim(_) => None,
                _ => None,
            })
            .collect();
        for member_item in &mut member_items {
            self.modify_member(&impl_item, member_item, &member_idents)?;
        }
        impl_item.items = member_items;
        Ok(crate::debug::dump_tokens(
            format!("impl {:?}", impl_item.self_ty),
            impl_item.into_token_stream(),
        ))
    }

    #[allow(non_snake_case)]
    fn modify_member(
        &self,
        impl_item: &syn::ItemImpl,
        member_item: &mut syn::ImplItem,
        member_idents: &HashSet<syn::Ident>,
    ) -> syn::Result<()> {
        let syn::ImplItem::Fn(fn_item) = member_item else {
            return Ok(());
        };

        let self_ty = &*impl_item.self_ty;

        let Some(tracked_attr_index) = fn_item.attrs.iter().position(|a| self.is_tracked_attr(a))
        else {
            return Ok(());
        };

        let trait_ = match &impl_item.trait_ {
            Some((path, _)) if impl_item.modifiers.polarity.is_none() => {
                Some((path, member_idents))
            }
            _ => None,
        };
        let mut change = ChangeSelfPath::new(self_ty, trait_);
        change.visit_impl_item_fn_mut(fn_item);

        let mut salsa_tracked_attr = fn_item.attrs.remove(tracked_attr_index);
        let mut args: FnArgs = match &salsa_tracked_attr.meta {
            syn::Meta::Path(..) => Default::default(),
            _ => salsa_tracked_attr.parse_args()?,
        };
        if let Some(configuration) = &args.configuration {
            return Err(syn::Error::new_spanned(
                &configuration.name,
                "`configuration` is only supported on free tracked functions, not tracked methods or associated functions",
            ));
        }
        let InnerTrait = self.hygiene.ident("InnerTrait");
        let inner_fn_name = self.hygiene.ident(fn_item.sig.ident.to_string());

        let AssociatedFunctionArguments {
            self_token,
            db_input_index,
            db_ty,
            db_ident,
            db_lt,
            has_explicit_db_lt,
            input_ids,
            input_tys,
            output_ty,
        } = self.validity_check(impl_item, fn_item)?;
        // We do not rename the database lifetime: the inner function carries the
        // user's original body, which may refer to the lifetime by name. Instead
        // we normalize elided lifetimes (`'_`) to the (possibly conjured) `db_lt`.
        let body_self_ty = ChangeLt::elided_to(&db_lt).in_type(self_ty);
        let db_ty = ChangeLt::elided_to(&db_lt).in_type(&self.with_db_lifetime(db_ty, &db_lt));
        let skipped_inputs = if self_token.is_some() { 2 } else { 1 };

        if args.self_ty.is_none() {
            args.self_ty = Some(body_self_ty.clone());
        }
        salsa_tracked_attr.meta = syn::Meta::List(syn::MetaList {
            path: salsa_tracked_attr.path().clone(),
            delimiter: syn::MacroDelimiter::Paren(syn::token::Paren::default()),
            tokens: quote! {#args},
        });

        let mut inner_fn = fn_item.clone();
        inner_fn.vis = syn::Visibility::Inherited;
        inner_fn.sig.ident = inner_fn_name.clone();
        self.normalize_signature_lifetimes(
            &mut inner_fn.sig,
            db_input_index,
            skipped_inputs,
            &db_lt,
        )?;

        let tracked_fn: syn::ItemFn = if self_token.is_some() {
            parse_quote! {
                #salsa_tracked_attr
                fn #inner_fn_name<#db_lt>(db: #db_ty, this: #body_self_ty, #(#input_ids: #input_tys),*) -> #output_ty {
                    <#body_self_ty as #InnerTrait>::#inner_fn_name(this, db, #(#input_ids),*)
                }
            }
        } else {
            parse_quote! {
                #salsa_tracked_attr
                fn #inner_fn_name<#db_lt>(db: #db_ty, #(#input_ids: #input_tys),*) -> #output_ty {
                    <#body_self_ty as #InnerTrait>::#inner_fn_name(db, #(#input_ids),*)
                }
            }
        };

        // Construct the body of the method or associated function

        let block = if let Some(self_token) = self_token {
            parse_quote!({
                salsa::plumbing::setup_tracked_method_body! {
                    salsa_tracked_attr: #salsa_tracked_attr,
                    self: #self_token,
                    self_ty: #body_self_ty,
                    db_lt: #db_lt,
                    db: #db_ident,
                    db_ty: (#db_ty),
                    input_ids: [#(#input_ids),*],
                    input_tys: [#(#input_tys),*],
                    output_ty: #output_ty,
                    inner_fn_name: #inner_fn_name,
                    inner_fn: #inner_fn,
                    tracked_fn: #tracked_fn,

                    // Annoyingly macro-rules hygiene does not extend to items defined in the macro.
                    // We have the procedural macro generate names for those items that are
                    // not used elsewhere in the user's code.
                    unused_names: [
                        #InnerTrait,
                    ]
                }
            })
        } else {
            parse_quote!({
                salsa::plumbing::setup_tracked_assoc_fn_body! {
                    salsa_tracked_attr: #salsa_tracked_attr,
                    self_ty: #body_self_ty,
                    db_lt: #db_lt,
                    db: #db_ident,
                    db_ty: (#db_ty),
                    input_ids: [#(#input_ids),*],
                    input_tys: [#(#input_tys),*],
                    output_ty: #output_ty,
                    inner_fn_name: #inner_fn_name,
                    inner_fn: #inner_fn,
                    tracked_fn: #tracked_fn,

                    // Annoyingly macro-rules hygiene does not extend to items defined in the macro.
                    // We have the procedural macro generate names for those items that are
                    // not used elsewhere in the user's code.
                    unused_names: [
                        #InnerTrait,
                    ]
                }
            })
        };

        // Update the method that will actually appear in the impl to have the new body
        // and its true return type.
        let outer_db_lt = if self.return_mode_uses_db_lifetime(&args)
            || self.return_type_uses_elided_lifetime(&fn_item.sig.output)
            || self.inputs_use_elided_lifetime(&fn_item.sig, skipped_inputs)
        {
            Some(db_lt.clone())
        } else {
            None
        };

        if let Some(outer_db_lt) = &outer_db_lt {
            if !has_explicit_db_lt {
                fn_item.sig.generics.params.push(parse_quote!(#outer_db_lt));
            }
            self.update_db_argument_lifetime(&mut fn_item.sig, db_input_index, outer_db_lt)?;
            self.update_input_lifetimes(&mut fn_item.sig, skipped_inputs, outer_db_lt)?;
        }

        self.update_input_patterns(&mut fn_item.sig, skipped_inputs, &input_ids)?;
        self.update_return_type(&mut fn_item.sig, &args, outer_db_lt.as_ref())?;
        fn_item.block = block;

        Ok(())
    }

    fn validity_check<'syn>(
        &self,
        impl_item: &'syn syn::ItemImpl,
        fn_item: &'syn syn::ImplItemFn,
    ) -> syn::Result<AssociatedFunctionArguments<'syn>> {
        let explicit_db_lt = self.extract_db_lifetime(impl_item, fn_item)?;
        let db_lt = match explicit_db_lt {
            Some(db_lt) => db_lt.clone(),
            None => self.conjure_db_lifetime(impl_item, fn_item),
        };

        // Use `first()` so a zero-argument method doesn't panic with
        // "index out of bounds" here — the downstream `.get(db_input_index)`
        // guard plus `check_db_argument`/`check_self_argument` already
        // produce a clean compile error in that case.
        let is_method = fn_item
            .sig
            .inputs
            .first()
            .is_some_and(|arg| matches!(arg, syn::FnArg::Receiver(_)));

        let (self_token, db_input_index, skipped_inputs) = if is_method {
            (Some(self.check_self_argument(fn_item)?), 1, 2)
        } else {
            (None, 0, 1)
        };

        let db_arg = fn_item.sig.inputs.get(db_input_index).ok_or_else(|| {
            syn::Error::new_spanned(
                &fn_item.sig,
                "tracked methods must have a database parameter after `self`",
            )
        })?;
        let (db_ident, db_ty) = self.check_db_argument(db_arg)?;

        let input_ids: Vec<syn::Ident> =
            crate::fn_util::input_ids(&self.hygiene, &fn_item.sig, skipped_inputs);
        let input_tys = crate::fn_util::input_tys(&fn_item.sig, skipped_inputs)?
            .into_iter()
            .map(|ty| ChangeLt::elided_to(&db_lt).in_type(ty))
            .collect();
        let output_ty = crate::fn_util::output_ty(Some(&db_lt), &fn_item.sig)?;

        Ok(AssociatedFunctionArguments {
            self_token,
            db_input_index,
            db_ident,
            db_lt,
            has_explicit_db_lt: explicit_db_lt.is_some(),
            db_ty,
            input_ids,
            input_tys,
            output_ty,
        })
    }

    fn is_tracked_attr(&self, attr: &syn::Attribute) -> bool {
        if attr.path().segments.len() != 2 {
            return false;
        }

        let seg0 = &attr.path().segments[0];
        let seg1 = &attr.path().segments[1];

        seg0.ident == "salsa"
            && seg1.ident == "tracked"
            && seg0.arguments.is_empty()
            && seg1.arguments.is_empty()
    }

    fn extract_db_lifetime<'syn>(
        &self,
        impl_item: &'syn syn::ItemImpl,
        fn_item: &'syn syn::ImplItemFn,
    ) -> syn::Result<Option<&'syn syn::Lifetime>> {
        // Either the impl XOR the fn can have generics, and it must be at most a lifetime
        let mut db_lt = None;
        for param in impl_item
            .generics
            .params
            .iter()
            .chain(fn_item.sig.generics.params.iter())
        {
            match param {
                syn::GenericParam::Lifetime(lt) => {
                    if db_lt.is_none() {
                        if let Some(bound) = lt.bounds.iter().next() {
                            return Err(syn::Error::new_spanned(
                                bound,
                                "lifetime parameters on tracked methods must not have bounds",
                            ));
                        }

                        db_lt = Some(&lt.lifetime);
                    } else {
                        return Err(syn::Error::new_spanned(
                            param,
                            "tracked method already has a lifetime parameter in scope",
                        ));
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        param,
                        "tracked methods cannot have non-lifetime generic parameters",
                    ));
                }
            }
        }

        Ok(db_lt)
    }

    /// Conjure a database lifetime name for a method that did not declare one.
    ///
    /// We default to `'db`, but fall back to `'db1`, `'db2`, ... if that name is
    /// already used elsewhere in the signature or self type (for example by a
    /// higher-ranked `for<'db>` binder), to avoid shadowing it.
    fn conjure_db_lifetime(
        &self,
        impl_item: &syn::ItemImpl,
        fn_item: &syn::ImplItemFn,
    ) -> syn::Lifetime {
        let mut used = UsedLifetimes(HashSet::new());
        used.visit_type(&impl_item.self_ty);
        used.visit_signature(&fn_item.sig);

        let span = fn_item.sig.ident.span();
        let mut name = String::from("db");
        let mut counter = 1;
        while used.0.contains(&name) {
            counter += 1;
            name = format!("db{counter}");
        }
        syn::Lifetime::new(&format!("'{name}"), span)
    }

    fn check_self_argument<'syn>(
        &self,
        fn_item: &'syn syn::ImplItemFn,
    ) -> syn::Result<&'syn syn::token::SelfValue> {
        if fn_item.sig.inputs.is_empty() {
            return Err(syn::Error::new_spanned(
                &fn_item.sig.ident,
                "tracked methods must have arguments",
            ));
        }

        let syn::FnArg::Receiver(receiver) = &fn_item.sig.inputs[0] else {
            return Err(syn::Error::new_spanned(
                &fn_item.sig.inputs[0],
                "tracked methods must take a `self` argument",
            ));
        };

        match &receiver.kind {
            syn::ReceiverKind::Value => Ok(&receiver.self_token),
            syn::ReceiverKind::Typed(colon_token, _) => Err(syn::Error::new_spanned(
                colon_token,
                "tracked method's `self` argument must not have an explicit type",
            )),
            syn::ReceiverKind::Reference(and_token, _, _) => Err(syn::Error::new_spanned(
                and_token,
                "tracked methods's first argument must be declared as `self`, not `&self` or `&mut self`",
            )),
            _ => Err(syn::Error::new_spanned(
                receiver,
                "tracked methods's first argument must be declared as `self`, not `&self` or `&mut self`",
            )),
        }
    }

    fn check_db_argument<'syn>(
        &self,
        input: &'syn syn::FnArg,
    ) -> syn::Result<(&'syn syn::Ident, &'syn syn::Type)> {
        let syn::FnArg::Typed(typed) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "tracked methods must take a database parameter",
            ));
        };

        let syn::Pat::Ident(db_pat_ident) = &*typed.pat else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "database parameter must have a simple name",
            ));
        };

        let db_ident = &db_pat_ident.ident;
        let db_ty = &*typed.ty;

        Ok((db_ident, db_ty))
    }

    fn with_db_lifetime(&self, db_ty: &syn::Type, db_lt: &syn::Lifetime) -> syn::Type {
        let mut db_ty = db_ty.clone();
        self.set_db_lifetime(&mut db_ty, db_lt);
        db_ty
    }

    fn update_db_argument_lifetime(
        &self,
        sig: &mut syn::Signature,
        db_input_index: usize,
        db_lt: &syn::Lifetime,
    ) -> syn::Result<()> {
        let Some(input) = sig.inputs.iter_mut().nth(db_input_index) else {
            return Err(syn::Error::new_spanned(
                &sig.ident,
                "tracked methods must take a database parameter",
            ));
        };

        let syn::FnArg::Typed(typed) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "tracked methods must take a database parameter",
            ));
        };

        self.set_db_lifetime(typed.ty.as_mut(), db_lt);
        Ok(())
    }

    fn set_db_lifetime(&self, db_ty: &mut syn::Type, db_lt: &syn::Lifetime) {
        let syn::Type::Reference(reference) = db_ty else {
            return;
        };

        match &reference.lifetime {
            Some(lifetime) if lifetime.ident != "_" => {}
            _ => reference.lifetime = Some(db_lt.clone()),
        }
    }

    /// Normalizes the signature of the inner (user body) function so that all of
    /// its elided lifetimes (`'_`) refer to the database lifetime `db_lt`.
    ///
    /// We deliberately replace only elided lifetimes; explicit lifetime names
    /// the user wrote are left intact so that the body — which may refer to them
    /// — still compiles.
    fn normalize_signature_lifetimes(
        &self,
        sig: &mut syn::Signature,
        db_input_index: usize,
        skipped_inputs: usize,
        db_lt: &syn::Lifetime,
    ) -> syn::Result<()> {
        self.update_db_argument_lifetime(sig, db_input_index, db_lt)?;
        if let Some(input) = sig.inputs.iter_mut().nth(db_input_index) {
            let syn::FnArg::Typed(typed) = input else {
                return Err(syn::Error::new_spanned(
                    input,
                    "tracked methods must take a database parameter",
                ));
            };
            *typed.ty = ChangeLt::elided_to(db_lt).in_type(&typed.ty);
        }
        for input in sig.inputs.iter_mut().skip(skipped_inputs) {
            let syn::FnArg::Typed(typed) = input else {
                return Err(syn::Error::new_spanned(input, "unexpected receiver"));
            };
            *typed.ty = ChangeLt::elided_to(db_lt).in_type(&typed.ty);
        }
        if let syn::ReturnType::Type(_, ty) = &mut sig.output {
            **ty = ChangeLt::elided_to(db_lt).in_type(ty);
        }
        Ok(())
    }

    fn update_input_lifetimes(
        &self,
        sig: &mut syn::Signature,
        skipped_inputs: usize,
        db_lt: &syn::Lifetime,
    ) -> syn::Result<()> {
        for input in sig.inputs.iter_mut().skip(skipped_inputs) {
            let syn::FnArg::Typed(typed) = input else {
                return Err(syn::Error::new_spanned(input, "unexpected receiver"));
            };
            *typed.ty = ChangeLt::elided_to(db_lt).in_type(&typed.ty);
        }
        Ok(())
    }

    fn update_input_patterns(
        &self,
        sig: &mut syn::Signature,
        skipped_inputs: usize,
        input_ids: &[syn::Ident],
    ) -> syn::Result<()> {
        for (input, input_id) in sig.inputs.iter_mut().skip(skipped_inputs).zip(input_ids) {
            let syn::FnArg::Typed(typed) = input else {
                return Err(syn::Error::new_spanned(input, "unexpected receiver"));
            };

            let syn::Pat::Ident(ident) = &*typed.pat else {
                *typed.pat = self.ident_pat(input_id);
                continue;
            };

            if ident.ident != *input_id {
                *typed.pat = self.ident_pat(input_id);
            }
        }
        Ok(())
    }

    fn ident_pat(&self, ident: &syn::Ident) -> syn::Pat {
        syn::Pat::Ident(syn::PatIdent {
            attrs: vec![],
            by_ref: None,
            mutability: None,
            ident: ident.clone(),
            subpat: None,
        })
    }

    fn return_mode_uses_db_lifetime(&self, args: &FnArgs) -> bool {
        if let Some(returns) = &args.returns {
            returns == "ref" || returns == "deref" || returns == "as_ref" || returns == "as_deref"
        } else {
            true
        }
    }

    fn return_type_uses_elided_lifetime(&self, output: &syn::ReturnType) -> bool {
        let syn::ReturnType::Type(_, ty) = output else {
            return false;
        };
        crate::xform::uses_elided_lifetime(ty)
    }

    fn inputs_use_elided_lifetime(&self, sig: &syn::Signature, skipped_inputs: usize) -> bool {
        sig.inputs.iter().skip(skipped_inputs).any(|input| {
            matches!(input, syn::FnArg::Typed(typed) if crate::xform::uses_elided_lifetime(&typed.ty))
        })
    }

    fn update_return_type(
        &self,
        sig: &mut syn::Signature,
        args: &FnArgs,
        db_lt: Option<&syn::Lifetime>,
    ) -> syn::Result<()> {
        if let Some(returns) = &args.returns {
            if let syn::ReturnType::Type(_, t) = &mut sig.output {
                if returns == "copy" || returns == "clone" {
                    if let Some(db_lt) = db_lt {
                        **t = ChangeLt::elided_to(db_lt).in_type(t);
                    }
                } else if returns == "ref" {
                    let ty = db_lt
                        .map(|db_lt| ChangeLt::elided_to(db_lt).in_type(t))
                        .unwrap_or_else(|| syn::Type::clone(t));
                    **t = parse_quote!(& #db_lt #ty)
                } else if returns == "deref" {
                    let ty = db_lt
                        .map(|db_lt| ChangeLt::elided_to(db_lt).in_type(t))
                        .unwrap_or_else(|| syn::Type::clone(t));
                    **t = parse_quote!(& #db_lt <#ty as ::core::ops::Deref>::Target)
                } else if returns == "as_ref" {
                    let ty = db_lt
                        .map(|db_lt| ChangeLt::elided_to(db_lt).in_type(t))
                        .unwrap_or_else(|| syn::Type::clone(t));
                    **t = parse_quote!(<#ty as ::salsa::SalsaAsRef>::AsRef<#db_lt>)
                } else if returns == "as_deref" {
                    let ty = db_lt
                        .map(|db_lt| ChangeLt::elided_to(db_lt).in_type(t))
                        .unwrap_or_else(|| syn::Type::clone(t));
                    **t = parse_quote!(<#ty as ::salsa::SalsaAsDeref>::AsDeref<#db_lt>)
                } else {
                    return Err(syn::Error::new_spanned(
                        returns,
                        format!("Unknown returns mode `{returns}`"),
                    ));
                }
            } else {
                return Err(syn::Error::new_spanned(
                    returns,
                    "returns attribute requires explicit return type",
                ));
            }
        } else {
            let db_lt = db_lt.expect("the default ref return mode uses the database lifetime");
            if let syn::ReturnType::Type(_, t) = &mut sig.output {
                let ty = ChangeLt::elided_to(db_lt).in_type(t);
                **t = parse_quote!(& #db_lt #ty);
            } else {
                sig.output = parse_quote!(-> & #db_lt ());
            }
        }
        Ok(())
    }
}

/// Collects the names of every lifetime mentioned in the visited syntax.
struct UsedLifetimes(HashSet<String>);

impl<'ast> syn::visit::Visit<'ast> for UsedLifetimes {
    fn visit_lifetime(&mut self, i: &'ast syn::Lifetime) {
        self.0.insert(i.ident.to_string());
    }
}
