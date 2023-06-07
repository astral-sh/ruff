use rustpython_parser::ast::{self, Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

const INFINITE_ITERATORS: &[&[&str]] = &[
    &["itertools", "count"],
    &["itertools", "cycle"],
    &["itertools", "repeat"],
];

#[violation]
pub struct ZipWithoutExplicitStrict;

impl Violation for ZipWithoutExplicitStrict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`zip()` without an explicit `strict=` parameter")
    }
}

/// B905
pub(crate) fn zip_without_explicit_strict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    kwargs: &[Keyword],
) {
    if let Expr::Name(ast::ExprName { id, .. }) = func {
        if id == "zip"
            && checker.semantic_model().is_builtin("zip")
            && !args
                .iter()
                .any(|arg| check_infinite_iterators(checker, arg))
            && !kwargs
                .iter()
                .any(|keyword| keyword.arg.as_ref().map_or(false, |name| name == "strict"))
        {
            checker
                .diagnostics
                .push(Diagnostic::new(ZipWithoutExplicitStrict, expr.range()));
        }
    }
}

fn check_infinite_iterators(checker: &mut Checker, arg: &Expr) -> bool {
    let Expr::Call(ast::ExprCall { func, .. }) = &arg else {
        return false;
    };

    return checker
        .semantic_model()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            INFINITE_ITERATORS
                .iter()
                .any(|target| &call_path.as_slice() == target)
        });
}
