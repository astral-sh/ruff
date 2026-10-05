use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{Error, Expr, FnArg, ItemFn, Pat, Path, Result, Signature, Token, TraitBound};

mod own_member;
mod relation;
mod shared_semantic_family;

/// Emits the canonical async relation and its direct synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_relation(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match relation::expand(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared async own-member lookup and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_own_member(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared async MRO lookup or finalization and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_mro_member(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_mro(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the shared namespace driver and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_namespace_lookup(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_namespace_lookup(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the shared instance-MRO lookup and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_instance_mro(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_instance_mro(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared async MRO root helper and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_mro_root(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_mro_root(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared async MRO iteration helper and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_mro_iteration(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_mro_iteration(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared async static-MRO construction helper and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_static_mro(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_static_mro(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared async base-MRO start or collection and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_base_mro(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_base_mro(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared async C3 merge and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_c3_merge(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_c3(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared async `ClassType` own-member lookup and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_class_type_own_member(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_class_type(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared async synthesized-member lookup and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_synthesized_member(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_synthesized(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared public-promotion reduction and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_public_promotion(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_promotion(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared protocol-interface construction and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_protocol_interface(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_protocol_interface(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits a declared protocol-relation body and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_protocol_relation(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_protocol_relation(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits a declared constraint type-analysis body and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_constraint_type(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_constraint_type(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits a declared satisfaction body and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_satisfaction(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_satisfaction(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared protocol-object entry and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_protocol_object(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_protocol_object(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the declared protocol member-presence body and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_protocol_members_defined(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_protocol_members_defined(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits the original async mapping function and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_mapping(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match expand(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand(arguments: TokenStream2, original: &TokenStream2) -> Result<TokenStream2> {
    if !arguments.is_empty() {
        return Err(Error::new_spanned(
            arguments,
            "dual_mapping takes no arguments",
        ));
    }

    let mut synchronous: ItemFn = syn::parse2(original.clone())?;
    let Some(name) = synchronous_name(&synchronous.sig.ident) else {
        return Err(Error::new_spanned(
            &synchronous.sig.ident,
            "this function is not in the dual_mapping manifest",
        ));
    };
    if synchronous.sig.asyncness.take().is_none() {
        return Err(Error::new_spanned(
            &synchronous.sig.ident,
            "dual_mapping requires an async function",
        ));
    }

    let has_callback = synchronous.sig.ident == "map_types_with";
    let mut lowerer = Lowerer {
        has_callback,
        scope: Scope::Signature,
        in_matches: false,
        error: None,
    };
    visit_mut::visit_signature_mut(&mut lowerer, &mut synchronous.sig);
    if let Some(error) = lowerer.error.take() {
        return Err(error);
    }
    lower_signature(&mut synchronous.sig, has_callback)?;
    synchronous.sig.ident = name;
    lowerer.scope = Scope::Body;
    lowerer.visit_block_mut(&mut synchronous.block);
    if let Some(error) = lowerer.error {
        return Err(error);
    }

    Ok(quote! { #original #synchronous })
}

fn synchronous_name(name: &Ident) -> Option<Ident> {
    let replacement = if name == "specialization_start_with" {
        "specialization_start_sync"
    } else if name == "mapping_start_with" {
        "mapping_start_sync"
    } else if name == "resume_mapping_with" {
        "resume_mapping_sync"
    } else if name == "complete_mapping_with" {
        "complete_mapping_sync"
    } else if name == "apply_type_mapping_with" {
        "apply_type_mapping_sync"
    } else if name == "apply_specialization_with" {
        "apply_specialization_sync"
    } else if name == "apply_optional_specialization_with" {
        "apply_optional_specialization_sync"
    } else if name == "map_types_with" {
        "map_types_sync"
    } else if name == "promote_impl_with" {
        "promote_impl_sync"
    } else {
        return None;
    };
    Some(Ident::new(replacement, name.span()))
}

fn lower_signature(signature: &mut Signature, has_callback: bool) -> Result<()> {
    let mut lowerer = SignatureLowerer {
        allows_callback: false,
        mapping_bounds: 0,
        callback_bounds: 0,
        error: None,
    };
    lowerer.visit_generics_mut(&mut signature.generics);
    for input in &mut signature.inputs {
        lowerer.allows_callback = has_callback
            && matches!(input, FnArg::Typed(argument) if matches!(argument.pat.as_ref(), Pat::Ident(pattern) if pattern.ident == "map"));
        lowerer.visit_fn_arg_mut(input);
    }
    lowerer.allows_callback = false;
    lowerer.visit_return_type_mut(&mut signature.output);
    if let Some(error) = lowerer.error {
        return Err(error);
    }
    if lowerer.mapping_bounds == 0 {
        return Err(Error::new_spanned(
            &signature.ident,
            "dual_mapping requires a MappingEffects or MappingStartEffects bound",
        ));
    }
    if has_callback && lowerer.callback_bounds != 1 {
        return Err(Error::new_spanned(
            &signature.ident,
            "map_types_with requires a map parameter with an AsyncFnMut bound",
        ));
    }
    Ok(())
}

struct SignatureLowerer {
    allows_callback: bool,
    mapping_bounds: usize,
    callback_bounds: usize,
    error: Option<Error>,
}

impl VisitMut for SignatureLowerer {
    fn visit_trait_bound_mut(&mut self, bound: &mut TraitBound) {
        if bound.path.leading_colon.is_none()
            && bound.path.segments.len() == 1
            && let Some(segment) = bound.path.segments.first_mut()
        {
            if segment.ident == "MappingEffects" || segment.ident == "MappingStartEffects" {
                let arguments = segment.arguments.clone();
                let mut path: Path = if segment.ident == "MappingStartEffects" {
                    syn::parse_quote_spanned! { segment.ident.span() =>
                        crate::types::mapping::effects::SynchronousMappingStartEffects
                    }
                } else {
                    syn::parse_quote_spanned! { segment.ident.span() =>
                        crate::types::mapping::effects::SynchronousMappingEffects
                    }
                };
                if let Some(last) = path.segments.last_mut() {
                    last.arguments = arguments;
                }
                bound.path = path;
                self.mapping_bounds += 1;
            } else if segment.ident == "AsyncFnMut" {
                if self.allows_callback {
                    segment.ident = Ident::new("FnMut", segment.ident.span());
                    self.callback_bounds += 1;
                } else {
                    self.error = Some(Error::new_spanned(
                        &segment.ident,
                        "only the map parameter of map_types_with may use AsyncFnMut",
                    ));
                }
            }
        }
        visit_mut::visit_trait_bound_mut(self, bound);
    }
}

#[derive(Clone, Copy)]
enum Scope {
    Signature,
    Body,
    DeferredBody,
}

struct Lowerer {
    has_callback: bool,
    scope: Scope,
    in_matches: bool,
    error: Option<Error>,
}

impl Lowerer {
    fn reject(&mut self, span: Span, message: &str) {
        if self.error.is_none() {
            self.error = Some(Error::new(span, message));
        }
    }

    fn lower_await(&mut self, awaited: &syn::ExprAwait) -> Result<Expr> {
        if !matches!(self.scope, Scope::Body) || self.in_matches {
            return Err(Error::new_spanned(
                awaited,
                "await is only supported in the mapping body and its declared async callback",
            ));
        }

        let mut lowered = *awaited.base.clone();
        match &mut lowered {
            Expr::MethodCall(call) => {
                let maps_callback = call.method == "map_types_with";
                if let Some(name) = synchronous_name(&call.method) {
                    call.method = name;
                    call.attrs.splice(0..0, awaited.attrs.iter().cloned());
                    if maps_callback && call.args.len() != 3 {
                        return Err(Error::new_spanned(
                            call,
                            "map_types_with requires db, an async closure, and effects",
                        ));
                    }
                    if !call
                        .args
                        .last()
                        .is_some_and(|arg| is_identifier(arg, "effects"))
                    {
                        return Err(Error::new_spanned(
                            call,
                            "declared mapping calls require effects as their final argument",
                        ));
                    }
                    for attribute in &mut call.attrs {
                        self.visit_attribute_mut(attribute);
                    }
                    self.visit_expr_mut(&mut call.receiver);
                    if let Some(arguments) = &mut call.turbofish {
                        self.visit_angle_bracketed_generic_arguments_mut(arguments);
                    }
                    let last = call.args.len() - 1;
                    for (index, argument) in call.args.iter_mut().enumerate() {
                        if index == last {
                            // The exact provider parameter is admitted only in this argument.
                            continue;
                        }
                        if maps_callback && index == 1 {
                            let Expr::Closure(callback) = argument else {
                                return Err(Error::new_spanned(
                                    argument,
                                    "map_types_with requires an async closure as its map argument",
                                ));
                            };
                            if callback.asyncness.take().is_none() {
                                return Err(Error::new_spanned(
                                    callback,
                                    "map_types_with requires an async closure as its map argument",
                                ));
                            }
                            visit_mut::visit_expr_closure_mut(self, callback);
                        } else {
                            self.visit_expr_mut(argument);
                        }
                    }
                } else {
                    if is_identifier(&call.receiver, "effects") && is_fact_method(&call.method) {
                        return Err(Error::new_spanned(
                            call,
                            "mapping fact methods must be called without await",
                        ));
                    }
                    if !((call.method == "checkpoint"
                        || call.method == "map_type"
                        || call.method == "should_bind_self")
                        && is_identifier(&call.receiver, "effects"))
                    {
                        return Err(Error::new_spanned(
                            call,
                            "await requires a declared mapping method or effects.checkpoint/effects.map_type/effects.should_bind_self",
                        ));
                    }
                    call.attrs.splice(0..0, awaited.attrs.iter().cloned());
                    self.visit_provider_arguments(call);
                }
            }
            Expr::Call(call) if self.has_callback && is_identifier(&call.func, "map") => {
                call.attrs.splice(0..0, awaited.attrs.iter().cloned());
                visit_mut::visit_expr_call_mut(self, call);
            }
            _ => {
                return Err(Error::new_spanned(
                    awaited,
                    "await is outside the dual_mapping manifest",
                ));
            }
        }
        Ok(lowered)
    }

    fn visit_provider_arguments(&mut self, call: &mut syn::ExprMethodCall) {
        for attribute in &mut call.attrs {
            self.visit_attribute_mut(attribute);
        }
        if let Some(arguments) = &mut call.turbofish {
            self.visit_angle_bracketed_generic_arguments_mut(arguments);
        }
        for argument in &mut call.args {
            self.visit_expr_mut(argument);
        }
    }
}

impl VisitMut for Lowerer {
    fn visit_pat_ident_mut(&mut self, pattern: &mut syn::PatIdent) {
        if !matches!(self.scope, Scope::Signature)
            && (pattern.ident == "effects"
                || pattern.ident == "r#effects"
                || self.has_callback && (pattern.ident == "map" || pattern.ident == "r#map"))
        {
            self.reject(
                pattern.ident.span(),
                "body bindings cannot shadow effects or the declared map callback",
            );
        }
        visit_mut::visit_pat_ident_mut(self, pattern);
    }

    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Await(awaited) = expression {
            match self.lower_await(awaited) {
                Ok(lowered) => *expression = lowered,
                Err(error) if self.error.is_none() => self.error = Some(error),
                Err(_) => {}
            }
        } else if let Expr::Verbatim(tokens) = expression {
            self.reject(
                tokens.span(),
                "unsupported expression syntax in dual_mapping",
            );
        } else {
            visit_mut::visit_expr_mut(self, expression);
        }
    }

    fn visit_expr_method_call_mut(&mut self, call: &mut syn::ExprMethodCall) {
        if is_identifier(&call.receiver, "effects") {
            if call.method == "legacy" && !matches!(self.scope, Scope::Signature) {
                self.visit_provider_arguments(call);
            } else if is_fact_method(&call.method) {
                if matches!(self.scope, Scope::Body) {
                    self.visit_provider_arguments(call);
                } else {
                    self.reject(call.span(), "fact calls are only supported in the mapping body and its declared async callback");
                }
            } else {
                self.reject(
                    call.span(),
                    "unawaited calls on effects require a declared mapping fact method or legacy",
                );
            }
            return;
        }
        if synchronous_name(&call.method).is_some() {
            self.reject(
                call.span(),
                "declared mapping methods must be awaited directly",
            );
            return;
        }
        visit_mut::visit_expr_method_call_mut(self, call);
    }

    fn visit_expr_path_mut(&mut self, path: &mut syn::ExprPath) {
        if path.qself.is_none()
            && (path.path.is_ident("effects") || path.path.is_ident("r#effects"))
        {
            self.reject(
                path.span(),
                "effects may only be used in a declared provider or mapping call",
            );
        }
        visit_mut::visit_expr_path_mut(self, path);
    }

    fn visit_expr_async_mut(&mut self, expression: &mut syn::ExprAsync) {
        self.reject(
            expression.span(),
            "async blocks are not supported by dual_mapping",
        );
    }

    fn visit_expr_closure_mut(&mut self, closure: &mut syn::ExprClosure) {
        if closure.asyncness.is_some() {
            self.reject(
                closure.span(),
                "async closures are only supported as the map argument of awaited map_types_with",
            );
            return;
        }
        let scope = self.scope;
        if matches!(self.scope, Scope::Body) {
            self.scope = Scope::DeferredBody;
        }
        visit_mut::visit_expr_closure_mut(self, closure);
        self.scope = scope;
    }

    fn visit_expr_const_mut(&mut self, expression: &mut syn::ExprConst) {
        let scope = self.scope;
        self.scope = Scope::DeferredBody;
        visit_mut::visit_expr_const_mut(self, expression);
        self.scope = scope;
    }

    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        let scope = self.scope;
        self.scope = Scope::DeferredBody;
        visit_mut::visit_type_mut(self, ty);
        self.scope = scope;
    }

    fn visit_generic_argument_mut(&mut self, argument: &mut syn::GenericArgument) {
        let scope = self.scope;
        self.scope = Scope::DeferredBody;
        visit_mut::visit_generic_argument_mut(self, argument);
        self.scope = scope;
    }

    fn visit_signature_mut(&mut self, signature: &mut Signature) {
        if signature.asyncness.is_some() {
            self.reject(
                signature.span(),
                "nested async items are not supported by dual_mapping",
            );
        }
        visit_mut::visit_signature_mut(self, signature);
    }

    fn visit_item_mut(&mut self, item: &mut syn::Item) {
        let scope = self.scope;
        if matches!(self.scope, Scope::Body) {
            self.scope = Scope::DeferredBody;
        }
        visit_mut::visit_item_mut(self, item);
        self.scope = scope;
    }

    fn visit_macro_mut(&mut self, invocation: &mut syn::Macro) {
        if self.in_matches {
            self.reject(invocation.span(), "matches! cannot contain nested macros");
        } else if !invocation.path.is_ident("matches") {
            self.reject(
                invocation.span(),
                "only checked matches! expressions are supported by dual_mapping",
            );
        } else {
            match syn::parse2::<MatchesInput>(invocation.tokens.clone()) {
                Ok(mut input) => {
                    self.in_matches = true;
                    self.visit_expr_mut(&mut input.expression);
                    self.visit_pat_mut(&mut input.pattern);
                    if let Some(guard) = &mut input.guard {
                        self.visit_expr_mut(guard);
                    }
                    self.in_matches = false;
                }
                Err(error) if self.error.is_none() => self.error = Some(error),
                Err(_) => {}
            }
        }
    }
}

