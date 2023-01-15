use crate::rules::pyflakes::format::FormatSummary;
use crate::checkers::ast::Checker;
use rustpython_ast::Expr;

/// UP032
pub(crate) fn f_strings(checker: &mut Checker, summary: &FormatSummary, expr: &Expr) {
}
