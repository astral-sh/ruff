use crate::checkers::ast::Checker;
use rustpython_parser::ast::Expr;

pub fn is_model(checker: &Checker, base: &Expr) -> bool {
    checker.resolve_call_path(base).map_or(false, |call_path| {
        call_path.as_slice() == ["django", "db", "models", "Model"]
    })
}
