//! Paired interfaces and shared bodies whose operations are declared in the same invocation.
//!
//! The body grammar checks how capabilities are used. Finite implementations and runtime
//! providers remain reviewed boundaries: their Rust types do not prove admission or termination.

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Attribute, Error, Expr, FnArg, Item, ItemTrait, Pat, Result, Signature, Token, TraitItem,
};

use crate::MatchesInput;

#[cfg(test)]
mod tests;

struct Binding {
    receiver: Ident,
    declaration: Ident,
}

impl Parse for Binding {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let receiver = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(Self {
            receiver,
            declaration: input.parse()?,
        })
    }
}

fn take_attribute(attributes: &mut Vec<Attribute>, name: &str) -> Result<Option<Attribute>> {
    let mut found = None;
    let mut retained = Vec::new();
    for attribute in std::mem::take(attributes) {
        if attribute.path().is_ident(name) {
            if found.is_some() {
                return Err(Error::new_spanned(attribute, "duplicate family attribute"));
            }
            found = Some(attribute);
        } else {
            retained.push(attribute);
        }
    }
    *attributes = retained;
    Ok(found)
}

fn required_attribute(
    attributes: &mut Vec<Attribute>,
    name: &str,
    span: Span,
) -> Result<Attribute> {
    take_attribute(attributes, name)?
        .ok_or_else(|| Error::new(span, format!("missing #[{name}(…)] declaration")))
}

fn inherited_methods(
    name: &str,
    traits: &BTreeMap<String, ItemTrait>,
    operations: &BTreeMap<String, BTreeMap<String, Operation>>,
    active: &mut BTreeSet<String>,
) -> Result<BTreeMap<String, Operation>> {
    let declaration = &traits[name];
    if !active.insert(name.to_owned()) {
        return Err(Error::new_spanned(declaration, "cyclic effect supertraits"));
    }
    let mut methods = operations[name].clone();
    for bound in &declaration.supertraits {
        if let syn::TypeParamBound::Trait(bound) = bound
            && bound.path.segments.len() == 1
            && let Some(parent) = bound.path.segments.first()
            && traits.contains_key(&parent.ident.to_string())
        {
            for (name, operation) in
                inherited_methods(&parent.ident.to_string(), traits, operations, active)?
            {
                if let Some(previous) = methods.get(&name)
                    && previous != &operation
                {
                    return Err(Error::new_spanned(
                        declaration,
                        "conflicting inherited effect operation",
                    ));
                }
                methods.insert(name, operation);
            }
        }
    }
    active.remove(name);
    Ok(methods)
}

#[derive(Clone, PartialEq, Eq)]
struct Operation {
    boundary: String,
    progress: bool,
    signature: Signature,
}

fn conditional_attribute(attributes: &[Attribute]) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr"))
}

fn progress_result(output: &syn::ReturnType) -> bool {
    let syn::ReturnType::Type(_, ty) = output else {
        return false;
    };
    let syn::Type::Path(result) = &**ty else {
        return false;
    };
    let Some(segment) = result.path.segments.last() else {
        return false;
    };
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return false;
    };
    if result.qself.is_some() || segment.ident != "Result" || arguments.args.len() != 2 {
        return false;
    }
    let mut arguments = arguments.args.iter();
    let Some(syn::GenericArgument::Type(syn::Type::Path(option))) = arguments.next() else {
        return false;
    };
    let Some(segment) = option.path.segments.last() else {
        return false;
    };
    let syn::PathArguments::AngleBracketed(values) = &segment.arguments else {
        return false;
    };
    if option.qself.is_some()
        || segment.ident != "Option"
        || values.args.len() != 1
        || !matches!(values.args.first(), Some(syn::GenericArgument::Type(_)))
    {
        return false;
    }
    matches!(arguments.next(), Some(syn::GenericArgument::Type(syn::Type::Path(error)))
        if error.qself.is_none()
            && error.path.leading_colon.is_none()
            && error.path.segments.len() == 2
            && error.path.segments.first().is_some_and(|segment| segment.ident == "Self")
            && error.path.segments.iter().all(|segment| matches!(segment.arguments, syn::PathArguments::None)))
}

struct SynchronousBounds<'a>(&'a BTreeMap<String, Ident>);

impl VisitMut for SynchronousBounds<'_> {
    fn visit_trait_bound_mut(&mut self, bound: &mut syn::TraitBound) {
        if bound.path.leading_colon.is_none()
            && bound.path.segments.len() == 1
            && let Some(segment) = bound.path.segments.first_mut()
            && let Some(name) = self.0.get(&segment.ident.to_string())
        {
            segment.ident = name.clone();
        }
        visit_mut::visit_trait_bound_mut(self, bound);
    }
}

