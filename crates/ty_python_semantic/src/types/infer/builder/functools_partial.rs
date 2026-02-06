use ruff_python_ast as ast;

use super::TypeInferenceBuilder;
use crate::Db;
use crate::types::diagnostic::INVALID_ARGUMENT_TYPE;
use crate::types::generics::{ApplySpecialization, SpecializationBuilder};
use crate::types::signatures::{Parameter, Signature};
use crate::types::{
    ApplyTypeMappingVisitor, CallableSignature, CallableType, CallableTypeKind, Parameters, Type,
    TypeContext, TypeMapping,
};

/// `functools.partial` inference.
impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Try to infer a precise callable type for a `functools.partial(func, ...)` call.
    ///
    /// Returns `Some(callable_type)` if we can compute the remaining signature after
    /// binding some arguments, or `None` to fall back to the default `partial[T]` type.
    pub(super) fn infer_functools_partial_call(
        &self,
        arguments: &ast::Arguments,
    ) -> Option<Type<'db>> {
        let db = self.db();

        // We need at least one positional argument (the wrapped function).
        let func_expr = arguments.args.first()?;

        // If the first positional arg is starred (e.g. `partial(*args)`), we
        // can't statically determine the wrapped function; fall back.
        if func_expr.is_starred_expr() {
            return None;
        }

        let func_ty = self.expression_type(func_expr);

        let callable = func_ty.try_upcast_to_callable(db)?.exactly_one()?;
        let overloads = &callable.signatures(db).overloads;

        // Collect bound positional argument types, skipping the first argument.
        let mut bound_positional: Vec<Type<'db>> = Vec::new();
        for arg in &arguments.args[1..] {
            // Starred arguments with fixed-length tuple types are unpacked inline.
            if let Some(starred) = arg.as_starred_expr() {
                let iterable_ty = self.expression_type(&starred.value);
                if let Type::NominalInstance(nominal) = iterable_ty
                    && let Some(tuple_spec) = nominal.tuple_spec(db)
                    && let Some(fixed) = tuple_spec.as_fixed_length()
                {
                    bound_positional.extend(fixed.all_elements().iter().copied());
                } else {
                    return None;
                }
            } else {
                bound_positional.push(self.expression_type(arg));
            }
        }

        // Collect bound keyword arguments.
        let mut bound_keywords: Vec<(&str, Type<'db>)> = Vec::new();
        for kw in &arguments.keywords {
            if let Some(id) = &kw.arg {
                bound_keywords.push((id.as_str(), self.expression_type(&kw.value)));
            } else {
                let splat_ty = self.expression_type(&kw.value);
                // `**kwargs` splats with `TypedDict` types are unpacked inline.
                if let Some(typed_dict) = splat_ty.as_typed_dict() {
                    for (name, field) in typed_dict.items(db) {
                        bound_keywords.push((name.as_str(), field.declared_ty));
                    }
                } else {
                    return None;
                }
            }
        }

        // Specialize each overload and remove bound params.
        let new_overloads: Vec<_> = overloads
            .iter()
            .map(|sig| apply_partial_to_signature(db, sig, &bound_positional, &bound_keywords))
            .collect();

        // Type-check bound args against the wrapped function's parameter types.
        // For overloaded functions, only report if no overload matches.
        self.check_partial_bound_args(arguments, overloads, &bound_positional, &bound_keywords);

        let new_callable_sig = CallableSignature::from_overloads(new_overloads);
        Some(Type::Callable(CallableType::new(
            db,
            new_callable_sig,
            CallableTypeKind::Regular,
        )))
    }

    /// Check that bound arguments to `partial()` are compatible with the wrapped
    /// function's parameter types, and emit diagnostics for mismatches.
    ///
    /// For overloaded functions, diagnostics are only emitted when no overload
    /// accepts the bound arguments.
    fn check_partial_bound_args(
        &self,
        arguments: &ast::Arguments,
        overloads: &[Signature<'db>],
        bound_positional: &[Type<'db>],
        bound_keywords: &[(&str, Type<'db>)],
    ) {
        let db = self.db();

        // Check if any overload accepts all bound arguments. If at least one
        // overload matches, we don't emit diagnostics.
        let any_overload_matches = overloads.iter().any(|overload| {
            let signature = specialize_signature_from_bound_args(
                db,
                overload,
                bound_positional,
                bound_keywords,
            );
            let params = signature.parameters().as_slice();
            bound_args_match_params(db, params, bound_positional, bound_keywords)
        });

        if any_overload_matches {
            return;
        }

        // No overload matched; emit diagnostics against the first overload.
        let Some(overload) = overloads.first() else {
            return;
        };

        let signature =
            specialize_signature_from_bound_args(db, overload, bound_positional, bound_keywords);
        let params = signature.parameters().as_slice();
        let positional_arg_exprs = &arguments.args[1..];

        // Check bound positional args.
        let mut positional_consumed = 0usize;
        for param in params {
            if param.is_positional() && positional_consumed < bound_positional.len() {
                let arg_ty = bound_positional[positional_consumed];
                let param_ty = param.annotated_type();
                if !arg_ty.is_assignable_to(db, param_ty) {
                    if let Some(arg_expr) = positional_arg_exprs.get(positional_consumed)
                        && let Some(builder) =
                            self.context.report_lint(&INVALID_ARGUMENT_TYPE, arg_expr)
                    {
                        let param_name = param
                            .name()
                            .map(|n| format!("`{n}`"))
                            .unwrap_or_else(|| format!("{}", positional_consumed + 1));
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Argument to bound parameter {param_name} is incorrect"
                        ));
                        diagnostic.set_primary_message(format_args!(
                            "Expected `{}`, found `{}`",
                            param_ty.display(db),
                            arg_ty.display(db),
                        ));
                    }
                }
                positional_consumed += 1;
            }
        }

        // Check bound keyword args (skip positional-only params).
        for keyword in &arguments.keywords {
            let Some(arg_ident) = &keyword.arg else {
                continue;
            };
            let arg_ty = self.expression_type(&keyword.value);

            // Find the corresponding non-positional-only parameter.
            let Some(param) = params.iter().find(|p| {
                !p.is_positional_only()
                    && p.name().is_some_and(|n| n.as_str() == arg_ident.as_str())
            }) else {
                continue;
            };

            let param_ty = param.annotated_type();
            if !arg_ty.is_assignable_to(db, param_ty) {
                if let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, keyword) {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Argument to bound parameter `{}` is incorrect",
                        arg_ident.as_str()
                    ));
                    diagnostic.set_primary_message(format_args!(
                        "Expected `{}`, found `{}`",
                        param_ty.display(db),
                        arg_ty.display(db),
                    ));
                }
            }
        }
    }
}

