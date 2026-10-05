use std::cmp::Ordering;

use proc_macro2::{Literal, Span, TokenStream};
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::{Ident, ItemFn};

use crate::hygiene::Hygiene;
use crate::options::{AllowedOptions, AllowedPersistOptions, Options};
use crate::xform::ChangeLt;
use crate::{db_lifetime, fn_util};

// Source:
//
// #[salsa::db]
// pub struct Database {
//    storage: salsa::Storage<Self>,
// }

pub(crate) fn tracked_fn(args: proc_macro::TokenStream, item: ItemFn) -> syn::Result<TokenStream> {
    let hygiene = Hygiene::from2(&item);
    let args: FnArgs = syn::parse(args)?;
    let db_macro = Macro { hygiene, args };
    db_macro.try_fn(item)
}

pub type FnArgs = Options<TrackedFn>;

pub struct TrackedFn;

impl AllowedOptions for TrackedFn {
    const RETURNS: bool = true;

    const SPECIFY: bool = true;

    const NO_EQ: bool = true;

    const DEBUG: bool = false;

    const NO_LIFETIME: bool = false;

    const NON_SALSA_VALUES: bool = true;

    const SINGLETON: bool = false;

    const DATA: bool = false;

    const DB: bool = false;

    const CYCLE_FN: bool = true;
    const ATTEMPT: bool = true;
    const CONFIGURATION: bool = true;

    const CYCLE_INITIAL: bool = true;

    const CYCLE_RESULT: bool = true;

    const LRU: bool = true;

    const CONSTRUCTOR_NAME: bool = false;

    const ID: bool = false;

    const REVISIONS: bool = false;

    const HEAP_SIZE: bool = true;

    const SELF_TY: bool = true;

    const PERSIST: AllowedPersistOptions = AllowedPersistOptions::AllowedIdent;
}

struct Macro {
    hygiene: Hygiene,
    args: FnArgs,
}

struct ValidFn<'item> {
    db_ident: &'item syn::Ident,
    db_path: &'item syn::Path,
}

const ALLOWED_RETURN_MODES: &[&str] = &["copy", "clone", "ref", "deref", "as_ref", "as_deref"];

#[allow(non_snake_case)]
impl Macro {
    fn try_fn(&self, item: syn::ItemFn) -> syn::Result<TokenStream> {
        let ValidFn { db_ident, db_path } = self.validity_check(&item)?;

        let attrs = &item.attrs;
        let fn_name = &item.sig.ident;
        let vis = &item.vis;
        let db_lt = db_lifetime::db_lifetime(&item.sig.generics);
        let input_ids = self.input_ids(&item);
        let input_tys = self.input_tys(&item)?;
        let with_db_lifetime = |ty: &syn::Type| ChangeLt::elided_to(&db_lt).in_type(ty);
        let interned_input_tys = input_tys
            .iter()
            .map(|&ty| with_db_lifetime(ty))
            .collect::<Vec<_>>();
        let output_ty = with_db_lifetime(&self.output_ty(&db_lt, &item)?);
        let (cycle_recovery_fn, cycle_recovery_initial, cycle_recovery_strategy) =
            self.cycle_recovery()?;
        let is_specifiable = self.args.specify.is_some();
        let attempt_policy = self
            .args
            .attempt
            .clone()
            .unwrap_or_else(|| Ident::new("Unclassified", Span::call_site()));
        let requires_salsa_value = self.args.non_salsa_values.is_none();
        let heap_size_fn = self.args.heap_size_fn.iter();
        let eq = if let Some(token) = &self.args.no_eq {
            if self.args.cycle_fn.is_some() {
                return Err(syn::Error::new_spanned(
                    token,
                    "the `no_eq` option cannot be used with `cycle_fn`",
                ));
            }
            quote!(false)
        } else {
            quote_spanned!(output_ty.span() =>
                old_value == new_value
            )
        };
        // we need to generate the entire function here
        // as the locals (parameters) will have def site hygiene otherwise
        // if emitted in the decl macro
        let eq = quote! {
            fn values_equal<#db_lt>(
                old_value: &Self::Output<#db_lt>,
                new_value: &Self::Output<#db_lt>,
            ) -> bool {
                #eq
            }
        };

