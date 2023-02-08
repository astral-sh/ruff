use itertools::Itertools;
use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{Boolop, Expr, ExprKind};

use crate::ast::hashable::HashableExpr;
use crate::ast::helpers::unparse_expr;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct ConsiderMergingIsinstance {
        pub obj: String,
        pub types: Vec<String>,
    }
);
impl Violation for ConsiderMergingIsinstance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderMergingIsinstance { obj, types } = self;
        let types = types.join(", ");
        format!("Merge these isinstance calls: `isinstance({obj}, ({types}))`")
    }
}

/// PLR1701
pub fn merge_isinstance(checker: &mut Checker, expr: &Expr, op: &Boolop, values: &[Expr]) {
    if !matches!(op, Boolop::Or) || !checker.is_builtin("isinstance") {
        return;
    }

    let mut obj_to_types: FxHashMap<HashableExpr, (usize, FxHashSet<HashableExpr>)> =
        FxHashMap::default();
    for value in values {
        let ExprKind::Call { func, args, .. } = &value.node else {
            continue;
        };
        if !matches!(&func.node, ExprKind::Name { id, .. } if id == "isinstance") {
            continue;
        }
        let [obj, types] = &args[..] else {
            continue;
        };
        let (num_calls, matches) = obj_to_types
            .entry(obj.into())
            .or_insert_with(|| (0, FxHashSet::default()));

        *num_calls += 1;
        matches.extend(match &types.node {
            ExprKind::Tuple { elts, .. } => elts.iter().map(HashableExpr::from_expr).collect(),
            _ => {
                vec![types.into()]
            }
        });
    }

    for (obj, (num_calls, types)) in obj_to_types {
        if num_calls > 1 && types.len() > 1 {
            checker.diagnostics.push(Diagnostic::new(
                ConsiderMergingIsinstance {
                    obj: unparse_expr(obj.as_expr(), checker.stylist),
                    types: types
                        .iter()
                        .map(HashableExpr::as_expr)
                        .map(|expr| unparse_expr(expr, checker.stylist))
                        .sorted()
                        .collect(),
                },
                Range::from_located(expr),
            ));
        }
    }
}
