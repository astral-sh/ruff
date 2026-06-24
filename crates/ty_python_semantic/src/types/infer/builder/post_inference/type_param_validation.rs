use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::diagnostic::format_enumeration;
use crate::types::{context::InferContext, diagnostic::INVALID_TYPE_VARIABLE_DEFAULT};

/// Check that no type parameter with a default follows a `TypeVarTuple` in a PEP 695
/// type parameter list. This is prohibited by the typing spec because a `TypeVarTuple`
/// consumes all remaining positional type arguments.
///
/// This check is used for both classes and type aliases with PEP 695 type parameters.
pub(crate) fn check_no_default_after_typevar_tuple_pep695(
    context: &InferContext<'_, '_>,
    type_params: &ast::TypeParams,
) {
    let mut typevar_tuple: Option<&ast::TypeParamTypeVarTuple> = None;
    let mut params_with_defaults = vec![];

    for type_param in type_params {
        if typevar_tuple.is_some() {
            if type_param.default().is_some() {
                params_with_defaults.push(type_param);
            }
        } else if let ast::TypeParam::TypeVarTuple(tvt) = type_param {
            typevar_tuple = Some(tvt);
        }
    }

    let Some(typevar_tuple) = typevar_tuple else {
        return;
    };

    if params_with_defaults.is_empty() {
        return;
    }

    let Some(builder) =
        context.report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, params_with_defaults[0])
    else {
        return;
    };

    let mut diagnostic = builder
        .into_diagnostic("Type parameters with defaults cannot follow a TypeVarTuple parameter");

    if let [single_param] = params_with_defaults.as_slice() {
        let single_name = single_param.name();

        diagnostic.set_concise_message(format_args!(
            "Type parameter `{single_name}` with a default follows TypeVarTuple `{}`",
            &typevar_tuple.name
        ));

        diagnostic.set_primary_message(format_args!("`{single_name}` has a default"));
    } else {
        let names = format_enumeration(params_with_defaults.iter().map(|p| p.name()));

        diagnostic.set_concise_message(format_args!(
            "Type parameters {names} with defaults follow TypeVarTuple `{}`",
            &typevar_tuple.name
        ));

        diagnostic.set_primary_message(format_args!(
            "`{}` has a default",
            params_with_defaults[0].name()
        ));

        for param in &params_with_defaults[1..] {
            diagnostic.annotate(
                context
                    .secondary(param.range())
                    .message(format_args!("`{}` also has a default", param.name())),
            );
        }
    }

    diagnostic.annotate(
        context
            .secondary(typevar_tuple)
            .message(format_args!("`{}` is a TypeVarTuple", &typevar_tuple.name)),
    );

    diagnostic.info("See https://typing.python.org/en/latest/spec/generics.html#defaults-following-typevartuple");
}
