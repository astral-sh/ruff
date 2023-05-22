use rustpython_parser::ast::{self, Arguments, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::is_immutable_annotation;
use ruff_python_semantic::model::SemanticModel;

use crate::checkers::ast::Checker;

#[violation]
pub struct MutableArgumentDefault;

impl Violation for MutableArgumentDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable data structures for argument defaults")
    }
}
const MUTABLE_FUNCS: &[&[&str]] = &[
    &["", "dict"],
    &["", "list"],
    &["", "set"],
    &["collections", "Counter"],
    &["collections", "OrderedDict"],
    &["collections", "defaultdict"],
    &["collections", "deque"],
];

pub(crate) fn is_mutable_func(model: &SemanticModel, func: &Expr) -> bool {
    model.resolve_call_path(func).map_or(false, |call_path| {
        MUTABLE_FUNCS
            .iter()
            .any(|target| call_path.as_slice() == *target)
    })
}

fn is_mutable_expr(model: &SemanticModel, expr: &Expr) -> bool {
    match expr {
        Expr::List(_)
        | Expr::Dict(_)
        | Expr::Set(_)
        | Expr::ListComp(_)
        | Expr::DictComp(_)
        | Expr::SetComp(_) => true,
        Expr::Call(ast::ExprCall { func, .. }) => is_mutable_func(model, func),
        _ => false,
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
        if is_mutable_expr(checker.semantic_model(), default)
            && !arg.annotation.as_ref().map_or(false, |expr| {
                is_immutable_annotation(checker.semantic_model(), expr)
            })
        {
            checker
                .diagnostics
                .push(Diagnostic::new(MutableArgumentDefault, default.range()));
        }
    }
}
