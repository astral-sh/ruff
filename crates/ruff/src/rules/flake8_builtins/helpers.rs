use ruff_text_size::TextRange;
use rustpython_parser::ast::{Excepthandler, Expr, Ranged, Stmt};

use ruff_python_ast::ranges::identifier_range;
use ruff_python_ast::source_code::Locator;
use ruff_python_stdlib::builtins::BUILTINS;

pub(super) fn shadows_builtin(name: &str, ignorelist: &[String]) -> bool {
    BUILTINS.contains(&name) && ignorelist.iter().all(|ignore| ignore != name)
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum AnyShadowing<'a> {
    Expression(&'a Expr),
    Statement(&'a Stmt),
    ExceptHandler(&'a Excepthandler),
}

impl AnyShadowing<'_> {
    pub(crate) fn range(self, locator: &Locator) -> TextRange {
        match self {
            AnyShadowing::Expression(expr) => expr.range(),
            AnyShadowing::Statement(stmt) => identifier_range(stmt, locator),
            AnyShadowing::ExceptHandler(handler) => handler.range(),
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

impl<'a> From<&'a Excepthandler> for AnyShadowing<'a> {
    fn from(value: &'a Excepthandler) -> Self {
        AnyShadowing::ExceptHandler(value)
    }
}
