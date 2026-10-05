//! Direct synchronous lowering for the canonical effectful relation dispatcher.

use proc_macro2::{Ident, Span, TokenStream, TokenTree};
use quote::quote;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Error, Expr, FnArg, GenericArgument, GenericParam, ItemFn, Pat, PathArguments, Result,
    ReturnType, Signature, Stmt, Type, TypeParamBound,
};

use super::is_identifier;

#[cfg(test)]
mod tests;

pub(super) fn expand(arguments: TokenStream, original: &TokenStream) -> Result<TokenStream> {
    if !arguments.is_empty() {
        return Err(Error::new_spanned(
            arguments,
            "dual_relation takes no arguments",
        ));
    }
    let mut synchronous: ItemFn = syn::parse2(original.clone())?;
    let (fields, disjointness) = lower_signature(&mut synchronous.sig)?;
    let mut lowerer = Lowerer {
        operation: true,
        result_position: false,
        disjointness,
        error: None,
    };
    // Any surviving use of E would refer to a generic removed from the synchronous method.
    lowerer.visit_signature_mut(&mut synchronous.sig);
    lowerer.block(&mut synchronous.block, true);
    if let Some(error) = lowerer.error {
        return Err(error);
    }
    synchronous.block.stmts.insert(0, fields);
    Ok(quote!(#original #synchronous))
}

fn lower_signature(signature: &mut Signature) -> Result<(Stmt, bool)> {
    let name = signature.ident.to_string();
    let Some(name) = name.strip_suffix("_with").filter(|name| !name.is_empty()) else {
        return Err(Error::new_spanned(
            &signature.ident,
            "dual_relation requires a function ending in _with",
        ));
    };
    if signature.asyncness.take().is_none() {
        return Err(Error::new_spanned(
            &signature.ident,
            "dual_relation requires an async function",
        ));
    }
    let Some(FnArg::Typed(provider)) = signature.inputs.last() else {
        return Err(Error::new_spanned(
            &signature.inputs,
            "dual_relation requires a final effects: &E parameter",
        ));
    };
    let valid_pattern = matches!(provider.pat.as_ref(), Pat::Ident(pattern) if pattern.ident == "effects" && pattern.by_ref.is_none() && pattern.mutability.is_none() && pattern.subpat.is_none());
    let valid_type = matches!(provider.ty.as_ref(), Type::Reference(reference) if reference.mutability.is_none() && matches!(reference.elem.as_ref(), Type::Path(path) if path.qself.is_none() && path.path.is_ident("E")));
    if !valid_pattern || !valid_type {
        return Err(Error::new_spanned(
            provider,
            "dual_relation requires a final effects: &E parameter",
        ));
    }
    let provider_type = signature
        .generics
        .type_params()
        .find(|parameter| parameter.ident == "E");
    let Some(provider_type) = provider_type else {
        return Err(Error::new_spanned(
            &signature.generics,
            "dual_relation requires E: PairEffects<'a, 'c, 'db>",
        ));
    };
    let expected_bound: TypeParamBound = syn::parse_quote!(PairEffects<'a, 'c, 'db>);
    let qualified_bound: TypeParamBound = syn::parse_quote!(pair_effects::PairEffects<'a, 'c, 'db>);
    let disjointness_bound: TypeParamBound = syn::parse_quote!(DisjointnessEffects<'a, 'c, 'db>);
    let qualified_disjointness_bound: TypeParamBound =
        syn::parse_quote!(disjointness_effects::DisjointnessEffects<'a, 'c, 'db>);
    let disjointness = provider_type.bounds.first() == Some(&disjointness_bound)
        || provider_type.bounds.first() == Some(&qualified_disjointness_bound);
    if provider_type.default.is_some()
        || provider_type.bounds.len() != 1
        || (!disjointness && provider_type.bounds.first() != Some(&expected_bound)
            && provider_type.bounds.first() != Some(&qualified_bound))
    {
        return Err(Error::new_spanned(
            provider_type,
            "dual_relation requires exactly E: PairEffects<'a, 'c, 'db> or E: DisjointnessEffects<'a, 'c, 'db>",
        ));
    }
    let ReturnType::Type(_, output) = &signature.output else {
        return Err(Error::new_spanned(
            &signature.output,
            "dual_relation requires Result<T, E::Error>",
        ));
    };
    let Type::Path(path) = output.as_ref() else {
        return Err(Error::new_spanned(
            output,
            "dual_relation requires Result<T, E::Error>",
        ));
    };
    let segment = path.path.segments.last().filter(|segment| {
        path.qself.is_none()
            && path.path.leading_colon.is_none()
            && path.path.segments.len() == 1
            && segment.ident == "Result"
    });
    let Some(segment) = segment else {
        return Err(Error::new_spanned(
            output,
            "dual_relation requires Result<T, E::Error>",
        ));
    };
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(Error::new_spanned(
            output,
            "dual_relation requires Result<T, E::Error>",
        ));
    };
    let expected_error: GenericArgument = syn::parse_quote!(E::Error);
    let Some(GenericArgument::Type(result)) = arguments.args.first() else {
        return Err(Error::new_spanned(
            output,
            "dual_relation requires Result<T, E::Error>",
        ));
    };
    if arguments.args.len() != 2 || arguments.args.last() != Some(&expected_error) {
        return Err(Error::new_spanned(
            output,
            "dual_relation requires Result<T, E::Error>",
        ));
    }
    let result = result.clone();
    let database_index = usize::from(matches!(signature.inputs.first(), Some(FnArg::Receiver(_))));
    let fields_binding = if disjointness {
        let Some(FnArg::Typed(fields)) = signature.inputs.iter().nth(database_index) else {
            return Err(Error::new_spanned(
                &signature.inputs,
                "dual_relation requires fields: RelationFieldReads<'db> before its operands",
            ));
        };
        let expected_fields: Type = syn::parse_quote!(RelationFieldReads<'db>);
        let qualified_fields: Type = syn::parse_quote!(field_reads::RelationFieldReads<'db>);
        if !matches!(fields.pat.as_ref(), Pat::Ident(pattern) if pattern.ident == "fields" && pattern.by_ref.is_none() && pattern.mutability.is_none() && pattern.subpat.is_none())
            || (fields.ty.as_ref() != &expected_fields && fields.ty.as_ref() != &qualified_fields)
        {
            return Err(Error::new_spanned(
                fields,
                "dual_relation requires fields: RelationFieldReads<'db> before its operands",
            ));
        }
        let binding = if fields.ty.as_ref() == &qualified_fields {
            syn::parse_quote!(let fields = field_reads::RelationFieldReads::new(db);)
        } else {
            syn::parse_quote!(let fields = RelationFieldReads::new(db);)
        };
        signature.inputs[database_index] = syn::parse_quote!(db: &'db dyn Db);
        binding
    } else {
        for input in &signature.inputs {
            if let FnArg::Typed(input) = input
                && (matches!(input.pat.as_ref(), Pat::Ident(pattern) if pattern.ident == "fields" || pattern.ident == "db")
                    || is_ordinary_reader(&input.ty))
            {
                return Err(Error::new_spanned(
                    input,
                    "pair relations cannot receive database or field-reader parameters",
                ));
            }
        }
        signature
            .inputs
            .insert(database_index, syn::parse_quote!(db: &'db dyn Db));
        syn::parse_quote!(let fields = RelationFieldReads::new(db);)
    };
    signature.ident = Ident::new(name, signature.ident.span());
    signature.inputs.pop();
    signature.generics.params = signature.generics.params.clone().into_iter().filter(|parameter| !matches!(parameter, GenericParam::Type(parameter) if parameter.ident == "E")).collect();
    if signature.generics.params.is_empty() {
        signature.generics.lt_token = None;
        signature.generics.gt_token = None;
    }
    signature.output = syn::parse_quote!(-> #result);
    Ok((fields_binding, disjointness))
}

fn is_ordinary_reader(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => is_ordinary_reader(&reference.elem),
        Type::Path(path) => path.path.segments.last().is_some_and(|segment| {
            segment.ident == "RelationFieldReads" || segment.ident == "FieldReads"
        }),
        Type::TraitObject(object) => object.bounds.iter().any(|bound| {
            matches!(bound, TypeParamBound::Trait(bound) if bound.path.segments.last().is_some_and(|segment| segment.ident == "Db" || segment.ident == "Database"))
        }),
        _ => false,
    }
}

fn is_stored_field_getter(name: &Ident) -> bool {
    matches!(
        name.to_string().as_str(),
        "type_form_argument"
            | "field_default"
            | "field_converter"
            | "method_wrapper_kind"
            | "method_wrapper_type"
            | "partial_wrapped"
            | "partial_callable"
            | "interned_type"
            | "type_is_argument"
            | "type_guard_return"
    )
}

fn checker_method_arity(name: &Ident) -> Option<usize> {
    match name.to_string().as_str() {
        "check_type_pair"
        | "check_typevar_subclass_relation_to_target"
        | "check_newtype_pair"
        | "check_source_union"
        | "check_target_union"
        | "check_target_intersection"
        | "check_source_intersection"
        | "check_source_typevar_bounds"
        | "check_function_pair"
        | "check_bound_method_pair"
        | "check_known_bound_method_pair"
        | "check_callable_pair"
        | "check_callable_signature_pair"
        | "check_callable_source"
        | "check_type_satisfies_protocol"
        | "check_meta_type_satisfies_protocol"
        | "check_typeddict_pair"
        | "check_class_pair"
        | "check_subclassof_pair"
        | "check_nominal_instance_pair"
        | "check_property_instance_pair"
        | "check_typeddict_fallback"
        | "check_bound_super_pair"
        | "when_recursive_types_relate_by_arguments"
        | "implied_typevar_relation"
        | "lazy_typevar_upper_constraint"
        | "lazy_typevar_lower_constraint"
        | "same_typevar_occurrence"
        | "nominal_has_known_class"
        | "same_sentinel"
        | "wrapper_matches_nominal"
        | "nominal_class_is_known"
        | "union_contains_type"
        | "intersection_positive_contains"
        | "intersection_negative_contains"
        | "check_string_literal_nominal"
        | "check_bytes_literal_nominal"
        | "check_enum_instance_literal" => Some(3),
        "protocol_is_equivalent_to_object"
        | "unfold_recursive"
        | "alias_value"
        | "union_has_aliases"
        | "expand_union_aliases"
        | "subclass_instance"
        | "class_default_specialization"
        | "class_instance"
        | "known_instance_type_form_argument"
        | "special_form_type_form_argument"
        | "enum_remaining_literals"
        | "enum_intersection"
        | "lookup_wrapped_function"
        | "specialize_partial_instance"
        | "union_contains_dynamic"
        | "intersection_contains_nondivergent_dynamic"
        | "intersection_contains_dynamic"
        | "instance_approximation"
        | "typevar_is_inferable"
        | "typevar_is_typevartuple"
        | "is_exact_tuple_instance"
        | "is_variadic_exact_tuple_instance"
        | "unpacked_typevartuple"
        | "typevar_domain"
        | "callable_is_gradual_paramspec_value"
        | "callable_is_top_paramspec_value"
        | "callable_is_bottom_paramspec_value"
        | "typevar_constraints"
        | "typevar_upper_bound"
        | "typevar_bound_or_constraints"
        | "newtype_concrete_base"
        | "type_is_always_falsy"
        | "type_is_always_truthy"
        | "callable_signatures"
        | "function_callable_signatures"
        | "known_class_instance"
        | "literal_fallback_instance"
        | "callable_runtime_class"
        | "subclass_inner_class"
        | "class_literal_metaclass_instance"
        | "class_metaclass_instance"
        | "subclass_metaclass_instance"
        | "special_form_instance_fallback"
        | "known_instance_fallback"
        | "property_instance_fallback" => Some(2),
        _ => None,
    }
}

fn disjointness_method_arity(name: &Ident) -> Option<usize> {
    match name.to_string().as_str() {
        "disjointness_clear_context" | "disjointness_has_context" => Some(1),
        "type_is_always_falsy"
        | "type_is_always_truthy"
        | "disjointness_bool_nominal"
        | "disjointness_bound_method_other"
        | "disjointness_descriptor_other"
        | "disjointness_callable_final_nominal"
        | "disjointness_module_nominal"
        | "disjointness_slot_other"
        | "disjointness_super_other"
        | "disjointness_typeddict_other"
        | "disjointness_transposed_typevar"
        | "disjointness_instance_approximation"
        | "disjointness_typevar_is_inferable"
        | "disjointness_nominal_is_final"
        | "disjointness_callable_runtime_class"
        | "disjointness_known_instance"
        | "disjointness_bound_function"
        | "disjointness_enum_class"
        | "disjointness_enum_aliases_known"
        | "disjointness_alias_origin"
        | "disjointness_boolean" => Some(2),
        "disjointness_left_enum_complement"
        | "disjointness_right_enum_complement"
        | "disjointness_subclass_typeform"
        | "disjointness_typevar_other"
        | "disjointness_typevar_instance"
        | "disjointness_typevar_bounds"
        | "disjointness_union"
        | "check_property_instance_pair"
        | "disjointness_interned_pair"
        | "disjointness_method_pair"
        | "disjointness_alias_specializations"
        | "disjointness_class_alias"
        | "disjointness_subclass_class"
        | "disjointness_subclass_alias"
        | "check_subclassof_pair"
        | "disjointness_subclass_other"
        | "disjointness_special_form_nominal"
        | "disjointness_known_instance_nominal"
        | "disjointness_literal_nominal"
        | "disjointness_newtype_other"
        | "disjointness_class_nominal"
        | "disjointness_alias_nominal"
        | "disjointness_function_nominal"
        | "disjointness_callable_other"
        | "disjointness_bound_method_fallback"
        | "disjointness_known_method_other"
        | "check_newtype_pair"
        | "disjointness_property_other"
        | "disjointness_bound_super_pair"
        | "disjointness_same_typevar"
        | "disjointness_negative_contains_typevar"
        | "disjointness_same_wrapper_kind"
        | "disjointness_function_names_differ"
        | "disjointness_same_sentinel"
        | "disjointness_literal_kinds_differ"
        | "disjointness_types_differ"
        | "check_type_pair"
        | "check_type_pair_impl" => Some(3),
        "disjointness_left_alias"
        | "disjointness_right_alias"
        | "disjointness_left_recursive"
        | "disjointness_right_recursive" => Some(4),
        "disjointness_intersections"
        | "disjointness_left_intersection"
        | "disjointness_right_intersection"
        | "disjointness_wrappers"
        | "disjointness_partials"
        | "disjointness_protocols"
        | "disjointness_protocol_special_form"
        | "disjointness_protocol_known_instance"
        | "disjointness_protocol_members"
        | "disjointness_protocol_nominal"
        | "disjointness_protocol_other"
        | "disjointness_bound_method_functions"
        | "disjointness_nominal_pair"
        | "disjointness_typeddicts" => Some(5),
        _ => None,
    }
}

struct Lowerer {
    operation: bool,
    result_position: bool,
    disjointness: bool,
    error: Option<Error>,
}

impl Lowerer {
    fn reject(&mut self, span: Span, message: &str) {
        if self.error.is_none() {
            self.error = Some(Error::new(span, message));
        }
    }

    fn expression(&mut self, expression: &mut Expr, result_position: bool) {
        let previous = self.result_position;
        self.result_position = result_position;
        self.visit_expr_mut(expression);
        self.result_position = previous;
    }

    fn block(&mut self, block: &mut syn::Block, result_position: bool) {
        let last = block.stmts.len().saturating_sub(1);
        for (index, statement) in block.stmts.iter_mut().enumerate() {
            if let Stmt::Expr(expression, None) = statement {
                self.expression(expression, result_position && index == last);
            } else {
                let previous = self.result_position;
                self.result_position = false;
                self.visit_stmt_mut(statement);
                self.result_position = previous;
            }
        }
    }

    fn callback(&mut self, expression: &Expr, arity: usize) -> Result<Expr> {
        let Expr::Closure(mut closure) = expression.clone() else {
            return Err(Error::new_spanned(
                expression,
                "relation callbacks require |...| async { ... }",
            ));
        };
        if closure.asyncness.is_some()
            || closure.inputs.len() != arity
            || !matches!(closure.output, ReturnType::Default)
        {
            return Err(Error::new_spanned(
                expression,
                "relation callback has an unsupported signature",
            ));
        }
        let Expr::Async(body) = *closure.body else {
            return Err(Error::new_spanned(
                expression,
                "relation callbacks require |...| async { ... }",
            ));
        };
        if !body.attrs.is_empty() {
            return Err(Error::new_spanned(
                expression,
                "relation callback async blocks do not support attributes",
            ));
        }
        if let Some(capture) = body.capture {
            closure.capture = Some(capture);
        }
        for input in &mut closure.inputs {
            self.visit_pat_mut(input);
        }
        let mut body = body.block;
        self.block(&mut body, true);
        closure.body = Box::new(syn::parse_quote!(#body));
        Ok(Expr::Closure(closure))
    }

    fn lower_await(&mut self, awaited: &syn::ExprAwait) -> Result<Expr> {
        if !self.operation {
            return Err(Error::new_spanned(
                awaited,
                "relation awaits are only supported in the operation and declared callbacks",
            ));
        }
        let Expr::MethodCall(call) = awaited.base.as_ref() else {
            return Err(Error::new_spanned(
                awaited,
                "relation awaits require a declared effect or _with method",
            ));
        };
        if is_identifier(&call.receiver, "effects") {
            if call.turbofish.is_some() || !call.attrs.is_empty() || !awaited.attrs.is_empty() {
                return Err(Error::new_spanned(
                    call,
                    "relation effect calls do not support attributes or generic arguments",
                ));
            }
            let mut args: Vec<_> = call.args.iter().cloned().collect();
            let method = &call.method;
            if !self.disjointness && is_stored_field_getter(method) {
                if args.len() != 1 {
                    return Err(Error::new_spanned(
                        call,
                        "relation stored-field effects require one value argument",
                    ));
                }
                self.expression(&mut args[0], false);
                let value = &args[0];
                return syn::parse2(quote!(fields.#method(#value)));
            }
            if args.is_empty() || is_identifier(&args[0], "db") {
                return Err(Error::new_spanned(
                    call,
                    "relation effects require the checker as their first argument",
                ));
            }
            let arity = if self.disjointness {
                disjointness_method_arity(method)
            } else {
                checker_method_arity(method)
            };
            if let Some(arity) = arity {
                if args.len() != arity {
                    return Err(Error::new_spanned(
                        call,
                        "relation checker effect has an unsupported argument count",
                    ));
                }
                for argument in &mut args {
                    self.expression(argument, false);
                }
                let checker = &args[0];
                let arguments = &args[1..];
                return syn::parse2(quote!((#checker).#method(db, #(#arguments),*)));
            }
            match method.to_string().as_str() {
                "guard" if args.len() == 4 => {
                    let callback = self.callback(&args[3], 0)?;
                    for argument in &mut args[..3] {
                        self.expression(argument, false);
                    }
                    let (checker, source, target) = (&args[0], &args[1], &args[2]);
                    syn::parse2(
                        quote!((#checker).with_recursion_guard(db, #source, #target, #callback)),
                    )
                }
                "and" | "or" | "when_some_and" | "when_none_or" | "when_all" | "when_any"
                    if args.len() == 3 =>
                {
                    let arity = usize::from(method != "and" && method != "or");
                    let callback = self.callback(&args[2], arity)?;
                    for argument in &mut args[..2] {
                        self.expression(argument, false);
                    }
                    let (checker, value) = (&args[0], &args[1]);
                    syn::parse2(quote!((#value).#method(db, (#checker).constraints, #callback)))
                }
                "is_never_satisfied" | "is_always_satisfied" if args.len() == 2 => {
                    for argument in &mut args {
                        self.expression(argument, false);
                    }
                    let (checker, value) = (&args[0], &args[1]);
                    syn::parse2(quote!((#value).#method(db, (#checker).env)))
                }
                _ => Err(Error::new_spanned(
                    call,
                    "relation effect is not in the manifest or has an unsupported argument count",
                )),
            }
        } else if let Some(name) = call.method.to_string().strip_suffix("_with") {
            let callback_arity = match name {
                "and" | "or" => Some(0),
                "when_some_and" | "when_none_or" | "when_all" | "when_any" => Some(1),
                "check_type_pair_inner" => None,
                _ => {
                    return Err(Error::new_spanned(
                        call,
                        "canonical relation helper is not in the manifest",
                    ));
                }
            };
            if !call
                .args
                .last()
                .is_some_and(|argument| is_identifier(argument, "effects"))
            {
                return Err(Error::new_spanned(
                    call,
                    "canonical relation helpers require effects as their final argument",
                ));
            }
            let mut call = call.clone();
            call.method = Ident::new(name, call.method.span());
            call.args.pop();
            call.args = call.args.into_iter().collect();
            self.expression(&mut call.receiver, false);
            if let Some(arity) = callback_arity {
                if call.args.len() != 2 {
                    return Err(Error::new_spanned(
                        call,
                        "relation extension helpers require constraints, callback, effects",
                    ));
                }
                call.args[1] = self.callback(&call.args[1], arity)?;
                self.expression(&mut call.args[0], false);
                call.args.insert(0, syn::parse_quote!(db));
            } else {
                if call.args.len() != 2 {
                    return Err(Error::new_spanned(
                        call,
                        "canonical relation dispatch requires source, target, effects",
                    ));
                }
                for argument in &mut call.args {
                    self.expression(argument, false);
                }
                call.args.insert(0, syn::parse_quote!(db));
            }
            if let Some(arguments) = &mut call.turbofish {
                self.visit_angle_bracketed_generic_arguments_mut(arguments);
            }
            Ok(Expr::MethodCall(call))
        } else {
            Err(Error::new_spanned(
                call,
                "relation await is not in the effect or canonical-helper manifest",
            ))
        }
    }

    fn lower_step(&mut self, call: &syn::ExprMethodCall) -> Result<Expr> {
        if !self.operation || call.args.len() != 1 || call.turbofish.is_some() {
            return Err(Error::new_spanned(
                call,
                "effects.step requires a synchronous zero-argument closure",
            ));
        }
        let Expr::Closure(closure) = &call.args[0] else {
            return Err(Error::new_spanned(
                call,
                "effects.step requires a synchronous zero-argument closure",
            ));
        };
        if closure.asyncness.is_some() || !closure.inputs.is_empty() {
            return Err(Error::new_spanned(
                closure,
                "effects.step requires a synchronous zero-argument closure",
            ));
        }
        let mut closure = Expr::Closure(closure.clone());
        self.expression(&mut closure, false);
        syn::parse2(quote!((#closure)()))
    }
}

impl VisitMut for Lowerer {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Try(tried) = expression {
            let lowered = match tried.expr.as_ref() {
                Expr::Await(awaited) => Some(self.lower_await(awaited)),
                Expr::MethodCall(call)
                    if is_identifier(&call.receiver, "effects") && call.method == "step" =>
                {
                    Some(self.lower_step(call))
                }
                _ => None,
            };
            if let Some(lowered) = lowered {
                if !tried.attrs.is_empty() {
                    self.reject(
                        tried.span(),
                        "relation dependency expressions do not support attributes",
                    );
                    return;
                }
                match lowered {
                    Ok(lowered) => *expression = lowered,
                    Err(error) if self.error.is_none() => self.error = Some(error),
                    Err(_) => {}
                }
                return;
            }
        }
        match expression {
            Expr::Call(call)
                if self.operation && self.result_position && is_identifier(&call.func, "Ok") =>
            {
                if call.args.len() != 1 || !call.attrs.is_empty() {
                    self.reject(call.span(), "operation Ok requires one result");
                    return;
                }
                let mut inner = call.args[0].clone();
                self.expression(&mut inner, false);
                *expression = inner;
            }
            Expr::Call(call)
                if self.operation && self.result_position && is_identifier(&call.func, "Err") =>
            {
                self.reject(call.span(), "dual_relation cannot erase an operation error");
            }
            Expr::Return(returned) if self.operation => {
                if let Some(value) = &mut returned.expr {
                    self.expression(value, true);
                }
            }
            Expr::Block(block) => self.block(&mut block.block, self.result_position),
            Expr::If(branch) => {
                self.expression(&mut branch.cond, false);
                self.block(&mut branch.then_branch, self.result_position);
                if let Some((_, other)) = &mut branch.else_branch {
                    self.expression(other, self.result_position);
                }
            }
            Expr::Match(matched) => {
                self.expression(&mut matched.expr, false);
                for arm in &mut matched.arms {
                    self.visit_pat_mut(&mut arm.pat);
                    self.expression(&mut arm.body, self.result_position);
                }
            }
            Expr::Paren(parenthesized) => {
                self.expression(&mut parenthesized.expr, self.result_position);
            }
            Expr::Group(grouped) => self.expression(&mut grouped.expr, self.result_position),
            Expr::Try(tried) if self.operation => self.reject(
                tried.span(),
                "operation question marks require a declared relation dependency",
            ),
            Expr::Await(awaited) if self.operation && self.result_position => {
                match self.lower_await(awaited) {
                    Ok(lowered) => *expression = lowered,
                    Err(error) if self.error.is_none() => self.error = Some(error),
                    Err(_) => {}
                }
            }
            Expr::Await(awaited) => {
                self.reject(
                    awaited.span(),
                    "relation awaits require .await? or an operation result position",
                );
            }
            Expr::Async(asynchronous) => self.reject(
                asynchronous.span(),
                "async blocks are only allowed in declared relation callbacks",
            ),
            Expr::TryBlock(tried) => self.reject(
                tried.span(),
                "try blocks are not supported by dual_relation",
            ),
            Expr::Yield(yielded) => {
                self.reject(yielded.span(), "yield is not supported by dual_relation");
            }
            Expr::Verbatim(tokens) => self.reject(
                tokens.span(),
                "unsupported expression syntax in dual_relation",
            ),
            _ => {
                let result_position = self.result_position;
                self.result_position = false;
                visit_mut::visit_expr_mut(self, expression);
                self.result_position = result_position;
            }
        }
    }

    fn visit_expr_closure_mut(&mut self, closure: &mut syn::ExprClosure) {
        if closure.asyncness.is_some() {
            self.reject(
                closure.span(),
                "async closures are not supported by dual_relation",
            );
            return;
        }
        let operation = self.operation;
        self.operation = false;
        visit_mut::visit_expr_closure_mut(self, closure);
        self.operation = operation;
    }

    fn visit_expr_const_mut(&mut self, constant: &mut syn::ExprConst) {
        let operation = self.operation;
        self.operation = false;
        visit_mut::visit_expr_const_mut(self, constant);
        self.operation = operation;
    }

    fn visit_item_mut(&mut self, item: &mut syn::Item) {
        let operation = self.operation;
        self.operation = false;
        visit_mut::visit_item_mut(self, item);
        self.operation = operation;
    }

    fn visit_signature_mut(&mut self, signature: &mut Signature) {
        if signature.asyncness.is_some() {
            self.reject(
                signature.span(),
                "nested async functions are not supported by dual_relation",
            );
        }
        visit_mut::visit_signature_mut(self, signature);
    }

    fn visit_expr_path_mut(&mut self, path: &mut syn::ExprPath) {
        if path.path.is_ident("effects") || path.path.is_ident("r#effects") {
            self.reject(
                path.span(),
                "effects may only be used in a declared relation dependency",
            );
        }
        visit_mut::visit_expr_path_mut(self, path);
    }

    fn visit_type_path_mut(&mut self, path: &mut syn::TypePath) {
        if path
            .path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "E")
        {
            self.reject(
                path.span(),
                "the synchronous relation cannot retain the removed provider type E",
            );
        }
        visit_mut::visit_type_path_mut(self, path);
    }

    fn visit_pat_ident_mut(&mut self, pattern: &mut syn::PatIdent) {
        if pattern.ident == "effects" || pattern.ident == "r#effects" {
            self.reject(pattern.span(), "relation bodies cannot shadow effects");
        }
        visit_mut::visit_pat_ident_mut(self, pattern);
    }

    fn visit_pat_guard_mut(&mut self, pattern: &mut syn::PatGuard) {
        self.visit_pat_mut(&mut pattern.pat);
        self.expression(&mut pattern.guard, false);
    }

    fn visit_macro_mut(&mut self, invocation: &mut syn::Macro) {
        fn unsupported(tokens: TokenStream) -> bool {
            tokens.into_iter().any(|token| match token {
                TokenTree::Ident(ident) => matches!(
                    ident.to_string().as_str(),
                    "effects" | "r#effects" | "async" | "await" | "yield"
                ),
                TokenTree::Group(group) => unsupported(group.stream()),
                _ => false,
            })
        }
        if unsupported(invocation.tokens.clone()) {
            self.reject(
                invocation.span(),
                "macros cannot hide relation provider or async syntax",
            );
        }
    }
}
