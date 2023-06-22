use std::fmt;

use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::Expr;
use rustpython_parser::{ast, lexer, Mode, Tok};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::{Ranged, Stmt};
use ruff_python_ast::source_code::Locator;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for cases where a new list is made as a copy of an existing one by appending elements
/// in a for-loop
///
/// ## Why is this bad?
/// It is more performant to use `list()` or `list.copy()` to copy a list
///
/// ## Example
/// ```python
/// original = range(10_000)
/// filtered = []
/// for i in original:
///     filtered.append(i)
/// ```
///
/// Use instead:
/// ```python
/// original = range(10_000)
/// filtered = list(original)
/// ```
#[violation]
pub struct UseListCopy;

impl Violation for UseListCopy {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `list()` or `list.copy()` to create a copy of a list")
    }
}

/// PERF402
pub(crate) fn use_list_copy(checker: &mut Checker, body: &[Stmt]) {}
