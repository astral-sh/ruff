use anyhow::Result;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::call_path::{format_call_path, CallPath};
use ruff_python_ast::Expr;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::add_argument;
use crate::importer::ImportRequest;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for uses of `open` and related calls without an explicit `encoding`
/// argument.
///
/// ## Why is this bad?
/// Using `open` in text mode without an explicit encoding can lead to
/// non-portable code, with differing behavior across platforms.
///
/// Instead, consider using the `encoding` parameter to enforce a specific
/// encoding. [PEP 597] recommends using `locale.getpreferredencoding(False)`
/// as the default encoding on versions earlier than Python 3.10, and
/// `encoding="locale"` on Python 3.10 and later.
///
/// ## Example
/// ```python
/// open("file.txt")
/// ```
///
/// Use instead:
/// ```python
/// open("file.txt", encoding="utf-8")
/// ```
///
/// ## References
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
///
/// [PEP 597]: https://peps.python.org/pep-0597/
#[violation]
pub struct UnspecifiedEncoding {
    function_name: String,
    mode: Mode,
}

impl AlwaysFixableViolation for UnspecifiedEncoding {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnspecifiedEncoding {
            function_name,
            mode,
        } = self;

        match mode {
            Mode::Supported => {
                format!("`{function_name}` in text mode without explicit `encoding` argument")
            }
            Mode::Unsupported => {
                format!("`{function_name}` without explicit `encoding` argument")
            }
        }
    }

    fn fix_title(&self) -> String {
        format!("Add explicit `encoding` argument")
    }
}

/// PLW1514
pub(crate) fn unspecified_encoding(checker: &mut Checker, call: &ast::ExprCall) {
    let Some((function_name, mode)) = checker
        .semantic()
        .resolve_call_path(&call.func)
        .filter(|call_path| is_violation(call, call_path))
        .map(|call_path| {
            (
                format_call_path(call_path.as_slice()),
                Mode::from(&call_path),
            )
        })
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        UnspecifiedEncoding {
            function_name,
            mode,
        },
        call.func.range(),
    );

    if checker.settings.target_version >= PythonVersion::Py310 {
        diagnostic.set_fix(generate_keyword_fix(checker, call));
    } else {
        diagnostic.try_set_fix(|| generate_import_fix(checker, call));
    }

    checker.diagnostics.push(diagnostic);
}

/// Generate an [`Edit`] for Python 3.10 and later.
fn generate_keyword_fix(checker: &Checker, call: &ast::ExprCall) -> Fix {
    Fix::unsafe_edit(add_argument(
        &format!(
            "encoding={}",
            checker
                .generator()
                .expr(&Expr::StringLiteral(ast::ExprStringLiteral {
                    value: ast::StringLiteralValue::single(ast::StringLiteral {
                        value: "locale".to_string(),
                        unicode: false,
                        range: TextRange::default(),
                    }),
                    range: TextRange::default(),
                }))
        ),
        &call.arguments,
        checker.indexer().comment_ranges(),
        checker.locator().contents(),
    ))
}

/// Generate an [`Edit`] for Python 3.9 and earlier.
fn generate_import_fix(checker: &Checker, call: &ast::ExprCall) -> Result<Fix> {
    let (import_edit, binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import("locale", "getpreferredencoding"),
        call.start(),
        checker.semantic(),
    )?;
    let argument_edit = add_argument(
        &format!("encoding={binding}(False)"),
        &call.arguments,
        checker.indexer().comment_ranges(),
        checker.locator().contents(),
    );
    Ok(Fix::unsafe_edits(import_edit, [argument_edit]))
}

/// Returns `true` if the given expression is a string literal containing a `b` character.
fn is_binary_mode(expr: &Expr) -> Option<bool> {
    Some(
        expr.as_string_literal_expr()?
            .value
            .chars()
            .any(|c| c == 'b'),
    )
}

/// Returns `true` if the given call lacks an explicit `encoding`.
fn is_violation(call: &ast::ExprCall, call_path: &CallPath) -> bool {
    // If we have something like `*args`, which might contain the encoding argument, abort.
    if call.arguments.args.iter().any(Expr::is_starred_expr) {
        return false;
    }
    // If we have something like `**kwargs`, which might contain the encoding argument, abort.
    if call
        .arguments
        .keywords
        .iter()
        .any(|keyword| keyword.arg.is_none())
    {
        return false;
    }
    match call_path.as_slice() {
        ["" | "codecs" | "_io", "open"] => {
            if let Some(mode_arg) = call.arguments.find_argument("mode", 1) {
                if is_binary_mode(mode_arg).unwrap_or(true) {
                    // binary mode or unknown mode is no violation
                    return false;
                }
            }
            // else mode not specified, defaults to text mode
            call.arguments.find_argument("encoding", 3).is_none()
        }
        ["tempfile", "TemporaryFile" | "NamedTemporaryFile" | "SpooledTemporaryFile"] => {
            let mode_pos = usize::from(call_path[1] == "SpooledTemporaryFile");
            if let Some(mode_arg) = call.arguments.find_argument("mode", mode_pos) {
                if is_binary_mode(mode_arg).unwrap_or(true) {
                    // binary mode or unknown mode is no violation
                    return false;
                }
            } else {
                // defaults to binary mode
                return false;
            }
            call.arguments
                .find_argument("encoding", mode_pos + 2)
                .is_none()
        }
        ["io" | "_io", "TextIOWrapper"] => call.arguments.find_argument("encoding", 1).is_none(),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The call supports a `mode` argument.
    Supported,
    /// The call does not support a `mode` argument.
    Unsupported,
}

impl From<&CallPath<'_>> for Mode {
    fn from(value: &CallPath<'_>) -> Self {
        match value.as_slice() {
            ["" | "codecs" | "_io", "open"] => Mode::Supported,
            ["tempfile", "TemporaryFile" | "NamedTemporaryFile" | "SpooledTemporaryFile"] => {
                Mode::Supported
            }
            ["io" | "_io", "TextIOWrapper"] => Mode::Unsupported,
            _ => Mode::Unsupported,
        }
    }
}
