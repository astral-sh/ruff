use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{
    Alias, Arguments, Boolop, Cmpop, ExcepthandlerKind, Expr, ExprKind, Stmt, StmtKind,
};

use crate::ast::types::{FunctionScope, Range, ScopeKind};
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLC2201
pub fn misplaced_comparison_constant(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    if let ([op], [right]) = (ops, comparators) {
        if matches!(
            op,
            Cmpop::Eq | Cmpop::NotEq | Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE,
        ) && matches!(&left.node, &ExprKind::Constant { .. })
            && !matches!(&right.node, &ExprKind::Constant { .. })
        {
            let reversed_op = match op {
                Cmpop::Eq => "==",
                Cmpop::NotEq => "!=",
                Cmpop::Lt => ">",
                Cmpop::LtE => ">=",
                Cmpop::Gt => "<",
                Cmpop::GtE => "<=",
                _ => unreachable!("Expected comparison operator"),
            };
            let suggestion = format!("{right} {reversed_op} {left}");
            let mut check = Check::new(
                CheckKind::MisplacedComparisonConstant(suggestion.clone()),
                Range::from_located(expr),
            );
            if checker.patch(check.kind.code()) {
                check.amend(Fix::replacement(
                    suggestion,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
        }
    }
}

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

/// PLR0402
pub fn consider_using_from_import(checker: &mut Checker, alias: &Alias) {
    if let Some(asname) = &alias.node.asname {
        if let Some((module, name)) = alias.node.name.rsplit_once('.') {
            if name == asname {
                checker.add_check(Check::new(
                    CheckKind::ConsiderUsingFromImport(module.to_string(), name.to_string()),
                    Range::from_located(alias),
                ));
            }
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

fn loop_exits_early(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match &stmt.node {
        StmtKind::If { body, .. } => loop_exits_early(body),
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        } => {
            loop_exits_early(body)
                || handlers.iter().any(|handler| match &handler.node {
                    ExcepthandlerKind::ExceptHandler { body, .. } => loop_exits_early(body),
                })
                || loop_exits_early(orelse)
                || loop_exits_early(finalbody)
        }
        StmtKind::For { orelse, .. }
        | StmtKind::AsyncFor { orelse, .. }
        | StmtKind::While { orelse, .. } => loop_exits_early(orelse),
        StmtKind::Break { .. } => true,
        _ => false,
    })
}

/// PLW0120
pub fn useless_else_on_loop(checker: &mut Checker, stmt: &Stmt, body: &[Stmt], orelse: &[Stmt]) {
    if !orelse.is_empty() && !loop_exits_early(body) {
        checker.add_check(Check::new(
            CheckKind::UselessElseOnLoop,
            Range::from_located(stmt),
        ));
    }
}