fn is_identifier(expression: &Expr, name: &str) -> bool {
    matches!(expression, Expr::Path(path) if path.qself.is_none() && path.path.is_ident(name))
}

fn is_fact_method(name: &Ident) -> bool {
    name == "variance"
        || name == "scalar_fallback"
        || name == "begin_transformation"
        || name == "finish_transformation"
}

struct MatchesInput {
    expression: Expr,
    pattern: Pat,
    guard: Option<Expr>,
}

impl Parse for MatchesInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let expression = input.parse()?;
        input.parse::<Token![,]>()?;
        let pattern = Pat::parse_multi_with_leading_vert(input)?;
        let guard = if input.parse::<Option<Token![if]>>()?.is_some() {
            Some(input.parse()?)
        } else {
            None
        };
        input.parse::<Option<Token![,]>>()?;
        Ok(Self {
            expression,
            pattern,
            guard,
        })
    }
}

#[cfg(test)]
mod tests;

/// Emits a declared sequent rule body and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_sequent(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_sequent(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits a shared member source selector or reduction and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_member_source(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_member_source(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits a declared class storage body and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_instance_storage(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_instance_storage(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Emits a declared slot selector or finite scan and its synchronous counterpart.
#[proc_macro_attribute]
pub fn dual_slot_selector(arguments: TokenStream, input: TokenStream) -> TokenStream {
    match own_member::expand_slot_selector(arguments.into(), &input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Generates paired effect interfaces and a synchronous copy of a shared capability body.
///
/// Finite capability implementations are reviewed code. This macro checks their use in the
/// shared body; it does not prove that their implementations are finite or charge semantic work.
/// The operation boundary describes the provider contract; it does not generate a scheduler.
///
/// ```rust
/// use ty_mapping_probe_macros::shared_semantic_family;
/// struct Facts;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)]
///         async fn value(&self) -> Result<u8, Self::Error>;
///     }
///     #[finite_capability]
///     impl Facts { fn identity(&self, value: u8) -> u8 { value } }
///     #[synchronous(read_sync)]
///     #[capabilities(effects = Effects, facts = Facts)]
///     #[passive_values()]
///     async fn read<E: Effects>(facts: Facts, effects: &E) -> Result<u8, E::Error> {
///         let value = effects.value().await?; Ok(facts.identity(value))
///     }
/// }
/// struct Provider;
/// impl SyncEffects for Provider {
///     type Error = ();
///     fn value(&self) -> Result<u8, Self::Error> { Ok(7) }
/// }
/// fn main() { assert_eq!(read_sync(Facts, &Provider), Ok(7)); }
/// ```
///
/// Concrete finite arguments are checked by Rust; a string cannot replace the byte above.
///
/// ```compile_fail
/// use ty_mapping_probe_macros::shared_semantic_family;
/// struct Facts;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)]
///         async fn value(&self) -> Result<u8, Self::Error>;
///     }
///     #[finite_capability]
///     impl Facts { fn identity(&self, value: u8) -> u8 { value } }
///     #[synchronous(read_sync)]
///     #[capabilities(effects = Effects, facts = Facts)]
///     #[passive_values()]
///     async fn read<E: Effects>(facts: Facts, effects: &E) -> Result<u8, E::Error> {
///         let value = effects.value().await?; Ok(facts.identity("wrong type"))
///     }
/// }
/// struct Provider;
/// impl SyncEffects for Provider {
///     type Error = ();
///     fn value(&self) -> Result<u8, Self::Error> { Ok(7) }
/// }
/// fn main() { assert_eq!(read_sync(Facts, &Provider), Ok(7)); }
/// ```
///
/// The declared result must also match the shared body's result.
///
/// ```compile_fail
/// use ty_mapping_probe_macros::shared_semantic_family;
/// struct Facts;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)]
///         async fn value(&self) -> Result<u8, Self::Error>;
///     }
///     #[finite_capability]
///     impl Facts { fn identity(&self, value: u8) -> u8 { value } }
///     #[synchronous(read_sync)]
///     #[capabilities(effects = Effects, facts = Facts)]
///     #[passive_values()]
///     async fn read<E: Effects>(facts: Facts, effects: &E) -> Result<bool, E::Error> {
///         let value = effects.value().await?; Ok(facts.identity(value))
///     }
/// }
/// struct Provider;
/// impl SyncEffects for Provider {
///     type Error = ();
///     fn value(&self) -> Result<u8, Self::Error> { Ok(7) }
/// }
/// fn main() { let _ = read_sync(Facts, &Provider); }
/// ```
///
/// Providers must implement every generated method.
///
/// ```compile_fail
/// use ty_mapping_probe_macros::shared_semantic_family;
/// struct Facts;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)]
///         async fn value(&self) -> Result<u8, Self::Error>;
///     }
///     #[finite_capability]
///     impl Facts { fn identity(&self, value: u8) -> u8 { value } }
///     #[synchronous(read_sync)]
///     #[capabilities(effects = Effects, facts = Facts)]
///     #[passive_values()]
///     async fn read<E: Effects>(facts: Facts, effects: &E) -> Result<u8, E::Error> {
///         let value = effects.value().await?; Ok(facts.identity(value))
///     }
/// }
/// struct Provider;
/// impl SyncEffects for Provider {
///     type Error = ();
/// }
/// fn main() { assert_eq!(read_sync(Facts, &Provider), Ok(7)); }
/// ```
///
/// Borrowed effect results retain their declared lifetime in both interfaces.
///
/// ```rust
/// use ty_mapping_probe_macros::shared_semantic_family;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(child)]
///         async fn borrow<'value>(&self, value: &'value u8) -> Result<&'value u8, Self::Error>;
///     }
///     #[synchronous(borrow_sync)]
///     #[capabilities(effects = Effects)]
///     #[passive_values()]
///     async fn borrow<'value, E: Effects>(value: &'value u8, effects: &E)
///         -> Result<&'value u8, E::Error>
///     {
///         effects.borrow(value).await
///     }
/// }
/// fn main() {}
/// ```
///
/// Lowering cannot extend the lifetime of a borrowed result.
///
/// ```compile_fail
/// use ty_mapping_probe_macros::shared_semantic_family;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(child)]
///         async fn borrow<'value>(&self, value: &'value u8) -> Result<&'value u8, Self::Error>;
///     }
///     #[synchronous(borrow_sync)]
///     #[capabilities(effects = Effects)]
///     #[passive_values()]
///     async fn borrow<'value, E: Effects>(value: &'value u8, effects: &E)
///         -> Result<&'static u8, E::Error>
///     {
///         effects.borrow(value).await
///     }
/// }
/// fn main() {}
/// ```
///
/// Structured iteration uses `#[cursor_loop] while let Some(item) = effects.advance(...).await?`.
/// The method must declare `#[progress]` and a `local` or `child` operation boundary. Its provider
/// must admit positive work before returning each `Some`, including cached items; exhaustion may
/// return `None` without a charge. The macro checks that every back edge calls this boundary. It
/// cannot establish the provider's charging behavior. Header arguments cannot contain loop exits.
/// Bare `break` and `continue` are allowed in the body; unrestricted loops remain unsupported.
///
/// A `#[passive_state] let mut value = ...` binding permits standalone assignments to that local.
/// Its inferred type must be `Copy`, so replacing it cannot destroy an owned resource. Cursor and
/// collection mutations instead pass through declared effects. Shadowing a state binding does
/// not grant the new binding assignment permission. All three markers are removed from both
/// generated bodies. The loop pattern is qualified to the standard `Option::Some` constructor.
///
/// This generic provider exercises both forms with identical item and result traces. Admission
/// precedes cursor mutation; asynchronous operations suspend before doing their work. A pending
/// child keeps the caller's cursor alive until the future completes or is dropped. A scheduler's
/// queued-child drain and protected-error behavior additionally require tests in its own runtime.
///
/// ```rust
/// use std::cell::{Cell, RefCell};
/// use std::future::{Future, poll_fn};
/// use std::pin::pin;
/// use std::rc::Rc;
/// use std::task::{Context, Poll, Waker};
/// use ty_mapping_probe_macros::shared_semantic_family;
///
/// #[derive(Clone, Debug, PartialEq)]
/// enum Event { Item(u8), Used(u8), Refused(usize), Exhausted }
/// #[derive(Clone, Default)]
/// struct Audit {
///     events: Rc<RefCell<Vec<Event>>>,
///     pending: Rc<Cell<usize>>,
///     cache_hits: Rc<Cell<usize>>,
///     dropped: Rc<Cell<usize>>,
/// }
/// struct Cursor { position: usize, audit: Audit }
/// impl Drop for Cursor {
///     fn drop(&mut self) { self.audit.dropped.set(self.audit.dropped.get() + 1); }
/// }
/// struct Facts;
/// #[derive(Debug, PartialEq)]
/// struct Refused;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)]
///         #[progress]
///         async fn advance(&mut self, cursor: &mut Cursor) -> Result<Option<u8>, Self::Error>;
///         #[operation(child)]
///         async fn use_item(&mut self, item: u8) -> Result<u8, Self::Error>;
///     }
///     #[finite_capability]
///     impl Facts { fn add(&self, total: u16, item: u8) -> u16 { total + u16::from(item) } }
///     #[synchronous(sum_sync)]
///     #[capabilities(effects = Effects, facts = Facts)]
///     #[passive_values()]
///     async fn sum<E: Effects>(mut cursor: Cursor, facts: Facts, effects: &mut E)
///         -> Result<u16, E::Error>
///     {
///         #[passive_state]
///         let mut total = 0;
///         #[cursor_loop]
///         while let Some(item) = effects.advance(&mut cursor).await? {
///             let used = effects.use_item(item).await?;
///             total = facts.add(total, used);
///         }
///         Ok(total)
///     }
/// }
/// struct Provider { allowance: usize, cached: Option<u8>, audit: Audit }
/// impl Provider {
///     fn admitted(&mut self, cursor: &mut Cursor) -> Result<Option<u8>, Refused> {
///         let items = [2, 2, 5];
///         if cursor.position == items.len() {
///             self.audit.events.borrow_mut().push(Event::Exhausted);
///             return Ok(None);
///         }
///         if self.allowance == 0 {
///             self.audit.events.borrow_mut().push(Event::Refused(cursor.position));
///             return Err(Refused);
///         }
///         self.allowance -= 1;
///         let candidate = items[cursor.position];
///         let item = if let Some(cached) = self.cached && cached == candidate {
///             self.audit.cache_hits.set(self.audit.cache_hits.get() + 1);
///             cached
///         } else {
///             self.cached = Some(candidate);
///             candidate
///         };
///         cursor.position += 1;
///         self.audit.events.borrow_mut().push(Event::Item(item));
///         Ok(Some(item))
///     }
///     fn used(&mut self, item: u8) -> Result<u8, Refused> {
///         self.audit.events.borrow_mut().push(Event::Used(item));
///         Ok(item)
///     }
/// }
/// async fn suspend_once(audit: &Audit) {
///     let mut pending = true;
///     poll_fn(|context| {
///         if pending {
///             pending = false;
///             audit.pending.set(audit.pending.get() + 1);
///             context.waker().wake_by_ref();
///             Poll::Pending
///         } else { Poll::Ready(()) }
///     }).await;
/// }
/// impl Effects for Provider {
///     type Error = Refused;
///     async fn advance(&mut self, cursor: &mut Cursor) -> Result<Option<u8>, Refused> {
///         suspend_once(&self.audit).await;
///         self.admitted(cursor)
///     }
///     async fn use_item(&mut self, item: u8) -> Result<u8, Refused> {
///         suspend_once(&self.audit).await;
///         self.used(item)
///     }
/// }
/// impl SyncEffects for Provider {
///     type Error = Refused;
///     fn advance(&mut self, cursor: &mut Cursor) -> Result<Option<u8>, Refused> {
///         self.admitted(cursor)
///     }
///     fn use_item(&mut self, item: u8) -> Result<u8, Refused> { self.used(item) }
/// }
/// fn drive<F: Future>(future: F) -> F::Output {
///     let mut future = pin!(future);
///     let mut context = Context::from_waker(Waker::noop());
///     loop {
///         if let Poll::Ready(output) = future.as_mut().poll(&mut context) { return output; }
///     }
/// }
/// fn run(allowance: usize, asynchronous: bool) -> (Result<u16, Refused>, Audit) {
///     let audit = Audit::default();
///     let cursor = Cursor { position: 0, audit: audit.clone() };
///     let mut provider = Provider { allowance, cached: None, audit: audit.clone() };
///     let output = if asynchronous { drive(sum(cursor, Facts, &mut provider)) }
///         else { sum_sync(cursor, Facts, &mut provider) };
///     assert_eq!(audit.dropped.get(), 1);
///     (output, audit)
/// }
/// for (allowance, expected) in [(3, Ok(9)), (2, Err(Refused)), (0, Err(Refused))] {
///     let (synchronous, sync_audit) = run(allowance, false);
///     let (asynchronous, async_audit) = run(allowance, true);
///     assert_eq!(synchronous, expected);
///     assert_eq!(asynchronous, expected);
///     assert_eq!(*sync_audit.events.borrow(), *async_audit.events.borrow());
///     assert!(async_audit.pending.get() > 0);
///     assert_eq!(async_audit.cache_hits.get(), if allowance >= 2 { 1 } else { 0 });
///     if allowance < 3 {
///         assert_eq!(async_audit.events.borrow().last(), Some(&Event::Refused(allowance)));
///     }
/// }
/// let audit = Audit::default();
/// let mut cursor = Cursor { position: 0, audit: audit.clone() };
/// let mut provider = Provider { allowance: 0, cached: None, audit: audit.clone() };
/// assert_eq!(drive(Effects::advance(&mut provider, &mut cursor)), Err(Refused));
/// assert_eq!(cursor.position, 0);
/// drop(cursor);
///
/// let audit = Audit::default();
/// let cursor = Cursor { position: 0, audit: audit.clone() };
/// let mut provider = Provider { allowance: 3, cached: None, audit: audit.clone() };
/// let mut pending = Box::pin(sum(cursor, Facts, &mut provider));
/// let mut context = Context::from_waker(Waker::noop());
/// assert!(pending.as_mut().poll(&mut context).is_pending()); // Before advance.
/// assert_eq!(audit.dropped.get(), 0);
/// assert!(audit.events.borrow().is_empty());
/// assert!(pending.as_mut().poll(&mut context).is_pending()); // Before the child completes.
/// assert_eq!(*audit.events.borrow(), vec![Event::Item(2)]);
/// assert_eq!(audit.dropped.get(), 0);
/// assert!(pending.as_mut().poll(&mut context).is_pending()); // Child drained; next advance pending.
/// assert_eq!(*audit.events.borrow(), vec![Event::Item(2), Event::Used(2)]);
/// assert_eq!(audit.dropped.get(), 0);
/// drop(pending);
/// assert_eq!(audit.dropped.get(), 1);
/// for polls in [0, 1, 2] {
///     let audit = Audit::default();
///     let cursor = Cursor { position: 0, audit: audit.clone() };
///     let mut provider = Provider { allowance: 3, cached: None, audit: audit.clone() };
///     let mut pending = Box::pin(sum(cursor, Facts, &mut provider));
///     for _ in 0..polls { assert!(pending.as_mut().poll(&mut context).is_pending()); }
///     assert_eq!(audit.dropped.get(), 0);
///     drop(pending);
///     assert_eq!(audit.dropped.get(), 1);
/// }
/// ```
///
/// Passive state supports a generic `Copy` value, even when its name resembles generated code.
///
/// ```rust
/// use ty_mapping_probe_macros::shared_semantic_family;
/// shared_semantic_family! {
///     #[synchronous(identity_sync)]
///     #[capabilities()]
///     #[passive_values()]
///     async fn identity<T: Copy>(seed: T) -> T {
///         #[passive_state]
///         let mut __shared_semantic_require_copy = seed;
///         __shared_semantic_require_copy = seed;
///         __shared_semantic_require_copy
///     }
/// }
/// assert_eq!(identity_sync(7), 7);
/// ```
///
/// A resource owner cannot become passive state. The generated check rejects `String: Copy`.
///
/// ```compile_fail,E0277
/// use ty_mapping_probe_macros::shared_semantic_family;
/// shared_semantic_family! {
///     #[synchronous(identity_sync)]
///     #[capabilities()]
///     #[passive_values()]
///     async fn identity(seed: String) -> String {
///         #[passive_state]
///         let mut __shared_semantic_require_copy = seed;
///         __shared_semantic_require_copy
///     }
/// }
/// fn main() {}
/// ```
///
/// Rust checks the standard Option result, cursor and item types, and mutable effect reference.
/// This control has a byte cursor and item, with mutable access to the provider.
///
/// ```rust
/// use ty_mapping_probe_macros::shared_semantic_family;
/// struct Facts;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)] #[progress]
///         async fn advance(&mut self, cursor: &mut u8) -> Result<Option<u8>, Self::Error>;
///     }
///     #[finite_capability]
///     impl Facts { fn accept(&self, item: u8) -> u8 { item } }
///     #[synchronous(scan_sync)]
///     #[capabilities(effects = Effects, facts = Facts)]
///     #[passive_values()]
///     async fn scan<E: Effects>(mut cursor: u8, facts: Facts, effects: &mut E)
///         -> Result<(), E::Error>
///     {
///         #[cursor_loop]
///         while let Some(item) = effects.advance(&mut cursor).await? { facts.accept(item); }
///         Ok(())
///     }
/// }
/// fn main() {}
/// ```
///
/// An unrelated enum named `Option` does not replace the standard loop pattern.
///
/// ```compile_fail,E0308
/// use ty_mapping_probe_macros::shared_semantic_family;
/// enum Option<T> { Some(T), None }
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)] #[progress]
///         async fn advance(&mut self, cursor: &mut u8) -> Result<Option<u8>, Self::Error>;
///     }
///     #[synchronous(scan_sync)]
///     #[capabilities(effects = Effects)]
///     #[passive_values()]
///     async fn scan<E: Effects>(mut cursor: u8, effects: &mut E) -> Result<(), E::Error> {
///         #[cursor_loop]
///         while let Some(item) = effects.advance(&mut cursor).await? {}
///         Ok(())
///     }
/// }
/// fn main() {}
/// ```
///
/// The cursor argument must match the declared progress operation.
///
/// ```compile_fail,E0308
/// use ty_mapping_probe_macros::shared_semantic_family;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)] #[progress]
///         async fn advance(&mut self, cursor: &mut u8) -> Result<Option<u8>, Self::Error>;
///     }
///     #[synchronous(scan_sync)]
///     #[capabilities(effects = Effects)]
///     #[passive_values()]
///     async fn scan<E: Effects>(mut cursor: u16, effects: &mut E) -> Result<(), E::Error> {
///         #[cursor_loop]
///         while let Some(item) = effects.advance(&mut cursor).await? {}
///         Ok(())
///     }
/// }
/// fn main() {}
/// ```
///
/// The yielded byte cannot be passed to a finite operation expecting a boolean.
///
/// ```compile_fail,E0308
/// use ty_mapping_probe_macros::shared_semantic_family;
/// struct Facts;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)] #[progress]
///         async fn advance(&mut self, cursor: &mut u8) -> Result<Option<u8>, Self::Error>;
///     }
///     #[finite_capability]
///     impl Facts { fn accept(&self, item: bool) -> bool { item } }
///     #[synchronous(scan_sync)]
///     #[capabilities(effects = Effects, facts = Facts)]
///     #[passive_values()]
///     async fn scan<E: Effects>(mut cursor: u8, facts: Facts, effects: &mut E)
///         -> Result<(), E::Error>
///     {
///         #[cursor_loop]
///         while let Some(item) = effects.advance(&mut cursor).await? { facts.accept(item); }
///         Ok(())
///     }
/// }
/// fn main() {}
/// ```
///
/// Lowering preserves the effect reference's mutability; `&E` cannot call an `&mut self` method.
///
/// ```compile_fail,E0596
/// use ty_mapping_probe_macros::shared_semantic_family;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)] #[progress]
///         async fn advance(&mut self, cursor: &mut u8) -> Result<Option<u8>, Self::Error>;
///     }
///     #[synchronous(scan_sync)]
///     #[capabilities(effects = Effects)]
///     #[passive_values()]
///     async fn scan<E: Effects>(mut cursor: u8, effects: &E) -> Result<(), E::Error> {
///         #[cursor_loop]
///         while let Some(item) = effects.advance(&mut cursor).await? {}
///         Ok(())
///     }
/// }
/// fn main() {}
/// ```
///
/// An item may borrow the cursor. Its last use must precede another mutable advancement.
///
/// ```rust
/// use ty_mapping_probe_macros::shared_semantic_family;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)] #[progress]
///         async fn advance<'a>(&mut self, cursor: &'a mut u8) -> Result<Option<&'a u8>, Self::Error>;
///         #[operation(child)]
///         async fn use_item(&mut self, item: &u8) -> Result<(), Self::Error>;
///     }
///     #[synchronous(scan_sync)]
///     #[capabilities(effects = Effects)]
///     #[passive_values()]
///     async fn scan<E: Effects>(mut cursor: u8, effects: &mut E) -> Result<(), E::Error> {
///         #[cursor_loop]
///         while let Some(item) = effects.advance(&mut cursor).await? {
///             effects.use_item(item).await?;
///             let next = effects.advance(&mut cursor).await?;
///         }
///         Ok(())
///     }
/// }
/// fn main() {}
/// ```
///
/// Keeping the first borrowed item live across a second advancement is a conflicting borrow.
///
/// ```compile_fail,E0499
/// use ty_mapping_probe_macros::shared_semantic_family;
/// shared_semantic_family! {
///     #[synchronous(SyncEffects)]
///     trait Effects {
///         type Error;
///         #[operation(local)] #[progress]
///         async fn advance<'a>(&mut self, cursor: &'a mut u8) -> Result<Option<&'a u8>, Self::Error>;
///         #[operation(child)]
///         async fn use_item(&mut self, item: &u8) -> Result<(), Self::Error>;
///     }
///     #[synchronous(scan_sync)]
///     #[capabilities(effects = Effects)]
///     #[passive_values()]
///     async fn scan<E: Effects>(mut cursor: u8, effects: &mut E) -> Result<(), E::Error> {
///         #[cursor_loop]
///         while let Some(item) = effects.advance(&mut cursor).await? {
///             let next = effects.advance(&mut cursor).await?;
///             effects.use_item(item).await?;
///         }
///         Ok(())
///     }
/// }
/// fn main() {}
/// ```
#[proc_macro]
pub fn shared_semantic_family(input: TokenStream) -> TokenStream {
    match shared_semantic_family::expand(input.into()) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
