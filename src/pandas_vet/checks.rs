use rustpython_ast::{Constant, Expr, ExprKind, Keyword};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

/// PDV002
pub fn inplace_argument(keywords: &[Keyword]) -> Option<Check> {
    for keyword in keywords {
        let arg = keyword.node.arg.as_ref()?;

        if arg == "inplace" {
            let is_true_literal = match &keyword.node.value.node {
                ExprKind::Constant {
                    value: Constant::Bool(boolean),
                    ..
                } => *boolean,
                _ => false,
            };
            if is_true_literal {
                return Some(Check::new(
                    CheckKind::UseOfInplaceArgument,
                    Range::from_located(keyword),
                ));
            }
        }
    }
    None
}

/// PDV901
pub fn assignment_to_df(targets: &[Expr]) -> Option<Check> {
    if targets.len() != 1 {
        return None;
    }
    let target = &targets[0];
    let ExprKind::Name { id, .. } = &target.node else {
        return None;
    };
    if id != "df" {
        return None;
    }
    Some(Check::new(
        CheckKind::DfIsABadVariableName,
        Range::from_located(target),
    ))
}
