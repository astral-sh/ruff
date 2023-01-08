use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Boolop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{violations, Diagnostic};

/// PLR1701
pub fn merge_isinstance(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, op: &Boolop, values: &[Expr]) {
    if !matches!(op, Boolop::Or) || !xxxxxxxx.is_builtin("isinstance") {
        return;
    }

    let mut obj_to_types: FxHashMap<String, (usize, FxHashSet<String>)> = FxHashMap::default();
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
            .entry(obj.to_string())
            .or_insert_with(|| (0, FxHashSet::default()));

        *num_calls += 1;
        matches.extend(match &types.node {
            ExprKind::Tuple { elts, .. } => {
                elts.iter().map(std::string::ToString::to_string).collect()
            }
            _ => {
                vec![types.to_string()]
            }
        });
    }

    for (obj, (num_calls, types)) in obj_to_types {
        if num_calls > 1 && types.len() > 1 {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::ConsiderMergingIsinstance(obj, types.into_iter().sorted().collect()),
                Range::from_located(expr),
            ));
        }
    }
}
