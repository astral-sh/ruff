use ruff_python_ast as ast;

use super::function::CheckedFunction;
use crate::DisplaySettings;
use crate::types::Type;
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    DISJOINT_TYPE_IS_NARROWING, INVALID_TYPE_GUARD_DEFINITION, SUSPICIOUS_TYPE_IS_NARROWING,
};

/// Check that all type guard function definitions have at least one positional parameter
/// (in addition to `self`/`cls` for methods), and for `TypeIs`, that the narrowed type is
/// assignable to the declared type of that parameter.
pub(crate) fn check_type_guard_definition<'db>(
    context: &InferContext<'db, '_>,
    ty: Type<'db>,
    checked_function: Option<CheckedFunction<'db>>,
    node: &ast::StmtFunctionDef,
) {
    let Type::FunctionLiteral(function) = ty else {
        return;
    };
    let Some(returns_expr) = node.returns.as_deref() else {
        return;
    };

    let db = context.db();
    let env = context.program_environment();

    let overload = function.literal(db).last_definition;
    // A decorator may have replaced the function, and cycle recovery can replace
    // an annotated guard with `Unknown` in the cached signature. In either case,
    // check the definition's signature directly. Wrapping an async return in a
    // coroutine can hide a type guard, but it cannot produce one.
    if let Some(checked) = checked_function
        && checked.overload == overload
        && !checked.return_ty.is_unknown()
        && !matches!(checked.return_ty, Type::TypeIs(_) | Type::TypeGuard(_))
    {
        return;
    }
    let signature = overload.signature(db);
    let return_ty = signature.return_ty;

    // Check if this is a `TypeIs` or `TypeGuard` return type.
    let (type_guard_form_name, narrowed_type) = match return_ty {
        Type::TypeIs(type_is) => ("TypeIs", Some(type_is.return_type(db))),
        Type::TypeGuard(_) => ("TypeGuard", None),
        _ => return,
    };

    // Check if this is a non-static method (first parameter is implicit `self`/`cls`).
    let has_implicit_receiver = overload.has_implicit_receiver(db);

    // Find the first positional parameter to narrow (skip implicit `self`/`cls`).
    let positional_params: Vec<_> = signature.parameters().positional().collect();
    let first_narrowed_param_index = usize::from(has_implicit_receiver);
    let first_narrowed_param = positional_params.get(first_narrowed_param_index);

    let Some(first_narrowed_param) = first_narrowed_param else {
        if let Some(builder) = context.report_lint(&INVALID_TYPE_GUARD_DEFINITION, returns_expr) {
            builder.into_diagnostic(format_args!(
                "`{type_guard_form_name}` function must have a parameter to narrow"
            ));
        }
        return;
    };

    let Some(narrowed_ty) = narrowed_type else {
        return;
    };
    let param_ty = first_narrowed_param.annotated_type();
    if narrowed_ty.is_assignable_to(db, env, param_ty) {
        return;
    }

    let is_disjoint = narrowed_ty.is_disjoint_from(db, env, param_ty);
    let (lint, relation) = if is_disjoint {
        (&DISJOINT_TYPE_IS_NARROWING, "disjoint from")
    } else {
        (&SUSPICIOUS_TYPE_IS_NARROWING, "not assignable to")
    };
    if let Some(builder) = context.report_lint(lint, returns_expr) {
        let settings =
            DisplaySettings::from_possibly_ambiguous_types(db, env, [param_ty, narrowed_ty]);
        let narrowed_display = narrowed_ty.display_with(db, env, settings.clone());
        let param_display = param_ty.display_with(db, env, settings);
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Narrowed type `{narrowed_display}` is {relation} \
             the declared parameter type `{param_display}`"
        ));
        if is_disjoint {
            diagnostic.info(format_args!(
                "`{param_display}` is disjoint from `{narrowed_display}`"
            ));
            narrowed_ty
                .disjointness_error_context(db, env, param_ty)
                .attach_to(db, env, &mut diagnostic);
        } else {
            diagnostic.info(
                "This is not necessarily unsound, but may indicate a mistake in the signature",
            );
        }
    }
}
