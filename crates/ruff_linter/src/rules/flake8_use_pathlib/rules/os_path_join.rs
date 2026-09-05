use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Arguments, Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::codes::Category;
use crate::{
    FixAvailability, Violation, checkers::ast::Checker, importer::ImportRequest,
    preview::is_fix_os_path_join_enabled, rules::flake8_use_pathlib::helpers::is_pathlib_path_call,
};

/// ## What it does
/// Checks for uses of `os.path.join` and `os.sep.join`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path`
/// objects and the `/` operator can improve readability over `os.path`.
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.join(ROOT_PATH, "folder", "file.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(ROOT_PATH) / "folder" / "file.py"
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231", category = Category::Pedantic)]
pub(crate) struct OsPathJoin {
    module: String,
    joiner: Joiner,
}

impl Violation for OsPathJoin {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let OsPathJoin { module, joiner } = self;

        match joiner {
            Joiner::Slash => {
                format!("`os.{module}.join()` should be replaced by `Path` with `/` operator")
            }
            Joiner::Joinpath => {
                format!("`os.{module}.join()` should be replaced by `Path.joinpath()`")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        match self.joiner {
            Joiner::Joinpath => Some("Replace with `Path(...).joinpath(...)`".to_string()),
            Joiner::Slash => Some("Replace with `Path(...) / ...`".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Joiner {
    Slash,
    Joinpath,
}

// PTH118
pub(crate) fn os_path_join(checker: &Checker, call: &ExprCall, segment: &[&str]) {
    let module = match segment {
        ["os", "path", "join"] => "path",
        ["os", "sep", "join"] => "sep",
        _ => return,
    };

    let joiner = if call.arguments.args.iter().any(Expr::is_starred_expr) {
        Joiner::Joinpath
    } else {
        Joiner::Slash
    };

    let mut diagnostic = checker.report_diagnostic(
        OsPathJoin {
            module: module.to_string(),
            joiner,
        },
        call.func.range(),
    );

    if !is_fix_os_path_join_enabled(checker.settings()) {
        return;
    }

    // Keyword arguments cannot be represented by the generated pathlib call.
    if !call.arguments.keywords.is_empty() {
        return;
    }

    let Some((first_arg, args)) = get_args(checker, &call.arguments, module) else {
        return;
    };

    if !is_valid_args(&first_arg, args.as_slice(), module) {
        return;
    }

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            call.start(),
            checker.semantic(),
        )?;

        let locator = checker.locator();

        let separator = match joiner {
            Joiner::Joinpath => ", ",
            Joiner::Slash => " / ",
        };

        let path_args = itertools::join(
            args.iter().map(|arg| match arg {
                JoinArg::Expr(expr) => locator.slice(expr.range()).to_string(),
                JoinArg::Char(c) => format!("\"{}\"", c.escape_default()),
            }),
            separator,
        );

        let base = match first_arg {
            JoinArg::Expr(expr) => {
                let arg_code = locator.slice(expr.range());

                if is_pathlib_path_call(checker, expr) {
                    arg_code.to_string()
                } else {
                    format!("{binding}({arg_code})")
                }
            }
            JoinArg::Char(c) => {
                format!("{binding}(\"{}\")", c.escape_default())
            }
        };

        let replacement = match joiner {
            Joiner::Joinpath => {
                format!("{base}.joinpath({path_args})")
            }
            Joiner::Slash => {
                if path_args.is_empty() {
                    base
                } else {
                    format!("{base} / {path_args}")
                }
            }
        };

        Ok(Fix::unsafe_edits(
            Edit::range_replacement(replacement, call.range()),
            [import_edit],
        ))
    });
}

enum JoinArg<'a> {
    Expr(&'a Expr),
    Char(char),
}

/// Returns the first path component and the remaining components.
fn get_args<'a>(
    checker: &Checker,
    arguments: &'a Arguments,
    module: &str,
) -> Option<(JoinArg<'a>, Vec<JoinArg<'a>>)> {
    match module {
        "path" => {
            let mut args = arguments.args.iter();

            let first_raw = args.next()?;

            let mut flattened_first = flatten(checker, first_raw, true);
            let first = flattened_first.remove(0);

            let rest = flattened_first
                .into_iter()
                .chain(args.flat_map(|arg| flatten(checker, arg, false)));

            Some((
                JoinArg::Expr(first),
                rest.into_iter().map(JoinArg::Expr).collect(),
            ))
        }

        "sep" => {
            let [iterable] = arguments.args.as_ref() else {
                return None;
            };

            match iterable {
                Expr::Tuple(tuple) => {
                    let mut elements = tuple.elts.iter();

                    let first = elements.next()?;
                    let rest = elements;

                    Some((
                        JoinArg::Expr(first),
                        rest.into_iter().map(JoinArg::Expr).collect(),
                    ))
                }

                Expr::List(list) => {
                    let mut elements = list.elts.iter();

                    let first = elements.next()?;
                    let rest = elements;

                    Some((
                        JoinArg::Expr(first),
                        rest.into_iter().map(JoinArg::Expr).collect(),
                    ))
                }

                Expr::StringLiteral(lit) => {
                    let mut chars = lit.value.chars();

                    let first = chars.next()?;
                    let rest = chars.map(JoinArg::Char).collect();

                    Some((JoinArg::Char(first), rest))
                }

                _ => None,
            }
        }

        _ => None,
    }
}

fn flatten<'a>(checker: &Checker, expr: &'a Expr, keep_single: bool) -> Vec<&'a Expr> {
    if let Expr::Call(call) = expr {
        if !call.arguments.args.is_empty()
            && call.arguments.keywords.is_empty()
            && is_pathlib_path_call(checker, expr)
        {
            if keep_single {
                let has_nested_path = call.arguments.args.iter().any(|arg| {
                    matches!(arg, Expr::Call(inner)
                        if !inner.arguments.args.is_empty()
                            && inner.arguments.keywords.is_empty()
                            && is_pathlib_path_call(checker, arg))
                });

                if !has_nested_path {
                    return vec![expr];
                }
            }

            return call
                .arguments
                .args
                .iter()
                .flat_map(|arg| flatten(checker, arg, false))
                .collect();
        }
    }

    vec![expr]
}

fn is_valid_args(first_arg: &JoinArg, args: &[JoinArg], module: &str) -> bool {
    match module {
        "path" => {
            let JoinArg::Expr(first_arg) = first_arg else {
                return false;
            };

            !matches!(first_arg, Expr::Tuple(_) | Expr::List(_) | Expr::Dict(_))
                && args.iter().all(|arg| {
                    let JoinArg::Expr(expr) = arg else {
                        return false;
                    };

                    !matches!(expr, Expr::Tuple(_) | Expr::List(_) | Expr::Dict(_))
                })
        }

        "sep" => {
            !matches!(
                first_arg,
                JoinArg::Expr(Expr::StringLiteral(lit)) if lit.value.is_empty()
            ) && args.iter().all(|arg| {
                !matches!(
                    arg,
                    JoinArg::Expr(Expr::StringLiteral(lit)) if lit.value.is_empty()
                )
            })
        }

        _ => false,
    }
}
