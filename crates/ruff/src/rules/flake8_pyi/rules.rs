use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct PrefixTypeParams {
        pub kind: String,
    }
);
impl Violation for PrefixTypeParams {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PrefixTypeParams { kind } = self;
        format!("Name of private `{kind}` must start with _")
    }
}

/// Y001
pub fn prefix_type_params(
    checker: &mut Checker,
    value: &Expr,
    targets: &[Expr],
    is_type_stub: bool,
) {
    if !is_type_stub {
        return;
    }
    if targets.len() != 1 {
        return;
    }
    if let ExprKind::Name { id, .. } = &targets[0].node {
        if id.starts_with('_') {
            return;
        }
    };

    let mut type_param_name: Option<&str> = None;
    if let ExprKind::Call { func, .. } = &value.node {
        if let ExprKind::Name { id, .. } = &func.node {
            type_param_name = Some(id);
        }
    }

    if let Some(type_param_name) = type_param_name {
        let diagnostic = Diagnostic::new(
            PrefixTypeParams {
                kind: type_param_name.into(),
            },
            Range::from_located(value),
        );
        checker.diagnostics.push(diagnostic);
    }
}
