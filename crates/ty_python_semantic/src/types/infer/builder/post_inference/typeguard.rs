use ruff_python_ast as ast;

use crate::{
    semantic_index::SemanticIndex,
    types::{Type, context::InferContext, diagnostic::INVALID_TYPE_GUARD_DEFINITION},
};

/// Check that all type guard function definitions have at least one positional parameter
/// (in addition to `self`/`cls` for methods), and for `TypeIs`, that the narrowed type is
/// assignable to the declared type of that parameter.
pub(crate) fn check_type_guard_definition<'db>(
    context: &InferContext<'db, '_>,
    ty: Type<'db>,
    node: &ast::StmtFunctionDef,
    index: &SemanticIndex<'db>,
) {
    let Type::FunctionLiteral(function) = ty else {
        return;
    };

    let db = context.db();

    for overload in function.iter_overloads_and_implementation(db) {
        let signature = overload.signature(db);
        let return_ty = signature.return_ty;

        // Check if this is a `TypeIs` or `TypeGuard` return type.
        let (type_guard_form_name, narrowed_type) = match return_ty {
            Type::TypeIs(type_is) => ("TypeIs", Some(type_is.return_type(db))),
            Type::TypeGuard(_) => ("TypeGuard", None),
            _ => continue,
        };

        // The return type annotation must exist since we matched `TypeIs`/`TypeGuard`.
        let Some(returns_expr) = node.returns.as_deref() else {
            continue;
        };

        // Check if this is a non-static method (first parameter is implicit `self`/`cls`).
        let is_method = index
            .class_definition_of_method(overload.body_scope(db).file_scope_id(db))
            .is_some();
        let has_implicit_receiver = is_method && !overload.is_staticmethod(db);

        // Find the first positional parameter to narrow (skip implicit `self`/`cls`).
        let positional_params: Vec<_> = signature.parameters().positional().collect();
        let first_narrowed_param_index = usize::from(has_implicit_receiver);
        let first_narrowed_param = positional_params.get(first_narrowed_param_index);

        let Some(first_narrowed_param) = first_narrowed_param else {
            if let Some(builder) = context.report_lint(&INVALID_TYPE_GUARD_DEFINITION, returns_expr)
            {
                builder.into_diagnostic(format_args!(
                    "`{type_guard_form_name}` function must have a parameter to narrow"
                ));
            }
            continue;
        };

        // For `TypeIs`, check that the narrowed type is assignable to the parameter type.
        if let Some(narrowed_ty) = narrowed_type {
            let param_ty = first_narrowed_param.annotated_type();
            if !narrowed_ty.is_assignable_to(db, param_ty)
                && let Some(builder) =
                    context.report_lint(&INVALID_TYPE_GUARD_DEFINITION, returns_expr)
            {
                builder.into_diagnostic(format_args!(
                    "Narrowed type `{narrowed}` is not assignable \
                        to the declared parameter type `{param}`",
                    narrowed = narrowed_ty.display(db),
                    param = param_ty.display(db)
                ));
            }
        }
    }
}
