use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{Applicability, Edit, Fix, Violation};
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

pub(crate) fn is_path_call(checker: &Checker, expr: &Expr) -> bool {
    expr.as_call_expr().is_some_and(|expr_call| {
        checker
            .semantic()
            .resolve_qualified_name(&expr_call.func)
            .is_some_and(|name| matches!(name.segments(), ["pathlib", "Path"]))
    })
}

pub(crate) fn check_os_path_get_calls(
    checker: &Checker,
    call: &ExprCall,
    fn_name: &str,
    attr: &str,
    fix_enabled: bool,
    violation: impl Violation,
) {
    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_none_or(|qualified_name| qualified_name.segments() != ["os", "path", fn_name])
    {
        return;
    }

    if call.arguments.len() != 1 {
        return;
    }

    let Some(arg) = call.arguments.find_argument_value("filename", 0) else {
        return;
    };

    let arg_code = checker.locator().slice(arg.range());
    let range = call.range();

    let mut diagnostic = checker.report_diagnostic(violation, call.func.range());

    if fix_enabled {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("pathlib", "Path"),
                call.start(),
                checker.semantic(),
            )?;

            let applicability = if checker.comment_ranges().intersects(range) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };

            let replacement = if is_path_call(checker, arg) {
                format!("{arg_code}.stat().{attr}")
            } else {
                format!("{binding}({arg_code}).stat().{attr}")
            };

            Ok(Fix::applicable_edits(
                Edit::range_replacement(replacement, range),
                [import_edit],
                applicability,
            ))
        });
    }
}
