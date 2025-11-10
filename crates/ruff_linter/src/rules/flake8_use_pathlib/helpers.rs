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
    applicability: Option<Applicability>,
) {
    // Check that we have exactly 1 positional argument OR the argument is passed as a keyword
    // This allows: func(arg), func(name=arg), and func(arg, dir_fd=None)
    let has_positional_arg = call.arguments.args.len() == 1;
    let has_keyword_arg = call.arguments.find_keyword(fn_argument).is_some();
    
    if !has_positional_arg && !has_keyword_arg {
        return;
    }
    
    // If we have both positional and keyword for the main argument, that's invalid
    if has_positional_arg && has_keyword_arg {
        return;
    }

    // If `dir_fd` is set to a non-default value, skip (pathlib doesn't support it)
    if is_keyword_only_argument_non_default(&call.arguments, "dir_fd") {
        return;
    }

    // If there are keyword arguments other than `dir_fd` or the main argument, skip
    // We need to allow the main argument to be passed as a keyword, and `dir_fd=None`
    let allowed_keywords = if has_keyword_arg {
        &[fn_argument, "dir_fd"][..]
    } else {
        &["dir_fd"][..]
    };
    if has_unknown_keywords_or_starred_expr(&call.arguments, allowed_keywords) {
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

        let fix = match applicability {
            Some(Applicability::Unsafe) => Fix::unsafe_edits(edit, [import_edit]),
            _ => {
                let applicability = if checker.comment_ranges().intersects(range) {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                };
                Fix::applicable_edits(edit, [import_edit], applicability)
            }
        };

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
