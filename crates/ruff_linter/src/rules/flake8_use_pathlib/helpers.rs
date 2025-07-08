use crate::checkers::ast::Checker;
use ruff_python_ast::{self as ast};
use crate::importer::ImportRequest;
use crate::{Applicability, Edit, Fix, Violation};
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

pub(crate) fn is_keyword_only_argument_non_default(arguments: &ast::Arguments, name: &str) -> bool {
    arguments
        .find_keyword(name)
        .is_some_and(|keyword| !keyword.value.is_none_literal_expr())
}

pub(crate) fn is_pathlib_path_call(checker: &Checker, expr: &Expr) -> bool {
    expr.as_call_expr().is_some_and(|expr_call| {
        checker
            .semantic()
            .resolve_qualified_name(&expr_call.func)
            .is_some_and(|name| matches!(name.segments(), ["pathlib", "Path"]))
    })
}

/// We check functions that take only 1 argument,  this does not apply to functions
/// with `dir_fd` argument, because `dir_fd` is not supported by pathlib,
/// so check if it's set to non-default values
pub(crate) fn check_os_pathlib_single_arg_calls(
    checker: &Checker,
    call: &ExprCall,
    full_import: &[&str],
    attr: &str,
    fn_argument: &str,
    fix_enabled: bool,
    violation: impl Violation,
) {
    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_none_or(|qualified_name| qualified_name.segments() != full_import)
    {
        return;
    }

    if call.arguments.len() != 1 {
        return;
    }

    let Some(arg) = call.arguments.find_argument_value(fn_argument, 0) else {
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

            let replacement = if is_pathlib_path_call(checker, arg) {
                format!("{arg_code}.{attr}")
            } else {
                format!("{binding}({arg_code}).{attr}")
            };

            Ok(Fix::applicable_edits(
                Edit::range_replacement(replacement, range),
                [import_edit],
                applicability,
            ))
        });
    }
}
