use crate::types::{ParamSpecAttrKind, Type, context::InferContext, diagnostic::INVALID_PARAMSPEC};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

/// Validate the usage of `ParamSpec` components (`P.args` and `P.kwargs`) across all
/// parameters of a function.
///
/// This enforces several rules from the typing spec:
/// - `P.args` and `P.kwargs` must always be used together
/// - When `*args: P.args` is present, `**kwargs: P.kwargs` must also be present (same P)
/// - No keyword-only parameters are allowed between `*args: P.args` and `**kwargs: P.kwargs`
pub(super) fn validate_paramspec_components<'db>(
    context: &'db InferContext<'db, '_>,
    parameters: &ast::Parameters,
    infer_type: impl Fn(&ast::Expr) -> Type<'db>,
) {
    let db = context.db();

    // Extract ParamSpec info from *args annotation
    let args_paramspec = parameters.vararg.as_deref().and_then(|vararg| {
        let annotation = vararg.annotation()?;
        let ty = infer_type(annotation);
        if let Type::TypeVar(typevar) = ty
            && typevar.is_paramspec(db)
            && typevar.paramspec_attr(db) == Some(ParamSpecAttrKind::Args)
        {
            Some((typevar.without_paramspec_attr(db), annotation))
        } else {
            None
        }
    });

    // Extract ParamSpec info from **kwargs annotation
    let kwargs_paramspec = parameters.kwarg.as_deref().and_then(|kwarg| {
        let annotation = kwarg.annotation()?;
        let ty = infer_type(annotation);
        if let Type::TypeVar(typevar) = ty
            && typevar.is_paramspec(db)
            && typevar.paramspec_attr(db) == Some(ParamSpecAttrKind::Kwargs)
        {
            Some((typevar.without_paramspec_attr(db), annotation))
        } else {
            None
        }
    });

    let vararg_name = parameters.vararg.as_deref().map(|v| v.name.as_str());
    let kwarg_name = parameters.kwarg.as_deref().map(|k| k.name.as_str());

    match (args_paramspec, kwargs_paramspec) {
        // Both *args: P.args and **kwargs: P.kwargs present
        (Some((args_tv, _args_annotation)), Some((kwargs_tv, kwargs_annotation))) => {
            // Check they refer to the same ParamSpec
            if !args_tv.is_same_typevar_as(db, kwargs_tv) {
                let args_name = args_tv.name(db);
                let vararg = vararg_name.unwrap_or("args");
                let kwarg = kwarg_name.unwrap_or("kwargs");
                if let Some(builder) = context.report_lint(&INVALID_PARAMSPEC, kwargs_annotation) {
                    builder.into_diagnostic(format_args!(
                        "`*{vararg}: {args_name}.args` must be accompanied \
                             by `**{kwarg}: {args_name}.kwargs`",
                    ));
                }
            } else {
                // Same ParamSpec - check no keyword-only params between them
                if !parameters.kwonlyargs.is_empty() {
                    let name = args_tv.name(db);
                    let vararg = vararg_name.unwrap_or("args");
                    let kwarg = kwarg_name.unwrap_or("kwargs");
                    if let Some(builder) =
                        context.report_lint(&INVALID_PARAMSPEC, &parameters.kwonlyargs[0])
                    {
                        builder.into_diagnostic(format_args!(
                            "No parameters may appear between \
                                 `*{vararg}: {name}.args` and `**{kwarg}: {name}.kwargs`",
                        ));
                    }
                }
            }
        }

        // *args: P.args without matching **kwargs: P.kwargs
        (Some((args_tv, args_annotation)), None) => {
            let name = args_tv.name(db);
            let vararg = vararg_name.unwrap_or("args");
            let kwarg = kwarg_name.unwrap_or("kwargs");
            // Report on the kwarg annotation if it exists, otherwise on *args
            let range = if let Some(kwarg_param) = parameters.kwarg.as_deref() {
                kwarg_param
                    .annotation()
                    .map(Ranged::range)
                    .unwrap_or_else(|| kwarg_param.range())
            } else {
                args_annotation.range()
            };
            if let Some(builder) = context.report_lint(&INVALID_PARAMSPEC, range) {
                builder.into_diagnostic(format_args!(
                    "`*{vararg}: {name}.args` must be accompanied by `**{kwarg}: {name}.kwargs`",
                ));
            }
        }

        // **kwargs: P.kwargs without matching *args: P.args
        (None, Some((kwargs_tv, kwargs_annotation))) => {
            let name = kwargs_tv.name(db);
            let vararg = vararg_name.unwrap_or("args");
            let kwarg = kwarg_name.unwrap_or("kwargs");
            // Report on the vararg annotation if it exists, otherwise on **kwargs
            let range = if let Some(vararg_param) = parameters.vararg.as_deref() {
                vararg_param
                    .annotation()
                    .map(Ranged::range)
                    .unwrap_or_else(|| vararg_param.range())
            } else {
                kwargs_annotation.range()
            };
            if let Some(builder) = context.report_lint(&INVALID_PARAMSPEC, range) {
                builder.into_diagnostic(format_args!(
                    "`**{kwarg}: {name}.kwargs` must be accompanied by `*{vararg}: {name}.args`",
                ));
            } else {
                // No *args at all
                if let Some(builder) = context.report_lint(&INVALID_PARAMSPEC, kwargs_annotation) {
                    builder.into_diagnostic(format_args!(
                        "`**{kwarg}: {name}.kwargs` must be accompanied by \
                             `*{kwarg}: {name}.args`",
                    ));
                }
            }
        }

        // No ParamSpec components in either position
        (None, None) => {}
    }
}
