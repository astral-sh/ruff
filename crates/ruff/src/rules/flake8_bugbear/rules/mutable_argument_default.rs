use rustpython_parser::ast::{Arguments, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::is_immutable_annotation;

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

pub fn is_mutable_func(checker: &Checker, func: &Expr) -> bool {
    checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            MUTABLE_FUNCS
                .iter()
                .any(|target| call_path.as_slice() == *target)
        })
}

fn is_mutable_expr(checker: &Checker, expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::List { .. }
        | ExprKind::Dict { .. }
        | ExprKind::Set { .. }
        | ExprKind::ListComp { .. }
        | ExprKind::DictComp { .. }
        | ExprKind::SetComp { .. } => true,
        ExprKind::Call { func, .. } => is_mutable_func(checker, func),
        _ => false,
    }
}

/// B006
pub fn mutable_argument_default(checker: &mut Checker, arguments: &Arguments) {
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
        if is_mutable_expr(checker, default)
            && !arg
                .node
                .annotation
                .as_ref()
                .map_or(false, |expr| is_immutable_annotation(&checker.ctx, expr))
        {
            checker
                .diagnostics
                .push(Diagnostic::new(MutableArgumentDefault, default.range()));
        }
    }
}
