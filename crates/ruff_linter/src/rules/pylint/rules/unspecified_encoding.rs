use std::fmt::{Display, Formatter};

use anyhow::Result;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, StringLiteralFlags};
use ruff_python_semantic::SemanticModel;
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
    mode: ModeArgument,
}

impl AlwaysFixableViolation for UnspecifiedEncoding {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnspecifiedEncoding {
            function_name,
            mode,
        } = self;

        match mode {
            ModeArgument::Supported => {
                format!("`{function_name}` in text mode without explicit `encoding` argument")
            }
            ModeArgument::Unsupported => {
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
    let Some((function_name, mode)) = Callee::try_from_call_expression(call, checker.semantic())
        .filter(|segments| is_violation(call, segments))
        .map(|segments| (segments.to_string(), segments.mode_argument()))
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

/// Represents the path of the function or method being called.
enum Callee<'a> {
    /// Fully-qualified symbol name of the callee.
    Qualified(QualifiedName<'a>),
    /// Attribute value for the `pathlib.Path(...)` call e.g., `open` in
    /// `pathlib.Path(...).open(...)`.
    Pathlib(&'a str),
}

impl<'a> Callee<'a> {
    fn try_from_call_expression(
        call: &'a ast::ExprCall,
        semantic: &'a SemanticModel,
    ) -> Option<Self> {
        if let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = call.func.as_ref() {
            // Check for `pathlib.Path(...).open(...)` or equivalent
            if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
                if semantic
                    .resolve_qualified_name(func)
                    .is_some_and(|qualified_name| {
                        matches!(qualified_name.segments(), ["pathlib", "Path"])
                    })
                {
                    return Some(Callee::Pathlib(attr));
                }
            }
        }

        if let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) {
            return Some(Callee::Qualified(qualified_name));
        }

        None
    }

    fn mode_argument(&self) -> ModeArgument {
        match self {
            Callee::Qualified(qualified_name) => match qualified_name.segments() {
                ["" | "codecs" | "_io", "open"] => ModeArgument::Supported,
                ["tempfile", "TemporaryFile" | "NamedTemporaryFile" | "SpooledTemporaryFile"] => {
                    ModeArgument::Supported
                }
                ["io" | "_io", "TextIOWrapper"] => ModeArgument::Unsupported,
                _ => ModeArgument::Unsupported,
            },
            Callee::Pathlib(attr) => match *attr {
                "open" => ModeArgument::Supported,
                _ => ModeArgument::Unsupported,
            },
        }
    }
}

impl Display for Callee<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Callee::Qualified(qualified_name) => f.write_str(&qualified_name.to_string()),
            Callee::Pathlib(attr) => f.write_str(&format!("pathlib.Path(...).{attr}")),
        }
    }
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
                        value: "locale".to_string().into_boxed_str(),
                        flags: StringLiteralFlags::default(),
                        range: TextRange::default(),
                    }),
                    range: TextRange::default(),
                }))
        ),
        &call.arguments,
        checker.comment_ranges(),
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
        checker.comment_ranges(),
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
fn is_violation(call: &ast::ExprCall, qualified_name: &Callee) -> bool {
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
    match qualified_name {
        Callee::Qualified(qualified_name) => match qualified_name.segments() {
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
            ["tempfile", tempfile_class @ ("TemporaryFile" | "NamedTemporaryFile" | "SpooledTemporaryFile")] =>
            {
                let mode_pos = usize::from(*tempfile_class == "SpooledTemporaryFile");
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
            ["io" | "_io", "TextIOWrapper"] => {
                call.arguments.find_argument("encoding", 1).is_none()
            }
            _ => false,
        },
        Callee::Pathlib(attr) => match *attr {
            "open" => {
                if let Some(mode_arg) = call.arguments.find_argument("mode", 0) {
                    if is_binary_mode(mode_arg).unwrap_or(true) {
                        // binary mode or unknown mode is no violation
                        return false;
                    }
                }
                // else mode not specified, defaults to text mode
                call.arguments.find_argument("encoding", 2).is_none()
            }
            "read_text" => call.arguments.find_argument("encoding", 0).is_none(),
            "write_text" => call.arguments.find_argument("encoding", 1).is_none(),
            _ => false,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModeArgument {
    /// The call supports a `mode` argument.
    Supported,
    /// The call does not support a `mode` argument.
    Unsupported,
}