fn has_effect_bound(function: &syn::ItemFn, parameter: &Ident, declaration: &str) -> bool {
    let matches = |bound: &syn::TypeParamBound| {
        matches!(bound, syn::TypeParamBound::Trait(bound)
            if bound.path.leading_colon.is_none()
                && bound.path.segments.len() == 1
                && bound.path.segments.first().is_some_and(|segment| segment.ident == declaration))
    };
    function.sig.generics.type_params().any(|ty| ty.ident == *parameter && ty.bounds.iter().any(&matches))
        || function.sig.generics.where_clause.as_ref().is_some_and(|clause| {
            clause.predicates.iter().any(|predicate| {
                matches!(predicate, syn::WherePredicate::Type(predicate)
                    if matches!(&predicate.bounded_ty, syn::Type::Path(ty) if ty.qself.is_none() && ty.path.is_ident(parameter))
                        && predicate.bounds.iter().any(&matches))
            })
        })
}

pub(super) fn expand(input: TokenStream) -> Result<TokenStream> {
    let parsed = syn::parse2::<syn::File>(input)?;
    let mut traits = BTreeMap::new();
    let mut operations = BTreeMap::new();
    let mut synchronous_names = BTreeMap::new();
    let mut finite = BTreeMap::new();
    let mut items = Vec::new();
    let mut functions = Vec::new();
    for item in parsed.items {
        match item {
            Item::Trait(mut declaration) => {
                let span = declaration.span();
                let name: Ident = required_attribute(&mut declaration.attrs, "synchronous", span)?
                    .parse_args()?;
                let mut methods = BTreeMap::new();
                for item in &mut declaration.items {
                    match item {
                        TraitItem::Fn(method) => {
                            let span = method.span();
                            let operation: Ident =
                                required_attribute(&mut method.attrs, "operation", span)?
                                    .parse_args()?;
                            if !matches!(
                                operation.to_string().as_str(),
                                "checkpoint" | "local" | "source" | "child"
                            ) {
                                return Err(Error::new_spanned(
                                    operation,
                                    "unknown operation boundary",
                                ));
                            }
                            if method.sig.asyncness.is_none() || method.default.is_some() {
                                return Err(Error::new_spanned(
                                    method,
                                    "effects require async method declarations",
                                ));
                            }
                            let progress = take_attribute(&mut method.attrs, "progress")?;
                            if let Some(progress) = &progress {
                                if !matches!(progress.meta, syn::Meta::Path(_)) {
                                    return Err(Error::new_spanned(
                                        progress,
                                        "expected #[progress]",
                                    ));
                                }
                                if !matches!(operation.to_string().as_str(), "local" | "child") {
                                    return Err(Error::new_spanned(
                                        operation,
                                        "progress requires a local or child operation",
                                    ));
                                }
                                if conditional_attribute(&method.attrs) {
                                    return Err(Error::new_spanned(
                                        method,
                                        "progress declarations cannot be conditional",
                                    ));
                                }
                                if !progress_result(&method.sig.output) {
                                    return Err(Error::new_spanned(
                                        &method.sig.output,
                                        "progress requires Result<Option<T>, Self::Error>",
                                    ));
                                }
                            }
                            methods.insert(
                                binding_name(&method.sig.ident),
                                Operation {
                                    boundary: operation.to_string(),
                                    progress: progress.is_some(),
                                    signature: method.sig.clone(),
                                },
                            );
                        }
                        TraitItem::Type(_) => {}
                        _ => {
                            return Err(Error::new_spanned(
                                item,
                                "effects support associated types and methods",
                            ));
                        }
                    }
                }
                if methods.values().any(|operation| operation.progress)
                    && conditional_attribute(&declaration.attrs)
                {
                    return Err(Error::new_spanned(
                        &declaration,
                        "progress declarations cannot be conditional",
                    ));
                }
                let mut markers = RejectMarkers::default();
                markers.visit_item_trait_mut(&mut declaration);
                if let Some(error) = markers.error {
                    return Err(error);
                }
                let key = declaration.ident.to_string();
                operations.insert(key.clone(), methods);
                if traits.insert(key.clone(), declaration.clone()).is_some() {
                    return Err(Error::new_spanned(
                        declaration,
                        "duplicate effect declaration",
                    ));
                }
                synchronous_names.insert(key, name);
                items.push(Item::Trait(declaration));
            }
            Item::Impl(mut implementation) => {
                let span = implementation.span();
                let attribute =
                    required_attribute(&mut implementation.attrs, "finite_capability", span)?;
                if !matches!(attribute.meta, syn::Meta::Path(_))
                    || implementation.trait_.is_some()
                    || !implementation.generics.params.is_empty()
                {
                    return Err(Error::new_spanned(
                        implementation,
                        "finite capabilities require a concrete inherent implementation",
                    ));
                }
                let syn::Type::Path(ty) = &*implementation.self_ty else {
                    return Err(Error::new_spanned(
                        &implementation.self_ty,
                        "expected a concrete capability type",
                    ));
                };
                let Some(name) = ty.path.get_ident().filter(|_| ty.qself.is_none()) else {
                    return Err(Error::new_spanned(
                        ty,
                        "expected an unqualified capability type",
                    ));
                };
                let mut methods = BTreeSet::new();
                for item in &implementation.items {
                    let syn::ImplItem::Fn(method) = item else {
                        return Err(Error::new_spanned(
                            item,
                            "finite capabilities contain methods only",
                        ));
                    };
                    if method.sig.asyncness.is_some() || method.sig.receiver().is_none() {
                        return Err(Error::new_spanned(
                            method,
                            "finite operations require synchronous methods",
                        ));
                    }
                    methods.insert(binding_name(&method.sig.ident));
                }
                if finite.insert(name.to_string(), methods).is_some() {
                    return Err(Error::new_spanned(
                        implementation,
                        "duplicate finite capability",
                    ));
                }
                let mut markers = RejectMarkers::default();
                markers.visit_item_impl_mut(&mut implementation);
                if let Some(error) = markers.error {
                    return Err(error);
                }
                items.push(Item::Impl(implementation));
            }
            Item::Fn(function) => functions.push(function),
            _ => {
                return Err(Error::new_spanned(
                    item,
                    "a family contains effects, finite implementations and shared functions",
                ));
            }
        }
    }
    let inherited: BTreeMap<_, _> = traits
        .keys()
        .map(|name| {
            inherited_methods(name, &traits, &operations, &mut BTreeSet::new())
                .map(|methods| (name.clone(), methods))
        })
        .collect::<Result<_>>()?;
    let mut output = TokenStream::new();
    for item in items {
        if let Item::Trait(declaration) = &item {
            let mut synchronous = declaration.clone();
            synchronous.ident = synchronous_names[&declaration.ident.to_string()].clone();
            SynchronousBounds(&synchronous_names).visit_item_trait_mut(&mut synchronous);
            for item in &mut synchronous.items {
                if let TraitItem::Fn(method) = item {
                    method.sig.asyncness = None;
                }
            }
            output.extend(quote!(#item #synchronous));
        } else {
            output.extend(quote!(#item));
        }
    }
    for mut function in functions {
        let span = function.span();
        let synchronous_name: Ident =
            required_attribute(&mut function.attrs, "synchronous", span)?.parse_args()?;
        let bindings = required_attribute(&mut function.attrs, "capabilities", span)?
            .parse_args_with(Punctuated::<Binding, Token![,]>::parse_terminated)?;
        let values = required_attribute(&mut function.attrs, "passive_values", span)?
            .parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)?;
        let mut markers = RejectMarkers::default();
        for attribute in &mut function.attrs {
            markers.visit_attribute_mut(attribute);
        }
        markers.visit_signature_mut(&mut function.sig);
        if let Some(error) = markers.error {
            return Err(error);
        }
        if function.sig.asyncness.is_none()
            || !matches!(function.sig.safety, syn::Safety::Default)
            || function.sig.abi.is_some()
            || function.sig.constness.is_some()
        {
            return Err(Error::new_spanned(
                &function.sig,
                "shared bodies require ordinary async functions",
            ));
        }
        let mut capabilities = BTreeMap::new();
        for binding in bindings {
            let declaration = binding.declaration.to_string();
            let (asynchronous, methods) = if traits.contains_key(&declaration) {
                (
                    true,
                    inherited[&declaration]
                        .iter()
                        .map(|(name, operation)| (name.clone(), operation.progress))
                        .collect(),
                )
            } else if let Some(methods) = finite.get(&declaration) {
                (
                    false,
                    methods.iter().map(|name| (name.clone(), false)).collect(),
                )
            } else {
                return Err(Error::new_spanned(
                    binding.declaration,
                    "unknown capability declaration",
                ));
            };
            let Some(FnArg::Typed(argument)) = function.sig.inputs.iter().find(|argument| {
                matches!(argument, FnArg::Typed(argument) if matches!(&*argument.pat, Pat::Ident(name) if name.ident == binding.receiver))
            }) else {
                return Err(Error::new_spanned(binding.receiver, "capability must be a function parameter"));
            };
            if asynchronous {
                let syn::Type::Reference(reference) = &*argument.ty else {
                    return Err(Error::new_spanned(
                        &argument.ty,
                        "effects require a borrowed type parameter",
                    ));
                };
                let syn::Type::Path(ty) = &*reference.elem else {
                    return Err(Error::new_spanned(
                        &argument.ty,
                        "effects require a borrowed type parameter",
                    ));
                };
                if ty.qself.is_some()
                    || !ty.path.get_ident().is_some_and(|parameter| {
                        has_effect_bound(&function, parameter, &declaration)
                    })
                {
                    return Err(Error::new_spanned(
                        &argument.ty,
                        "effect parameter requires its declared trait bound",
                    ));
                }
            }
            if !asynchronous
                && !matches!(&*argument.ty, syn::Type::Path(ty) if ty.qself.is_none() && ty.path.is_ident(&declaration))
            {
                return Err(Error::new_spanned(
                    &argument.ty,
                    "finite parameter must use its declared concrete type",
                ));
            }
            if capabilities
                .insert(
                    binding_name(&binding.receiver),
                    Capability {
                        asynchronous,
                        methods,
                    },
                )
                .is_some()
            {
                return Err(Error::new_spanned(
                    binding.receiver,
                    "duplicate capability receiver",
                ));
            }
        }
        let mut validator = Body {
            capabilities,
            values: values
                .iter()
                .filter_map(|path| {
                    path.segments
                        .first()
                        .map(|segment| segment.ident.to_string())
                })
                .collect(),
            constructors: values
                .iter()
                .map(|path| path.to_token_stream().to_string())
                .collect(),
            scopes: vec![BTreeMap::new()],
            loop_depth: 0,
            in_header: false,
            error: None,
        };
        for parameter in &function.sig.generics.params {
            let name = match parameter {
                syn::GenericParam::Type(parameter) => &parameter.ident,
                syn::GenericParam::Const(parameter) => &parameter.ident,
                syn::GenericParam::Lifetime(_) => continue,
            };
            let spelling = name.to_string();
            let spelling = spelling.trim_start_matches("r#");
            if finite
                .keys()
                .any(|name| name.trim_start_matches("r#") == spelling)
                || validator.reserved_constructor(name)
            {
                return Err(Error::new_spanned(
                    name,
                    "generic parameters cannot replace finite types or passive constructors",
                ));
            }
        }
        for argument in &function.sig.inputs {
            let FnArg::Typed(argument) = argument else {
                return Err(Error::new_spanned(
                    argument,
                    "shared bodies do not have self receivers",
                ));
            };
            let Pat::Ident(name) = &*argument.pat else {
                return Err(Error::new_spanned(
                    &argument.pat,
                    "shared parameters require names",
                ));
            };
            if name.subpat.is_some() {
                return Err(Error::new_spanned(
                    name,
                    "shared parameters cannot introduce subpatterns",
                ));
            }
            if validator.reserved_constructor(&name.ident) {
                return Err(Error::new_spanned(
                    name,
                    "formal parameters cannot shadow passive constructors",
                ));
            }
            validator.bind(&name.ident, LocalKind::Ordinary);
        }
        validator.visit_block_mut(&mut function.block);
        if let Some(error) = validator.error {
            return Err(error);
        }
        let mut synchronous = function.clone();
        synchronous.sig.ident = synchronous_name;
        synchronous.sig.asyncness = None;
        SynchronousBounds(&synchronous_names).visit_signature_mut(&mut synchronous.sig);
        Lowerer { synchronous: false }.visit_block_mut(&mut function.block);
        Lowerer { synchronous: true }.visit_block_mut(&mut synchronous.block);
        output.extend(quote!(#function #synchronous));
    }
    Ok(output)
}

struct Capability {
    asynchronous: bool,
    methods: BTreeMap<String, bool>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalKind {
    Ordinary,
    PassiveState,
}

struct Body {
    capabilities: BTreeMap<String, Capability>,
    values: BTreeSet<String>,
    constructors: BTreeSet<String>,
    scopes: Vec<BTreeMap<String, LocalKind>>,
    loop_depth: usize,
    in_header: bool,
    error: Option<Error>,
}

impl Body {
    fn reject(&mut self, span: Span, message: &str) {
        if self.error.is_none() {
            self.error = Some(Error::new(span, message));
        }
    }

    fn bind(&mut self, name: &Ident, kind: LocalKind) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(binding_name(name), kind);
        }
    }

    fn local(&self, name: &Ident) -> Option<LocalKind> {
        let name = binding_name(name);
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&name).copied())
    }

    fn attributes(&mut self, attributes: &[Attribute]) {
        for attribute in attributes {
            if internal_marker(attribute) {
                self.reject(attribute.span(), "family marker is not valid here");
            }
            if self.in_header && conditional_attribute(std::slice::from_ref(attribute)) {
                self.reject(
                    attribute.span(),
                    "cursor loop headers cannot be conditional",
                );
            }
        }
    }

    fn reserved_constructor(&self, name: &Ident) -> bool {
        let spelling = name.to_string();
        let spelling = spelling.trim_start_matches("r#");
        self.values
            .iter()
            .any(|name| name.trim_start_matches("r#") == spelling)
            || matches!(spelling, "Ok" | "Some" | "None")
    }

    fn passive_path(&self, path: &syn::Path) -> bool {
        path.leading_colon.is_none()
            && path.segments.len() <= 2
            && path
                .segments
                .iter()
                .all(|segment| matches!(segment.arguments, syn::PathArguments::None))
            && path
                .segments
                .first()
                .is_some_and(|segment| self.values.contains(&segment.ident.to_string()))
    }

    fn method(&mut self, call: &mut syn::ExprMethodCall, awaited: bool) {
        self.attributes(&call.attrs);
        let Expr::Path(receiver) = &*call.receiver else {
            self.reject(
                call.span(),
                "operation requires its declared capability receiver",
            );
            return;
        };
        self.attributes(&receiver.attrs);
        let capability = receiver
            .path
            .get_ident()
            .filter(|_| receiver.qself.is_none())
            .and_then(|name| self.capabilities.get(&binding_name(name)));
        if !capability.is_some_and(|capability| {
            capability.asynchronous == awaited
                && capability.methods.contains_key(&binding_name(&call.method))
        }) || call.turbofish.is_some()
        {
            self.reject(
                call.span(),
                "operation must use a declared method and its declared await boundary",
            );
        }
        for argument in &mut call.args {
            self.visit_expr_mut(argument);
        }
    }

    fn cursor_loop(&mut self, expression: &mut syn::ExprWhile) {
        let mut attributes = expression.attrs.clone();
        let marker = match take_attribute(&mut attributes, "cursor_loop") {
            Ok(Some(marker)) => marker,
            Ok(None) => {
                self.reject(
                    expression.span(),
                    "expression is outside the shared capability grammar",
                );
                return;
            }
            Err(error) => {
                self.error = self.error.take().or(Some(error));
                return;
            }
        };
        if !matches!(marker.meta, syn::Meta::Path(_)) {
            self.reject(marker.span(), "expected #[cursor_loop]");
        }
        if conditional_attribute(&attributes) {
            self.reject(
                expression.span(),
                "cursor loop headers cannot be conditional",
            );
        }
        self.attributes(&attributes);
        if expression.label.is_some() {
            self.reject(expression.span(), "cursor loops cannot have labels");
        }
        if self.in_header {
            self.reject(
                expression.span(),
                "cursor loop headers cannot contain loops or loop exits",
            );
        }
        let Expr::Let(condition) = &mut *expression.cond else {
            self.reject(
                expression.cond.span(),
                "cursor loop requires let Some(name) = capability.operation(...).await?",
            );
            return;
        };
        let Pat::TupleStruct(pattern) = &mut *condition.pat else {
            self.reject(
                condition.pat.span(),
                "cursor loop requires an unmodified Some(name) pattern",
            );
            return;
        };
        if !pattern.attrs.is_empty() || !pattern.path.is_ident("Some") || pattern.elems.len() != 1 {
            self.reject(
                pattern.span(),
                "cursor loop requires an unmodified Some(name) pattern",
            );
            return;
        }
        let Some(Pat::Ident(binding)) = pattern.elems.first_mut() else {
            self.reject(
                pattern.span(),
                "cursor loop requires an unmodified Some(name) pattern",
            );
            return;
        };
        if !binding.attrs.is_empty()
            || binding.by_ref.is_some()
            || binding.mutability.is_some()
            || binding.subpat.is_some()
        {
            self.reject(
                binding.span(),
                "cursor loop requires an unmodified Some(name) pattern",
            );
        }
        if self.reserved_constructor(&binding.ident) {
            self.reject(
                binding.span(),
                "capability and passive constructor names cannot be shadowed",
            );
        }
        let progress = match &*condition.expr {
            Expr::Try(attempt) => match &*attempt.expr {
                Expr::Await(awaited) => match &*awaited.base {
                    Expr::MethodCall(call) if call.turbofish.is_none() => match &*call.receiver {
                        Expr::Path(receiver) if receiver.qself.is_none() => receiver
                            .path
                            .get_ident()
                            .and_then(|name| self.capabilities.get(&binding_name(name)))
                            .is_some_and(|capability| {
                                capability.asynchronous
                                    && capability.methods.get(&binding_name(&call.method))
                                        == Some(&true)
                            }),
                        _ => false,
                    },
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        };
        if !progress {
            self.reject(
                condition.expr.span(),
                "cursor loop requires a declared progress operation followed by .await?",
            );
        }
        let in_header = self.in_header;
        self.in_header = true;
        self.attributes(&condition.attrs);
        self.visit_expr_mut(&mut condition.expr);
        self.in_header = in_header;
        self.scopes.push(BTreeMap::new());
        self.visit_pat_ident_mut(binding);
        self.loop_depth += 1;
        self.visit_block_mut(&mut expression.body);
        self.loop_depth -= 1;
        self.scopes.pop();
    }

    fn assignment(&mut self, assignment: &mut syn::ExprAssign) {
        for attributes in [
            assignment.attrs.as_slice(),
            expression_attributes(&assignment.left),
        ] {
            if conditional_attribute(attributes) {
                self.reject(
                    assignment.span(),
                    "passive-state assignments cannot be conditional",
                );
            }
            self.attributes(attributes);
        }
        let allowed = matches!(&*assignment.left, Expr::Path(path)
            if path.qself.is_none()
                && path.attrs.is_empty()
                && path.path.get_ident().is_some_and(|name| self.local(name) == Some(LocalKind::PassiveState)));
        if !allowed {
            self.reject(
                assignment.left.span(),
                "assignment requires a declared passive-state local",
            );
        }
        self.visit_expr_mut(&mut assignment.right);
    }
}

impl VisitMut for Body {
    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        self.scopes.push(BTreeMap::new());
        visit_mut::visit_block_mut(self, block);
        self.scopes.pop();
    }

    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if !matches!(expression, Expr::While(_)) {
            self.attributes(expression_attributes(expression));
        }
        match expression {
            Expr::Await(awaited) => {
                let Expr::MethodCall(call) = &mut *awaited.base else {
                    self.reject(awaited.span(), "only declared effects may be awaited");
                    return;
                };
                self.method(call, true);
            }
            Expr::MethodCall(call) => self.method(call, false),
            Expr::Call(call) => {
                if !matches!(&*call.func, Expr::Path(path) if path.qself.is_none() && (self.constructors.contains(&path.path.to_token_stream().to_string()) || path.path.is_ident("Ok") || path.path.is_ident("Some")))
                {
                    self.reject(
                        call.span(),
                        "only passive constructors may be called directly",
                    );
                }
                for argument in &mut call.args {
                    self.visit_expr_mut(argument);
                }
            }
            Expr::Struct(structure) => {
                if structure.qself.is_some() || !self.passive_path(&structure.path) {
                    self.reject(structure.span(), "undeclared passive value constructor");
                }
                visit_mut::visit_expr_struct_mut(self, structure);
            }
            Expr::Path(path) => {
                let local = path.qself.is_none()
                    && path.path.get_ident().is_some_and(|name| {
                        self.local(name).is_some()
                            && !self.capabilities.contains_key(&binding_name(name))
                    });
                if !local
                    && !(path.qself.is_none()
                        && (self.passive_path(&path.path) || path.path.is_ident("None")))
                {
                    self.reject(path.span(), "value is not a local or declared passive value; capabilities cannot escape");
                }
            }
            Expr::Macro(invocation) if invocation.mac.path.is_ident("matches") => {
                match syn::parse2::<MatchesInput>(invocation.mac.tokens.clone()) {
                    Ok(mut input) => {
                        self.visit_expr_mut(&mut input.expression);
                        self.scopes.push(BTreeMap::new());
                        self.visit_pat_mut(&mut input.pattern);
                        if let Some(guard) = &mut input.guard {
                            self.visit_expr_mut(guard);
                        }
                        self.scopes.pop();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Expr::Binary(binary) if matches!(binary.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) => {
                visit_mut::visit_expr_binary_mut(self, binary);
            }
            Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Not(_)) => {
                visit_mut::visit_expr_unary_mut(self, unary);
            }
            Expr::While(expression) => self.cursor_loop(expression),
            Expr::Break(exit) => {
                if self.in_header {
                    self.reject(
                        exit.span(),
                        "cursor loop headers cannot contain loops or loop exits",
                    );
                } else if self.loop_depth == 0 || exit.label.is_some() || exit.expr.is_some() {
                    self.reject(
                        exit.span(),
                        "only bare break inside a cursor loop is supported",
                    );
                }
            }
            Expr::Continue(exit) => {
                if self.in_header {
                    self.reject(
                        exit.span(),
                        "cursor loop headers cannot contain loops or loop exits",
                    );
                } else if self.loop_depth == 0 || exit.label.is_some() {
                    self.reject(
                        exit.span(),
                        "only bare continue inside a cursor loop is supported",
                    );
                }
            }
            Expr::Assign(assignment) => self.reject(
                assignment.span(),
                "passive-state assignment must be a standalone statement",
            ),
            Expr::If(conditional) => {
                self.scopes.push(BTreeMap::new());
                self.visit_expr_mut(&mut conditional.cond);
                self.visit_block_mut(&mut conditional.then_branch);
                self.scopes.pop();
                if let Some((_, branch)) = &mut conditional.else_branch {
                    self.visit_expr_mut(branch);
                }
            }
            Expr::Let(binding) => {
                self.visit_expr_mut(&mut binding.expr);
                self.visit_pat_mut(&mut binding.pat);
            }
            Expr::Match(selection) => {
                self.visit_expr_mut(&mut selection.expr);
                for arm in &mut selection.arms {
                    self.scopes.push(BTreeMap::new());
                    self.attributes(&arm.attrs);
                    self.visit_pat_mut(&mut arm.pat);
                    self.visit_expr_mut(&mut arm.body);
                    self.scopes.pop();
                }
            }
            Expr::Block(_)
            | Expr::Field(_)
            | Expr::Group(_)
            | Expr::Lit(_)
            | Expr::Paren(_)
            | Expr::Reference(_)
            | Expr::Return(_)
            | Expr::Try(_)
            | Expr::Tuple(_) => {
                visit_mut::visit_expr_mut(self, expression);
            }
            _ => self.reject(
                expression.span(),
                "expression is outside the shared capability grammar",
            ),
        }
    }

    fn visit_stmt_mut(&mut self, statement: &mut syn::Stmt) {
        match statement {
            syn::Stmt::Local(local) => {
                let mut attributes = local.attrs.clone();
                let marker = match take_attribute(&mut attributes, "passive_state") {
                    Ok(marker) => marker,
                    Err(error) => {
                        self.error = self.error.take().or(Some(error));
                        return;
                    }
                };
                if marker.is_some() && conditional_attribute(&attributes) {
                    self.reject(
                        local.span(),
                        "passive-state declarations cannot be conditional",
                    );
                }
                self.attributes(&attributes);
                if !attributes.is_empty() {
                    self.reject(local.span(), "attributes on shared locals are unsupported");
                }
                if let Some(marker) = &marker {
                    if !matches!(marker.meta, syn::Meta::Path(_)) {
                        self.reject(marker.span(), "expected #[passive_state]");
                    }
                    if passive_binding(&local.pat).is_none()
                        || !local
                            .init
                            .as_ref()
                            .is_some_and(|init| init.diverge.is_none())
                    {
                        self.reject(
                            local.span(),
                            "passive state requires an initialized mutable local binding",
                        );
                    }
                }
                if let Some(initializer) = &mut local.init {
                    self.visit_expr_mut(&mut initializer.expr);
                    if let Some((_, diverge)) = &mut initializer.diverge {
                        self.visit_expr_mut(diverge);
                    }
                }
                self.visit_pat_mut(&mut local.pat);
                if marker.is_some()
                    && let Some(name) = passive_binding(&local.pat)
                {
                    self.bind(name, LocalKind::PassiveState);
                }
            }
            syn::Stmt::Expr(Expr::Assign(assignment), Some(_)) => self.assignment(assignment),
            syn::Stmt::Expr(expression, _) => self.visit_expr_mut(expression),
            _ => self.reject(
                statement.span(),
                "shared bodies cannot declare items or opaque statement macros",
            ),
        }
    }

    fn visit_pat_ident_mut(&mut self, pattern: &mut syn::PatIdent) {
        let name = pattern.ident.to_string();
        // Syn represents both bindings and bare unit variants as PatIdent.
        if name == "None"
            && pattern.attrs.is_empty()
            && pattern.by_ref.is_none()
            && pattern.mutability.is_none()
            && pattern.subpat.is_none()
        {
            return;
        }
        if self
            .capabilities
            .keys()
            .any(|capability| capability.trim_start_matches("r#") == name.trim_start_matches("r#"))
            || self.reserved_constructor(&pattern.ident)
        {
            self.reject(
                pattern.span(),
                "capability and passive constructor names cannot be shadowed",
            );
        }
        self.bind(&pattern.ident, LocalKind::Ordinary);
        visit_mut::visit_pat_ident_mut(self, pattern);
    }

    fn visit_pat_mut(&mut self, pattern: &mut Pat) {
        if matches!(pattern, Pat::Macro(_)) {
            self.reject(pattern.span(), "opaque patterns are unsupported");
        } else {
            visit_mut::visit_pat_mut(self, pattern);
        }
    }

    fn visit_attribute_mut(&mut self, attribute: &mut Attribute) {
        self.attributes(std::slice::from_ref(attribute));
    }
}

fn binding_name(name: &Ident) -> String {
    name.to_string().trim_start_matches("r#").to_owned()
}

fn internal_marker(attribute: &Attribute) -> bool {
    ["progress", "passive_state", "cursor_loop"]
        .into_iter()
        .any(|name| attribute.path().is_ident(name))
}

#[derive(Default)]
struct RejectMarkers {
    error: Option<Error>,
}

impl VisitMut for RejectMarkers {
    fn visit_attribute_mut(&mut self, attribute: &mut Attribute) {
        if self.error.is_none() && internal_marker(attribute) {
            self.error = Some(Error::new_spanned(
                attribute,
                "family marker is not valid here",
            ));
        }
    }
}

fn passive_binding(pattern: &Pat) -> Option<&Ident> {
    let pattern = if let Pat::Type(typed) = pattern {
        if !typed.attrs.is_empty() {
            return None;
        }
        &*typed.pat
    } else {
        pattern
    };
    match pattern {
        Pat::Ident(binding)
            if binding.attrs.is_empty()
                && binding.by_ref.is_none()
                && binding.mutability.is_some()
                && binding.subpat.is_none() =>
        {
            Some(&binding.ident)
        }
        _ => None,
    }
}

fn expression_attributes(expression: &Expr) -> &[Attribute] {
    match expression {
        Expr::Assign(expr) => &expr.attrs,
        Expr::Await(expr) => &expr.attrs,
        Expr::Binary(expr) => &expr.attrs,
        Expr::Block(expr) => &expr.attrs,
        Expr::Break(expr) => &expr.attrs,
        Expr::Call(expr) => &expr.attrs,
        Expr::Continue(expr) => &expr.attrs,
        Expr::Field(expr) => &expr.attrs,
        Expr::Group(expr) => &expr.attrs,
        Expr::If(expr) => &expr.attrs,
        Expr::Let(expr) => &expr.attrs,
        Expr::Lit(expr) => &expr.attrs,
        Expr::Macro(expr) => &expr.attrs,
        Expr::Match(expr) => &expr.attrs,
        Expr::MethodCall(expr) => &expr.attrs,
        Expr::Paren(expr) => &expr.attrs,
        Expr::Path(expr) => &expr.attrs,
        Expr::Reference(expr) => &expr.attrs,
        Expr::Return(expr) => &expr.attrs,
        Expr::Struct(expr) => &expr.attrs,
        Expr::Try(expr) => &expr.attrs,
        Expr::Tuple(expr) => &expr.attrs,
        Expr::Unary(expr) => &expr.attrs,
        Expr::While(expr) => &expr.attrs,
        _ => &[],
    }
}

struct Lowerer {
    synchronous: bool,
}

impl VisitMut for Lowerer {
    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        let mut statements = Vec::new();
        for mut statement in std::mem::take(&mut block.stmts) {
            let state = if let syn::Stmt::Local(local) = &mut statement
                && local
                    .attrs
                    .iter()
                    .any(|attribute| attribute.path().is_ident("passive_state"))
            {
                local
                    .attrs
                    .retain(|attribute| !attribute.path().is_ident("passive_state"));
                passive_binding(&local.pat).cloned()
            } else {
                None
            };
            self.visit_stmt_mut(&mut statement);
            statements.push(statement);
            if let Some(state) = state {
                let name = if binding_name(&state) == "__shared_semantic_require_copy" {
                    "__shared_semantic_require_copy_state"
                } else {
                    "__shared_semantic_require_copy"
                };
                let helper = Ident::new(name, Span::mixed_site());
                statements.push(syn::parse_quote! {
                    {
                        fn #helper<T: ::core::marker::Copy>(_: &T) {}
                        #helper(&#state);
                    }
                });
            }
        }
        block.stmts = statements;
    }

    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::While(cursor) = expression {
            cursor
                .attrs
                .retain(|attribute| !attribute.path().is_ident("cursor_loop"));
            if let Expr::Let(condition) = &mut *cursor.cond
                && let Pat::TupleStruct(pattern) = &mut *condition.pat
            {
                pattern.path = syn::parse_quote!(::core::option::Option::Some);
            }
        }
        if let Expr::Macro(invocation) = expression
            && invocation.mac.path.is_ident("matches")
        {
            // The validator has already parsed this non-opaque macro's expression and guard.
            if let Ok(mut input) = syn::parse2::<MatchesInput>(invocation.mac.tokens.clone()) {
                self.visit_expr_mut(&mut input.expression);
                if let Some(guard) = &mut input.guard {
                    self.visit_expr_mut(guard);
                }
                let value = input.expression;
                let pattern = input.pattern;
                let guard = input.guard.map(|guard| quote!(if #guard));
                invocation.mac.tokens = quote!(#value, #pattern #guard);
            }
            return;
        }
        if self.synchronous
            && let Expr::Await(awaited) = expression
            && let Expr::MethodCall(call) = &mut *awaited.base
        {
            let mut lowered = call.clone();
            lowered.attrs.splice(0..0, awaited.attrs.iter().cloned());
            visit_mut::visit_expr_method_call_mut(self, &mut lowered);
            *expression = Expr::MethodCall(lowered);
        } else {
            visit_mut::visit_expr_mut(self, expression);
        }
    }
}
