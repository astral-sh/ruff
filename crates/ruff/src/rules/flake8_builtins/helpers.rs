use ruff_text_size::TextRange;
use rustpython_parser::ast::{ExceptHandler, Expr, Ranged, Stmt};

use ruff_python_ast::identifier::{Identifier, TryIdentifier};
use ruff_python_stdlib::builtins::BUILTINS;

pub(super) fn shadows_builtin(name: &str, ignorelist: &[String]) -> bool {
    BUILTINS.contains(&name) && ignorelist.iter().all(|ignore| ignore != name)
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum AnyShadowing<'a> {
    Expression(&'a Expr),
    Statement(&'a Stmt),
    ExceptHandler(&'a ExceptHandler),
}

impl Identifier for AnyShadowing<'_> {
    fn identifier(&self) -> TextRange {
        match self {
            AnyShadowing::Expression(expr) => expr.range(),
            AnyShadowing::Statement(stmt) => stmt.identifier(),
            AnyShadowing::ExceptHandler(handler) => handler.try_identifier().unwrap(),
        }
    }
}

impl<'a> From<&'a Stmt> for AnyShadowing<'a> {
    fn from(value: &'a Stmt) -> Self {
        AnyShadowing::Statement(value)
    }
}

impl<'a> From<&'a Expr> for AnyShadowing<'a> {
    fn from(value: &'a Expr) -> Self {
        AnyShadowing::Expression(value)
    }
}

impl<'a> From<&'a ExceptHandler> for AnyShadowing<'a> {
    fn from(value: &'a ExceptHandler) -> Self {
        AnyShadowing::ExceptHandler(value)
    }
}
