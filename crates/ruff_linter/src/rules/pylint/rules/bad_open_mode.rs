use bitflags::bitflags;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::SemanticModel;
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
#[violation]
pub struct BadOpenMode {
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
pub(crate) fn bad_open_mode(checker: &mut Checker, call: &ast::ExprCall) {
    let Some(kind) = is_open(call.func.as_ref(), checker.semantic()) else {
        return;
    };

    let Some(mode) = extract_mode(call, kind) else {
        return;
    };

    let Some(ast::ExprStringLiteral { value, .. }) = mode.as_string_literal_expr() else {
        return;
    };

    if is_valid_mode(value) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
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
    match func {
        // Ex) `pathlib.Path(...).open(...)`
        Expr::Attribute(ast::ExprAttribute { attr, value, .. }) if attr.as_str() == "open" => {
            match value.as_ref() {
                Expr::Call(ast::ExprCall { func, .. }) => semantic
                    .resolve_call_path(func)
                    .is_some_and(|call_path| matches!(call_path.as_slice(), ["pathlib", "Path"]))
                    .then_some(Kind::Pathlib),
                _ => None,
            }
        }
        // Ex) `open(...)`
        Expr::Name(ast::ExprName { id, .. }) => {
            (id.as_str() == "open" && semantic.is_builtin("open")).then_some(Kind::Builtin)
        }
        _ => None,
    }
}

/// Returns the mode argument, if present.
fn extract_mode(call: &ast::ExprCall, kind: Kind) -> Option<&Expr> {
    match kind {
        Kind::Builtin => call.arguments.find_argument("mode", 1),
        Kind::Pathlib => call.arguments.find_argument("mode", 0),
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub(super) struct OpenMode: u8 {
        /// `r`
        const READ = 0b0001;
        /// `w`
        const WRITE = 0b0010;
        /// `a`
        const APPEND = 0b0100;
        /// `x`
        const CREATE = 0b1000;
        /// `b`
        const BINARY = 0b10000;
        /// `t`
        const TEXT = 0b10_0000;
        /// `+`
        const PLUS = 0b100_0000;
        /// `U`
        const UNIVERSAL_NEWLINES = 0b1000_0000;

    }
}

impl TryFrom<char> for OpenMode {
    type Error = ();

    fn try_from(value: char) -> Result<Self, Self::Error> {
        match value {
            'r' => Ok(Self::READ),
            'w' => Ok(Self::WRITE),
            'a' => Ok(Self::APPEND),
            'x' => Ok(Self::CREATE),
            'b' => Ok(Self::BINARY),
            't' => Ok(Self::TEXT),
            '+' => Ok(Self::PLUS),
            'U' => Ok(Self::UNIVERSAL_NEWLINES),
            _ => Err(()),
        }
    }
}

/// Returns `true` if the open mode is valid.
fn is_valid_mode(mode: &str) -> bool {
    // Flag duplicates and invalid characters.
    let mut flags = OpenMode::empty();
    for char in mode.chars() {
        let Ok(flag) = OpenMode::try_from(char) else {
            return false;
        };
        if flags.intersects(flag) {
            return false;
        }
        flags.insert(flag);
    }

    // Both text and binary mode cannot be set at the same time.
    if flags.contains(OpenMode::TEXT | OpenMode::BINARY) {
        return false;
    }

    // The `U` mode is only valid with `r`.
    if flags.contains(OpenMode::UNIVERSAL_NEWLINES)
        && flags.intersects(OpenMode::WRITE | OpenMode::APPEND | OpenMode::CREATE)
    {
        return false;
    }

    // Otherwise, reading, writing, creating, and appending are mutually exclusive.
    [
        OpenMode::READ | OpenMode::UNIVERSAL_NEWLINES,
        OpenMode::WRITE,
        OpenMode::CREATE,
        OpenMode::APPEND,
    ]
    .into_iter()
    .filter(|flag| flags.intersects(*flag))
    .count()
        == 1
}