        let mut inner_fn = item.clone();
        inner_fn.vis = syn::Visibility::Inherited;
        inner_fn.sig.ident = self.hygiene.ident("inner");

        let zalsa = self.hygiene.ident("zalsa");
        let Configuration = self.hygiene.scoped_ident(fn_name, "Configuration");
        let InternedData = self.hygiene.scoped_ident(fn_name, "InternedData");
        let FN_CACHE = self.hygiene.scoped_ident(fn_name, "FN_CACHE");
        let INTERN_CACHE = self.hygiene.scoped_ident(fn_name, "INTERN_CACHE");
        let inner = &inner_fn.sig.ident;

        let (marker_vis, configuration_vis, configuration_alias, configuration_impl) = if let Some(
            configuration,
        ) =
            &self.args.configuration
        {
            let alias_vis = &configuration.visibility;
            let alias = &configuration.name;
            let visibility = wider_configuration_visibility(vis, alias_vis)?;
            (
                visibility.clone(),
                visibility.clone(),
                quote! {
                    #[doc(hidden)]
                    #alias_vis type #alias =
                        <#fn_name as ::salsa::plumbing::TrackedFunctionConfiguration>::Configuration;
                },
                quote! {
                    impl ::salsa::plumbing::TrackedFunctionConfiguration for #fn_name {
                        type Configuration = #Configuration;
                    }
                },
            )
        } else {
            (
                vis.clone(),
                syn::Visibility::Inherited,
                quote! {},
                quote! {},
            )
        };

        let function_type = function_type(&item);

        if is_specifiable {
            match function_type {
                FunctionType::Constant | FunctionType::RequiresInterning => {
                    return Err(syn::Error::new_spanned(
                        self.args.specify.as_ref().unwrap(),
                        "only functions with a single salsa struct as their input can be specified",
                    ));
                }
                FunctionType::SalsaStruct => {}
            }
        }

        if let (Some(_), Some(token)) = (&self.args.lru, &self.args.specify) {
            return Err(syn::Error::new_spanned(
                token,
                "the `specify` and `lru` options cannot be used together",
            ));
        }

        let needs_interner = match function_type {
            FunctionType::Constant | FunctionType::RequiresInterning => true,
            FunctionType::SalsaStruct => false,
        };

        let lru = Literal::usize_unsuffixed(self.args.lru.unwrap_or(0));

        // Determine the eviction policy type based on whether LRU capacity is specified
        let eviction_type = if self.args.lru.is_some() {
            quote!(::salsa::plumbing::function::Lru)
        } else {
            quote!(::salsa::plumbing::function::NoopEviction)
        };

        let return_mode = self
            .args
            .returns
            .clone()
            .unwrap_or(Ident::new("ref", Span::call_site()));

        // Validate return mode
        if !ALLOWED_RETURN_MODES
            .iter()
            .any(|mode| mode == &return_mode.to_string())
        {
            return Err(syn::Error::new(
                return_mode.span(),
                format!("Invalid return mode. Allowed modes are: {ALLOWED_RETURN_MODES:?}"),
            ));
        }

        let persist = self.args.persist();

        let assert_output_is_salsa_value_or_static = if requires_salsa_value {
            crate::salsa_value::assert_salsa_value_or_static(&db_lt, &zalsa, &output_ty)
        } else {
            quote! {}
        };
        let assert_interned_inputs_are_salsa_values = if requires_salsa_value {
            interned_input_tys
                .iter()
                .map(|input_ty| {
                    crate::salsa_value::assert_salsa_value_field(&db_lt, &zalsa, input_ty, false)
                })
                .collect()
        } else {
            quote! {}
        };
        let self_ty = match &self.args.self_ty {
            Some(ty) => quote! { self_ty: #ty, },
            None => quote! {},
        };

        Ok(crate::debug::dump_tokens(
            fn_name,
            quote![salsa::plumbing::setup_tracked_fn! {
                attrs: [#(#attrs),*],
                vis: #vis,
                marker_vis: #marker_vis,
                configuration_vis: #configuration_vis,
                configuration_alias: { #configuration_alias },
                configuration_impl: { #configuration_impl },
                fn_name: #fn_name,
                db_lt: #db_lt,
                Db: #db_path,
                db: #db_ident,
                input_ids: [#(#input_ids),*],
                input_tys: [#(#input_tys),*],
                interned_input_tys: [#(#interned_input_tys),*],
                output_ty: #output_ty,
                inner_fn: { #inner_fn },
                cycle_recovery_fn: #cycle_recovery_fn,
                cycle_recovery_initial: #cycle_recovery_initial,
                cycle_recovery_strategy: #cycle_recovery_strategy,
                attempt_policy: #attempt_policy,
                is_specifiable: #is_specifiable,
                values_equal: {#eq},
                needs_interner: #needs_interner,
                heap_size_fn: #(#heap_size_fn)*,
                eviction: #eviction_type,
                lru: #lru,
                return_mode: #return_mode,
                persist: #persist,
                assert_interned_inputs_are_salsa_values: { #assert_interned_inputs_are_salsa_values },
                assert_output_is_salsa_value_or_static: { #assert_output_is_salsa_value_or_static },
                #self_ty
                unused_names: [
                    #zalsa,
                    #Configuration,
                    #InternedData,
                    #FN_CACHE,
                    #INTERN_CACHE,
                    #inner,
                ]
            }],
        ))
    }

    fn validity_check<'item>(&self, item: &'item syn::ItemFn) -> syn::Result<ValidFn<'item>> {
        db_lifetime::require_optional_db_lifetime(&item.sig.generics)?;

        if item.sig.inputs.is_empty() {
            return Err(syn::Error::new_spanned(
                &item.sig.ident,
                "tracked functions must have at least a database argument",
            ));
        }

        let (db_ident, db_path) =
            check_db_argument(&item.sig.inputs[0], item.sig.generics.lifetimes().next())?;

        Ok(ValidFn { db_ident, db_path })
    }

    fn cycle_recovery(&self) -> syn::Result<(TokenStream, TokenStream, TokenStream)> {
        // TODO should we ask the user to specify a struct that impls a trait with two methods,
        // rather than asking for two methods separately?
        match (
            &self.args.cycle_fn,
            &self.args.cycle_initial,
            &self.args.cycle_result,
        ) {
            (Some(cycle_fn), Some(cycle_initial), None) => Ok((
                quote!(((#cycle_fn))),
                quote!(((#cycle_initial))),
                quote!(Fixpoint),
            )),
            (None, None, None) => Ok((
                quote!((salsa::plumbing::unexpected_cycle_recovery!)),
                quote!((salsa::plumbing::unexpected_cycle_initial!)),
                quote!(Panic),
            )),
            (Some(_), None, None) => Err(syn::Error::new_spanned(
                self.args.cycle_fn.as_ref().unwrap(),
                "must provide `cycle_initial` along with `cycle_fn`",
            )),
            (None, Some(cycle_initial), None) => Ok((
                quote!((salsa::plumbing::unexpected_cycle_recovery!)),
                quote!(((#cycle_initial))),
                quote!(Fixpoint),
            )),
            (None, None, Some(cycle_result)) => Ok((
                quote!((salsa::plumbing::unexpected_cycle_recovery!)),
                quote!(((#cycle_result))),
                quote!(FallbackImmediate),
            )),
            (_, _, Some(_)) => Err(syn::Error::new_spanned(
                self.args.cycle_initial.as_ref().unwrap(),
                "must provide either `cycle_result` or `cycle_fn` & `cycle_initial`, not both",
            )),
        }
    }

    fn input_ids(&self, item: &ItemFn) -> Vec<syn::Ident> {
        fn_util::input_ids(&self.hygiene, &item.sig, 1)
    }

    fn input_tys<'syn>(&self, item: &'syn ItemFn) -> syn::Result<Vec<&'syn syn::Type>> {
        fn_util::input_tys(&item.sig, 1)
    }

    fn output_ty(&self, db_lt: &syn::Lifetime, item: &syn::ItemFn) -> syn::Result<syn::Type> {
        fn_util::output_ty(Some(db_lt), &item.sig)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum FunctionType {
    Constant,
    SalsaStruct,
    RequiresInterning,
}

fn function_type(item_fn: &syn::ItemFn) -> FunctionType {
    match item_fn.sig.inputs.len() {
        0 => unreachable!(
            "functions have been checked to have at least a database argument by this point"
        ),
        1 => FunctionType::Constant,
        2 => FunctionType::SalsaStruct,
        _ => FunctionType::RequiresInterning,
    }
}

pub fn check_db_argument<'arg>(
    fn_arg: &'arg syn::FnArg,
    explicit_lt: Option<&'arg syn::LifetimeParam>,
) -> syn::Result<(&'arg syn::Ident, &'arg syn::Path)> {
    match fn_arg {
        syn::FnArg::Receiver(_) => {
            // If we see `&self` where a database was expected, that indicates
            // that `#[tracked]` was applied to a method.
            Err(syn::Error::new_spanned(
                fn_arg,
                "#[salsa::tracked] must also be applied to the impl block for tracked methods",
            ))
        }
        syn::FnArg::Typed(typed) => {
            let syn::Pat::Ident(db_pat_ident) = &*typed.pat else {
                return Err(syn::Error::new_spanned(
                    &typed.pat,
                    "database parameter must have a simple name",
                ));
            };

            let syn::PatIdent {
                attrs,
                by_ref,
                mutability,
                ident: db_ident,
                subpat,
            } = db_pat_ident;

            if !attrs.is_empty() {
                return Err(syn::Error::new_spanned(
                    db_pat_ident,
                    "database parameter cannot have attributes",
                ));
            }

            if by_ref.is_some() {
                return Err(syn::Error::new_spanned(
                    by_ref,
                    "database parameter cannot be borrowed",
                ));
            }

            if mutability.is_some() {
                return Err(syn::Error::new_spanned(
                    mutability,
                    "database parameter cannot be mutable",
                ));
            }

            if let Some((at, _)) = subpat {
                return Err(syn::Error::new_spanned(
                    at,
                    "database parameter cannot have a subpattern",
                ));
            }

            let tykind_error_msg =
                "must have type `&dyn Db`, where `Db` is some Salsa Database trait";

            let syn::Type::Reference(ref_type) = &*typed.ty else {
                return Err(syn::Error::new(typed.ty.span(), tykind_error_msg));
            };

            if let Some(lt) = explicit_lt {
                if ref_type.lifetime.is_none() {
                    return Err(syn::Error::new_spanned(
                        ref_type.and_token,
                        format!("must have a `{}` lifetime", lt.lifetime.to_token_stream()),
                    ));
                }
            }

            let extract_db_path = || -> Result<&'arg syn::Path, Span> {
                if let Some(m) = &ref_type.mutability {
                    return Err(m.span());
                }

                let syn::Type::TraitObject(d) = &*ref_type.elem else {
                    return Err(ref_type.span());
                };

                if d.bounds.len() != 1 {
                    return Err(d.span());
                }

                let syn::TypeParamBound::Trait(syn::TraitBound {
                    paren_token,
                    lifetimes,
                    modifiers,
                    maybe,
                    path,
                }) = &d.bounds[0]
                else {
                    return Err(d.span());
                };

                if let Some(p) = paren_token {
                    return Err(p.span.open());
                }

                if modifiers.require_empty().is_err() || maybe.is_some() {
                    return Err(d.span());
                }

                if let Some(lt) = lifetimes {
                    return Err(lt.span());
                }

                Ok(path)
            };

            let db_path =
                extract_db_path().map_err(|span| syn::Error::new(span, tykind_error_msg))?;

            Ok((db_ident, db_path))
        }
    }
}

/// Returns the wider scope written on the function and its configuration alias.
///
/// Both visibilities must remain reachable: narrowing the existing marker would break
/// callers using its HasJar implementation even when the alias itself is private.
fn wider_configuration_visibility<'a>(
    function: &'a syn::Visibility,
    alias: &'a syn::Visibility,
) -> syn::Result<&'a syn::Visibility> {
    let ordering = match (VisibilityScope::of(function)?, VisibilityScope::of(alias)?) {
        (VisibilityScope::Public, _) | (_, VisibilityScope::Here) => Ordering::Greater,
        (_, VisibilityScope::Public) | (VisibilityScope::Here, _) => Ordering::Less,
        (VisibilityScope::Ancestors(left), VisibilityScope::Ancestors(right)) => left.cmp(&right),
        (VisibilityScope::CratePath(left), VisibilityScope::CratePath(right))
            if left
                .segments
                .iter()
                .zip(&right.segments)
                .all(|(a, b)| a.ident == b.ident) =>
        {
            right.segments.len().cmp(&left.segments.len())
        }
        (VisibilityScope::CratePath(path), _) if path.segments.len() == 1 => Ordering::Greater,
        (_, VisibilityScope::CratePath(path)) if path.segments.len() == 1 => Ordering::Less,
        _ => {
            return Err(syn::Error::new_spanned(
                alias,
                "cannot compare function and configuration visibility; use matching or ancestor-related paths with the same `crate` or `super` base",
            ));
        }
    };
    Ok(if ordering == Ordering::Less {
        alias
    } else {
        function
    })
}

#[derive(Debug)]
enum VisibilityScope<'a> {
    Here,
    Public,
    CratePath(&'a syn::Path),
    Ancestors(usize),
}

impl<'a> VisibilityScope<'a> {
    fn of(visibility: &'a syn::Visibility) -> syn::Result<Self> {
        match visibility {
            syn::Visibility::Inherited => Ok(Self::Here),
            syn::Visibility::Public(_) => Ok(Self::Public),
            syn::Visibility::Restricted(restricted) => {
                let path = &restricted.path;
                if path.is_ident("self") {
                    Ok(Self::Here)
                } else if path
                    .segments
                    .first()
                    .is_some_and(|segment| segment.ident == "crate")
                {
                    Ok(Self::CratePath(path))
                } else if path.segments.iter().all(|segment| segment.ident == "super") {
                    Ok(Self::Ancestors(path.segments.len()))
                } else {
                    Err(syn::Error::new_spanned(
                        visibility,
                        "configuration visibility requires `self`, `crate`, a `crate::...` path, or a `super` ancestor path",
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod configuration_tests {
    use quote::ToTokens;

    use super::{FnArgs, wider_configuration_visibility};

    #[test]
    fn configuration_option_round_trips() -> syn::Result<()> {
        for option in [
            "configuration = (PrivateConfiguration)",
            "configuration = (pub PublicConfiguration)",
            "configuration = (pub(in crate::queries) RestrictedConfiguration)",
        ] {
            let args: FnArgs = syn::parse_str(option)?;
            let reparsed: FnArgs = syn::parse2(args.to_token_stream())?;
            let Some(original) = args.configuration else {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "missing original configuration",
                ));
            };
            let Some(reparsed) = reparsed.configuration else {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "missing reparsed configuration",
                ));
            };
            assert_eq!(original.name, reparsed.name);
            assert_eq!(original.visibility, reparsed.visibility);
        }
        Ok(())
    }

    #[test]
    fn configuration_visibility_preserves_both_access_scopes() -> syn::Result<()> {
        for (function, alias, expected) in [
            ("", "pub", "pub"),
            ("pub", "", "pub"),
            ("pub", "pub(crate)", "pub"),
            ("pub(crate)", "pub(in crate::queries)", "pub(crate)"),
            (
                "pub(in crate::queries::nested)",
                "pub(in crate::queries)",
                "pub(in crate::queries)",
            ),
            ("pub(super)", "pub(in super::super)", "pub(in super::super)"),
            ("pub(self)", "", "pub(self)"),
        ] {
            let function = syn::parse_str(function)?;
            let alias = syn::parse_str(alias)?;
            let expected: syn::Visibility = syn::parse_str(expected)?;
            assert_eq!(
                wider_configuration_visibility(&function, &alias)?,
                &expected
            );
        }
        for (function, alias) in [
            ("pub(super)", "pub(in crate::queries)"),
            ("pub(in crate::queries)", "pub(super)"),
            ("pub(in crate::left)", "pub(in crate::right)"),
        ] {
            assert!(
                wider_configuration_visibility(&syn::parse_str(function)?, &syn::parse_str(alias)?)
                    .is_err()
            );
        }
        Ok(())
    }
}
