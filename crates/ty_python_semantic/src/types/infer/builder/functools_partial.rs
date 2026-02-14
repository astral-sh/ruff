use ruff_python_ast as ast;

use super::TypeInferenceBuilder;
use crate::Db;
use crate::types::Type;
use crate::types::diagnostic::{INVALID_ARGUMENT_TYPE, TOO_MANY_POSITIONAL_ARGUMENTS};
use crate::types::signatures::{Parameter, Signature};

/// `functools.partial` error checking.
impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Check that bound arguments to a `functools.partial(func, ...)` call are
    /// compatible with the wrapped function's parameter types, and emit
    /// diagnostics for mismatches.
    pub(super) fn check_functools_partial_call(&self, arguments: &ast::Arguments) {
        let db = self.db();

        // We need at least one positional argument (the wrapped function).
        let Some(func_expr) = arguments.args.first() else {
            return;
        };

        // If the first positional arg is starred, we can't determine the wrapped function.
        if func_expr.is_starred_expr() {
            return;
        }

        let func_ty = self.expression_type(func_expr);

        let Some(callable) = func_ty.try_upcast_to_callable(db) else {
            return;
        };
        let Some(callable) = callable.exactly_one() else {
            return;
        };
        let overloads = &callable.signatures(db).overloads;

        // Collect bound positional argument types, skipping the first argument.
        let mut bound_positional: Vec<Type<'db>> = Vec::new();
        for arg in &arguments.args[1..] {
            if let Some(starred) = arg.as_starred_expr() {
                let iterable_ty = self.expression_type(&starred.value);
                if let Type::NominalInstance(nominal) = iterable_ty
                    && let Some(tuple_spec) = nominal.tuple_spec(db)
                    && let Some(fixed) = tuple_spec.as_fixed_length()
                {
                    bound_positional.extend(fixed.all_elements().iter().copied());
                } else {
                    return;
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
                if let Some(typed_dict) = splat_ty.as_typed_dict() {
                    for (name, field) in typed_dict.items(db) {
                        bound_keywords.push((name.as_str(), field.declared_ty));
                    }
                } else {
                    return;
                }
            }
        }

        // Specialize each overload (inferring type variables from bound args).
        let specialized: Vec<_> = overloads
            .iter()
            .map(|sig| sig.specialize_from_bound_args(db, &bound_positional, &bound_keywords))
            .collect();

        self.check_partial_bound_args(arguments, &specialized, &bound_positional, &bound_keywords);
    }

    /// Check that bound arguments are compatible with the wrapped function's
    /// parameter types, and emit diagnostics for mismatches.
    ///
    /// For overloaded functions, diagnostics are only emitted when no overload
    /// accepts the bound arguments.
    fn check_partial_bound_args(
        &self,
        arguments: &ast::Arguments,
        specialized: &[Signature<'db>],
        bound_positional: &[Type<'db>],
        bound_keywords: &[(&str, Type<'db>)],
    ) {
        let db = self.db();

        // Check if any overload accepts all bound arguments. If at least one
        // overload matches, we don't emit diagnostics.
        let any_overload_matches = specialized.iter().any(|signature| {
            let params = signature.parameters().as_slice();
            bound_args_match_params(db, params, bound_positional, bound_keywords)
        });

        if any_overload_matches {
            return;
        }

        // No overload matched; emit diagnostics against the first overload.
        let Some(first) = specialized.first() else {
            return;
        };

        let params = first.parameters().as_slice();
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

        // Check for excess positional arguments (only if the function has no
        // variadic `*args` parameter to absorb them).
        let has_variadic = params.iter().any(Parameter::is_variadic);
        let expected_positional_count = params.iter().filter(|p| p.is_positional()).count();
        if !has_variadic && bound_positional.len() > expected_positional_count {
            let first_excess_index = expected_positional_count;
            if let Some(first_excess_expr) = positional_arg_exprs.get(first_excess_index)
                && let Some(builder) = self
                    .context
                    .report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, first_excess_expr)
            {
                builder.into_diagnostic(format_args!(
                    "Too many positional arguments to `partial()`: expected \
                    {expected_positional_count}, got {}",
                    bound_positional.len(),
                ));
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
    let has_variadic = params.iter().any(Parameter::is_variadic);
    let expected_positional_count = params.iter().filter(|p| p.is_positional()).count();

    if !has_variadic && bound_positional.len() > expected_positional_count {
        return false;
    }

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
