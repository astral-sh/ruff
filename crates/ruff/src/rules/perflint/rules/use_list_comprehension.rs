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
/// Checks for cases where new lists are made by appending elements (with a filter) in a for-loop
///
/// ## Why is this bad?
/// List comprehensions are 25% more efficient at creating new lists, with or without an
/// if-statement. So these should be used when creating new lists
///
/// ## Example
/// ```python
/// original = range(10_000)
/// filtered = []
/// for i in original:
///     if i % 2:
///         filtered.append(i)
/// ```
///
/// Use instead:
/// ```python
/// original = range(10_000)
/// filtered = [x for x in original if x % 2]
/// ```
#[violation]
pub struct UseListComprehension;

impl Violation for UseListComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a list comprehension to create a new filtered list")
    }
}

/// PERF401
pub(crate) fn use_list_comprehension(checker: &mut Checker, body: &[Stmt]) {}
