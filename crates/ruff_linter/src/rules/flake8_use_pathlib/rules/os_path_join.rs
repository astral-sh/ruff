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
    pub(crate) module: String,
    pub(crate) joiner: Joiner,
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
pub(crate) enum Joiner {
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

    let Some((first_arg, args)) = get_args(&call.arguments, module) else {
        return;
    };

    if !is_valid_args(first_arg, args.as_slice(), module) {
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
            args.iter().map(|expr| locator.slice(expr.range())),
            separator,
        );

        let arg_code = locator.slice(first_arg.range());

        let base = if is_pathlib_path_call(checker, first_arg) {
            arg_code.to_string()
        } else {
            format!("{binding}({arg_code})")
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

/// Returns the first path component and the remaining components.
fn get_args<'a>(arguments: &'a Arguments, module: &str) -> Option<(&'a Expr, Vec<&'a Expr>)> {
    match module {
        "path" => {
            let mut args = arguments.args.iter();

            let first = args.next()?;
            let rest = args.collect();

            Some((first, rest))
        }

        "sep" => {
            let [iterable] = arguments.args.as_ref() else {
                return None;
            };

            match iterable {
                Expr::Tuple(tuple) => {
                    let mut elements = tuple.elts.iter();

                    let first = elements.next()?;
                    let rest = elements.collect();

                    Some((first, rest))
                }

                Expr::List(list) => {
                    let mut elements = list.elts.iter();

                    let first = elements.next()?;
                    let rest = elements.collect();

                    Some((first, rest))
                }

                _ => None,
            }
        }

        _ => None,
    }
}

fn is_valid_args(first_arg: &Expr, args: &[&Expr], module: &str) -> bool {
    match module {
        "path" => {
            // Path components such as lists, tuples and dictionaries cannot
            // be passed directly to Path /.
            !matches!(first_arg, Expr::Tuple(_) | Expr::List(_) | Expr::Dict(_))
                && args
                    .iter()
                    .all(|expr| !matches!(expr, Expr::Tuple(_) | Expr::List(_) | Expr::Dict(_)))
        }

        "sep" => {
            // get_args only returns here for a literal tuple/list.
            // Each element becomes a separate Path component.
            true
        }

        _ => false,
    }
}
