use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `collections.namedtuple` in stub files.
///
/// ## Why is this bad?
/// `typing.NamedTuple` is the "typed version" of `collections.namedtuple`.
///
/// Inheriting from `typing.NamedTuple` creates a custom `tuple` subclass in
/// the same way as using the `collections.namedtuple` factory function.
/// However, using `typing.NamedTuple` allows you to provide a type annotation
/// for each field in the class. This means that type checkers will have more
/// information to work with, and will be able to analyze your code more
/// precisely.
///
/// ## Example
/// ```pyi
/// from collections import namedtuple
///
/// person = namedtuple("Person", ["name", "age"])
/// ```
///
/// Use instead:
/// ```pyi
/// from typing import NamedTuple
///
/// class Person(NamedTuple):
///     name: str
///     age: int
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct CollectionsNamedTuple;

impl Violation for CollectionsNamedTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `typing.NamedTuple` instead of `collections.namedtuple`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `typing.NamedTuple`".to_string())
    }
}

/// PYI024
pub(crate) fn collections_named_tuple(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::COLLECTIONS) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["collections", "namedtuple"])
        })
    {
        checker.report_diagnostic(Diagnostic::new(CollectionsNamedTuple, expr.range()));
    }
}
