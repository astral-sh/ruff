use rustpython_parser::ast::{self, Arguments, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::is_immutable_annotation;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;

#[violation]
pub struct MutableArgumentDefault;

impl Violation for MutableArgumentDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable data structures for argument defaults")
    }
}

/// B006
pub(crate) fn mutable_argument_default(checker: &mut Checker, arguments: &Arguments) {
    // Scan in reverse order to right-align zip().
    for (arg, default) in arguments
        .kwonlyargs
        .iter()
        .rev()
        .zip(arguments.kw_defaults.iter().rev())
        .chain(
            arguments
                .args
                .iter()
                .rev()
                .chain(arguments.posonlyargs.iter().rev())
                .zip(arguments.defaults.iter().rev()),
        )
    {
        if is_mutable_expr(default, checker.semantic())
            && !arg.annotation.as_ref().map_or(false, |expr| {
                is_immutable_annotation(expr, checker.semantic())
            })
        {
            checker
                .diagnostics
                .push(Diagnostic::new(MutableArgumentDefault, default.range()));
        }
    }
}

pub(crate) fn is_mutable_func(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(func).map_or(false, |call_path| {
        matches!(
            call_path.as_slice(),
            ["", "dict"]
                | ["", "list"]
                | ["", "set"]
                | ["collections", "Counter"]
                | ["collections", "OrderedDict"]
                | ["collections", "defaultdict"]
                | ["collections", "deque"]
        )
    })
}

fn is_mutable_expr(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::List(_)
        | Expr::Dict(_)
        | Expr::Set(_)
        | Expr::ListComp(_)
        | Expr::DictComp(_)
        | Expr::SetComp(_) => true,
        Expr::Call(ast::ExprCall { func, .. }) => is_mutable_func(func, semantic),
        _ => false,
    }
}
