use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Boolop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLR1701
pub fn consider_merging_isinstance(
    checker: &mut Checker,
    expr: &Expr,
    op: &Boolop,
    values: &[Expr],
) {
    if !matches!(op, Boolop::Or) || !checker.is_builtin("isinstance") {
        return;
    }

    let mut obj_to_types: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();
    for value in values {
        if let ExprKind::Call { func, args, .. } = &value.node {
            if matches!(&func.node, ExprKind::Name { id, .. } if id == "isinstance") {
                if let [obj, types] = &args[..] {
                    obj_to_types
                        .entry(obj.to_string())
                        .or_insert_with(FxHashSet::default)
                        .extend(match &types.node {
                            ExprKind::Tuple { elts, .. } => {
                                elts.iter().map(std::string::ToString::to_string).collect()
                            }
                            _ => {
                                vec![types.to_string()]
                            }
                        });
                }
            }
        }
    }

    for (obj, types) in obj_to_types {
        if types.len() > 1 {
            checker.add_check(Check::new(
                CheckKind::ConsiderMergingIsinstance(obj, types.into_iter().sorted().collect()),
                Range::from_located(expr),
            ));
        }
    }
}
