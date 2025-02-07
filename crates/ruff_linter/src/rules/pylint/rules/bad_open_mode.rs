use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::open_mode::OpenMode;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for an invalid `mode` argument in `open` calls.
///
/// ## Why is this bad?
/// The `open` function accepts a `mode` argument that specifies how the file
/// should be opened (e.g., read-only, write-only, append-only, etc.).
///
/// Python supports a variety of open modes: `r`, `w`, `a`, and `x`, to control
/// reading, writing, appending, and creating, respectively, along with
/// `b` (binary mode), `+` (read and write), and `U` (universal newlines),
/// the latter of which is only valid alongside `r`. This rule detects both
/// invalid combinations of modes and invalid characters in the mode string
/// itself.
///
/// ## Example
/// ```python
/// with open("file", "rwx") as f:
///     return f.read()
/// ```
///
/// Use instead:
/// ```python
/// with open("file", "r") as f:
///     return f.read()
/// ```
///
/// ## References
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
#[derive(ViolationMetadata)]
pub(crate) struct BadOpenMode {
    mode: String,
}

impl Violation for BadOpenMode {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadOpenMode { mode } = self;
        format!("`{mode}` is not a valid mode for `open`")
    }
}

/// PLW1501
pub(crate) fn bad_open_mode(checker: &Checker, call: &ast::ExprCall) {
    let Some(kind) = is_open(call.func.as_ref(), checker.semantic()) else {
        return;
    };

    let Some(mode) = extract_mode(call, kind) else {
        return;
    };

    let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = mode else {
        return;
    };

    if OpenMode::from_chars(value.chars()).is_ok() {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(
        BadOpenMode {
            mode: value.to_string(),
        },
        mode.range(),
    ));
}

#[derive(Debug, Copy, Clone)]
enum Kind {
    /// A call to the builtin `open(...)`.
    Builtin,
    /// A call to `pathlib.Path(...).open(...)`.
    Pathlib,
}

/// If a function is a call to `open`, returns the kind of `open` call.
fn is_open(func: &Expr, semantic: &SemanticModel) -> Option<Kind> {
    // Ex) `open(...)`
    if semantic.match_builtin_expr(func, "open") {
        return Some(Kind::Builtin);
    }

    // Ex) `pathlib.Path(...).open(...)`
    let ast::ExprAttribute { attr, value, .. } = func.as_attribute_expr()?;
    if attr != "open" {
        return None;
    }
    let ast::ExprCall {
        func: value_func, ..
    } = value.as_call_expr()?;
    let qualified_name = semantic.resolve_qualified_name(value_func)?;
    match qualified_name.segments() {
        ["pathlib", "Path"] => Some(Kind::Pathlib),
        _ => None,
    }
}

/// Returns the mode argument, if present.
fn extract_mode(call: &ast::ExprCall, kind: Kind) -> Option<&Expr> {
    match kind {
        Kind::Builtin => call.arguments.find_argument_value("mode", 1),
        Kind::Pathlib => call.arguments.find_argument_value("mode", 0),
    }
}
