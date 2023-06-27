use rustpython_parser::ast::{self, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Definition, Member, MemberKind};
use ruff_python_whitespace::UniversalNewlines;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;

/// ## What it does
/// Checks for the function's signature in the first line of the docstring.
///
/// ## Why is this bad?
/// [PEP 257](https://peps.python.org/pep-0257/#what-is-a-docstring) recommends
/// against including the function's signature in the docstring.
///
/// Instead, consider using type annotations for the function's parameters and
/// return value.
///
///
/// ## Example
/// ```python
/// def function(a, b):
///     """function(a: int, b: int) -> list"""
/// ```
///
/// Use instead:
/// ```python
/// def function(a: int, b: int) -> list:
///     """Return a list of a and b."""
/// ```
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
/// - [PEP 484 – Type Hints](https://peps.python.org/pep-0484/)
#[violation]
pub struct NoSignature;

impl Violation for NoSignature {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should not be the function's signature")
    }
}

/// D402
pub(crate) fn no_signature(checker: &mut Checker, docstring: &Docstring) {
    let Definition::Member(Member {
        kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
        stmt,
        ..
    }) = docstring.definition else {
        return;
    };
    let Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) = stmt else {
        return;
    };

    let body = docstring.body();

    let Some(first_line) = body.trim().universal_newlines().next() else {
        return;
    };

    if !first_line.contains(&format!("{name}(")) {
        return;
    };

    checker
        .diagnostics
        .push(Diagnostic::new(NoSignature, docstring.range()));
}
