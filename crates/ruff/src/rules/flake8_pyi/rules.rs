use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct PrefixPrivateTypes {
        pub kind: String,
    }
);
impl Violation for PrefixPrivateTypes {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PrefixPrivateTypes { kind } = self;
        format!("Name of private `{kind}` must start with _")
    }
}

/// Y001
pub fn prefix_private_types(
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

    let is_prefixed = if let ExprKind::Name { id, .. } = &targets[0].node {
        id.starts_with('_')
    } else {
        false
    };

    let is_generic_type_param = if let ExprKind::Call { func, .. } = &value.node {
        if let ExprKind::Name { id, .. } = &func.node {
            id == "TypeVar" || id == "ParamSpec" || id == "TypeVarTuple"
        } else {
            false
        }
    } else {
        false
    };

    if !is_prefixed && is_generic_type_param {
        let diagnostic = Diagnostic::new(
            PrefixPrivateTypes {
                kind: "TypeVar".to_string(),
            },
            Range::from_located(value),
        );
        checker.diagnostics.push(diagnostic);
    }
}
