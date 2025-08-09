use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{Applicability, Edit, Fix, Violation};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_python_semantic::{SemanticModel, analyze::typing};
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

/// Check if the given segments represent a pathlib Path subclass or `PackagePath` with preview mode support.
/// In stable mode, only checks for `Path` and `PurePath`. In preview mode, also checks for
/// `PosixPath`, `PurePosixPath`, `WindowsPath`, `PureWindowsPath`, and `PackagePath`.
pub(crate) fn is_pure_path_subclass_with_preview(
    checker: &crate::checkers::ast::Checker,
    segments: &[&str],
) -> bool {
    let is_core_pathlib = matches!(segments, ["pathlib", "Path" | "PurePath"]);

    if is_core_pathlib {
        return true;
    }

    if checker.settings().preview.is_enabled() {
        let is_expanded_pathlib = matches!(
            segments,
            [
                "pathlib",
                "PosixPath" | "PurePosixPath" | "WindowsPath" | "PureWindowsPath"
            ]
        );
        let is_packagepath = matches!(segments, ["importlib", "metadata", "PackagePath"]);

        return is_expanded_pathlib || is_packagepath;
    }

    false
}

/// We check functions that take only 1 argument,  this does not apply to functions
/// with `dir_fd` argument, because `dir_fd` is not supported by pathlib,
/// so check if it's set to non-default values
pub(crate) fn check_os_pathlib_single_arg_calls(
    checker: &Checker,
    call: &ExprCall,
    attr: &str,
    fn_argument: &str,
    fix_enabled: bool,
    violation: impl Violation,
) {
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

pub(crate) fn get_name_expr(expr: &Expr) -> Option<&ast::ExprName> {
    match expr {
        Expr::Name(name) => Some(name),
        Expr::Call(ExprCall { func, .. }) => get_name_expr(func),
        _ => None,
    }
}

/// Returns `true` if the given expression looks like a file descriptor, i.e., if it is an integer.
pub(crate) fn is_file_descriptor(expr: &Expr, semantic: &SemanticModel) -> bool {
    if matches!(
        expr,
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(_),
            ..
        })
    ) {
        return true;
    }

    let Some(name) = get_name_expr(expr) else {
        return false;
    };

    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return false;
    };

    typing::is_int(binding, semantic)
}

pub(crate) fn check_os_pathlib_two_arg_calls(
    checker: &Checker,
    call: &ExprCall,
    attr: &str,
    path_arg: &str,
    second_arg: &str,
    fix_enabled: bool,
    violation: impl Violation,
) {
    let range = call.range();
    let mut diagnostic = checker.report_diagnostic(violation, call.func.range());

    let (Some(path_expr), Some(second_expr)) = (
        call.arguments.find_argument_value(path_arg, 0),
        call.arguments.find_argument_value(second_arg, 1),
    ) else {
        return;
    };

    let path_code = checker.locator().slice(path_expr.range());
    let second_code = checker.locator().slice(second_expr.range());

    if fix_enabled {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("pathlib", "Path"),
                call.start(),
                checker.semantic(),
            )?;

            let replacement = if is_pathlib_path_call(checker, path_expr) {
                format!("{path_code}.{attr}({second_code})")
            } else {
                format!("{binding}({path_code}).{attr}({second_code})")
            };

            let applicability = if checker.comment_ranges().intersects(range) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };

            Ok(Fix::applicable_edits(
                Edit::range_replacement(replacement, range),
                [import_edit],
                applicability,
            ))
        });
    }
}