/// Returns `true` if all bound arguments are assignable to their corresponding
/// parameter types.
fn bound_args_match_params<'db>(
    db: &'db dyn Db,
    params: &[Parameter<'db>],
    bound_positional: &[Type<'db>],
    bound_keywords: &[(&str, Type<'db>)],
) -> bool {
    let mut positional_consumed = 0usize;
    for param in params {
        if param.is_positional() && positional_consumed < bound_positional.len() {
            let arg_ty = bound_positional[positional_consumed];
            let param_ty = param.annotated_type();
            if !arg_ty.is_assignable_to(db, param_ty) {
                return false;
            }
            positional_consumed += 1;
        }
    }

    for &(kw_name, arg_ty) in bound_keywords {
        if let Some(param) = params
            .iter()
            .find(|p| !p.is_positional_only() && p.name().is_some_and(|n| n.as_str() == kw_name))
        {
            let param_ty = param.annotated_type();
            if !arg_ty.is_assignable_to(db, param_ty) {
                return false;
            }
        }
    }

    true
}

/// Specialize a generic signature by inferring type variables from bound arguments.
///
/// Returns the specialized signature (with all parameters intact) or a clone
/// if the signature is not generic.
fn specialize_signature_from_bound_args<'db>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    bound_positional: &[Type<'db>],
    bound_keywords: &[(&str, Type<'db>)],
) -> Signature<'db> {
    let Some(generic_context) = signature.generic_context else {
        return signature.clone();
    };

    let inferable = generic_context.inferable_typevars(db);
    let mut builder = SpecializationBuilder::new(db, inferable);
    let params = signature.parameters().as_slice();
    let mut positional_consumed = 0usize;

    // Infer type variable assignments from bound positional arguments.
    for param in params {
        if param.is_positional() && positional_consumed < bound_positional.len() {
            let _ = builder.infer(
                param.annotated_type(),
                bound_positional[positional_consumed],
            );
            positional_consumed += 1;
        }
    }

    // Infer type variable assignments from bound keyword arguments.
    for param in params {
        if let Some(name) = param.name() {
            if let Some(&(_, arg_ty)) = bound_keywords.iter().find(|(kw, _)| *kw == name.as_str()) {
                let _ = builder.infer(param.annotated_type(), arg_ty);
            }
        }
    }

    // Promote literal types (e.g., `Literal[1]` to `int`) in inferred type
    // variable assignments, since `partial()` creates a reusable callable.
    let mut builder = builder.mapped(generic_context, |_, _, ty| {
        ty.promote_literals(db, TypeContext::default())
    });
    let specialization = builder.build(generic_context);
    let type_mapping =
        TypeMapping::ApplySpecialization(ApplySpecialization::Specialization(specialization));
    signature.apply_type_mapping_impl(
        db,
        &type_mapping,
        TypeContext::default(),
        &ApplyTypeMappingVisitor::default(),
    )
}

/// Apply partial binding to a single signature, returning the remaining signature
/// after removing bound parameters.
///
/// For generic signatures, type variables are inferred from the bound arguments
/// and the signature is specialized before removing bound parameters.
fn apply_partial_to_signature<'db>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    bound_positional: &[Type<'db>],
    bound_keywords: &[(&str, Type<'db>)],
) -> Signature<'db> {
    let signature =
        specialize_signature_from_bound_args(db, signature, bound_positional, bound_keywords);

    let params = signature.parameters().as_slice();
    let return_ty = signature.return_ty;
    let bound_positional_count = bound_positional.len();

    let mut remaining = Vec::new();
    let mut positional_consumed = 0usize;

    for param in params {
        if param.is_variadic() || param.is_keyword_variadic() {
            remaining.push(param.clone());
        } else if param.is_positional() {
            if positional_consumed < bound_positional_count {
                // Consumed by a bound positional argument (and thus cannot be overridden).
                positional_consumed += 1;
            } else if !param.is_positional_only()
                && let Some(name) = param.name()
                && let Some(&(_, bound_ty)) =
                    bound_keywords.iter().find(|(k, _)| *k == name.as_str())
            {
                // Bound by keyword, but `partial` allows overriding keyword
                // arguments at call time, so keep the parameter with a default.
                remaining.push(param.clone().with_default_type(bound_ty));
            } else {
                remaining.push(param.clone());
            }
        } else if param.is_keyword_only() {
            if let Some(name) = param.name()
                && let Some(&(_, bound_ty)) =
                    bound_keywords.iter().find(|(k, _)| *k == name.as_str())
            {
                // Bound by keyword, but can be overridden at call time.
                remaining.push(param.clone().with_default_type(bound_ty));
            } else {
                remaining.push(param.clone());
            }
        }
    }

    Signature::new(Parameters::new(db, remaining), return_ty)
}
