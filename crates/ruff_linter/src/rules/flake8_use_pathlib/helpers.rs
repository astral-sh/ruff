use ruff_python_ast::{self as ast, Arguments, Expr, ExprCall};
use ruff_python_semantic::{SemanticModel, analyze::typing};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{Applicability, Edit, Fix, Violation};

pub(crate) fn is_keyword_only_argument_non_default(arguments: &Arguments, name: &str) -> bool {
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
pub(crate) fn is_pure_path_subclass_with_preview(checker: &Checker, segments: &[&str]) -> bool {
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
    applicability: Applicability,
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

    if !fix_enabled {
        return;
    }

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;

        let replacement = if is_pathlib_path_call(checker, arg) {
            format!("{arg_code}.{attr}")
        } else {
            format!("{binding}({arg_code}).{attr}")
        };

        let edit = Edit::range_replacement(replacement, range);

        let applicability = match applicability {
            Applicability::DisplayOnly => Applicability::DisplayOnly,
            _ if checker.comment_ranges().intersects(range) => Applicability::Unsafe,
            _ => applicability,
        };

        let fix = Fix::applicable_edits(edit, [import_edit], applicability);

        Ok(fix)
    });
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

#[expect(clippy::too_many_arguments)]
pub(crate) fn check_os_pathlib_two_arg_calls(
    checker: &Checker,
    call: &ExprCall,
    attr: &str,
    path_arg: &str,
    second_arg: &str,
    fix_enabled: bool,
    violation: impl Violation,
    applicability: Applicability,
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

            let applicability = match applicability {
                Applicability::DisplayOnly => Applicability::DisplayOnly,
                _ if checker.comment_ranges().intersects(range) => Applicability::Unsafe,
                _ => applicability,
            };

            Ok(Fix::applicable_edits(
                Edit::range_replacement(replacement, range),
                [import_edit],
                applicability,
            ))
        });
    }
}

pub(crate) fn has_unknown_keywords_or_starred_expr(
    arguments: &Arguments,
    allowed: &[&str],
) -> bool {
    if arguments.args.iter().any(Expr::is_starred_expr) {
        return true;
    }

    arguments.keywords.iter().any(|kw| match &kw.arg {
        Some(arg) => !allowed.contains(&arg.as_str()),
        None => true,
    })
}

/// Returns `true` if argument `name` is set to a non-default `None` value.
pub(crate) fn is_argument_non_default(arguments: &Arguments, name: &str, position: usize) -> bool {
    arguments
        .find_argument_value(name, position)
        .is_some_and(|expr| !expr.is_none_literal_expr())
}

/// Returns `true` if the given call is a top-level expression in its statement.
/// This means the call's return value is not used, so return type changes don't matter.
pub(crate) fn is_top_level_expression_call(checker: &Checker) -> bool {
    checker.semantic().current_expression_parent().is_none()
}
