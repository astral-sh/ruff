use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Arguments, Boolop, Expr, ExprKind, Stmt};

use crate::ast::types::{FunctionScope, Range, ScopeKind};
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLC3002
pub fn unnecessary_direct_lambda_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Lambda { .. } = &func.node {
        checker.add_check(Check::new(
            CheckKind::UnnecessaryDirectLambdaCall,
            Range::from_located(expr),
        ));
    }
}

/// PLE1142
pub fn await_outside_async(checker: &mut Checker, expr: &Expr) {
    if !checker
        .current_scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionScope { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(true)
    {
        checker.add_check(Check::new(
            CheckKind::AwaitOutsideAsync,
            Range::from_located(expr),
        ));
    }
}

/// PLR0206
pub fn property_with_parameters(
    checker: &mut Checker,
    stmt: &Stmt,
    decorator_list: &[Expr],
    args: &Arguments,
) {
    if decorator_list
        .iter()
        .any(|d| matches!(&d.node, ExprKind::Name { id, .. } if id == "property"))
    {
        if checker.is_builtin("property")
            && args
                .args
                .iter()
                .chain(args.posonlyargs.iter())
                .chain(args.kwonlyargs.iter())
                .count()
                > 1
        {
            checker.add_check(Check::new(
                CheckKind::PropertyWithParameters,
                Range::from_located(stmt),
            ));
        }
    }
}

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
