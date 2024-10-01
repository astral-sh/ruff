use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
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
#[violation]
pub struct CollectionsNamedTuple;

impl Violation for CollectionsNamedTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `typing.NamedTuple` instead of `collections.namedtuple`")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `typing.NamedTuple`"))
    }
}

/// PYI024
pub(crate) fn collections_named_tuple(checker: &mut Checker, expr: &Expr) {
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
        checker
            .diagnostics
            .push(Diagnostic::new(CollectionsNamedTuple, expr.range()));
    }
}
