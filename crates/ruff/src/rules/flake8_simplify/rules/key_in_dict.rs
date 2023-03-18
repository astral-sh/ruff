use rustpython_parser::ast::{Cmpop, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct InDictKeys {
    pub key: String,
    pub dict: String,
}

impl AlwaysAutofixableViolation for InDictKeys {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InDictKeys { key, dict } = self;
        format!("Use `{key} in {dict}` instead of `{key} in {dict}.keys()`")
    }

    fn autofix_title(&self) -> String {
        let InDictKeys { key, dict } = self;
        format!("Convert to `{key} in {dict}`")
    }
}

/// SIM118
fn key_in_dict(checker: &mut Checker, left: &Expr, right: &Expr, range: Range) {
    let ExprKind::Call {
        func,
        args,
        keywords,
    } = &right.node else {
        return;
    };
    if !(args.is_empty() && keywords.is_empty()) {
        return;
    }

    let ExprKind::Attribute { attr, value, .. } = &func.node else {
        return;
    };
    if attr != "keys" {
        return;
    }

    // Slice exact content to preserve formatting.
    let left_content = checker.locator.slice(left);
    let value_content = checker.locator.slice(value);

    let mut diagnostic = Diagnostic::new(
        InDictKeys {
            key: left_content.to_string(),
            dict: value_content.to_string(),
        },
        range,
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            value_content.to_string(),
            right.location,
            right.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM118 in a for loop
pub fn key_in_dict_for(checker: &mut Checker, target: &Expr, iter: &Expr) {
    key_in_dict(
        checker,
        target,
        iter,
        Range::new(target.location, iter.end_location.unwrap()),
    );
}

/// SIM118 in a comparison
pub fn key_in_dict_compare(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    if !matches!(ops[..], [Cmpop::In]) {
        return;
    }

    if comparators.len() != 1 {
        return;
    }
    let right = comparators.first().unwrap();

    key_in_dict(checker, left, right, Range::from(expr));
}
