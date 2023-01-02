use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};

/// Returns `true` if `expr` contains an `ExprKind::Await`.
fn contains_await(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Await { .. } => true,
        ExprKind::BoolOp { values, .. } => values.iter().any(contains_await),
        ExprKind::NamedExpr { target, value } => contains_await(target) || contains_await(value),
        ExprKind::BinOp { left, right, .. } => contains_await(left) || contains_await(right),
        ExprKind::UnaryOp { operand, .. } => contains_await(operand),
        ExprKind::Lambda { body, .. } => contains_await(body),
        ExprKind::IfExp { test, body, orelse } => {
            contains_await(test) || contains_await(body) || contains_await(orelse)
        }
        ExprKind::Dict { keys, values } => keys.iter().chain(values.iter()).any(contains_await),
        ExprKind::Set { elts } => elts.iter().any(contains_await),
        ExprKind::ListComp { elt, .. } => contains_await(elt),
        ExprKind::SetComp { elt, .. } => contains_await(elt),
        ExprKind::DictComp { key, value, .. } => contains_await(key) || contains_await(value),
        ExprKind::GeneratorExp { elt, .. } => contains_await(elt),
        ExprKind::Yield { value } => value.as_ref().map_or(false, |value| contains_await(value)),
        ExprKind::YieldFrom { value } => contains_await(value),
        ExprKind::Compare {
            left, comparators, ..
        } => contains_await(left) || comparators.iter().any(contains_await),
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            contains_await(func)
                || args.iter().any(contains_await)
                || keywords
                    .iter()
                    .any(|keyword| contains_await(&keyword.node.value))
        }
        ExprKind::FormattedValue {
            value, format_spec, ..
        } => {
            contains_await(value)
                || format_spec
                    .as_ref()
                    .map_or(false, |value| contains_await(value))
        }
        ExprKind::JoinedStr { values } => values.iter().any(contains_await),
        ExprKind::Constant { .. } => false,
        ExprKind::Attribute { value, .. } => contains_await(value),
        ExprKind::Subscript { value, slice, .. } => contains_await(value) || contains_await(slice),
        ExprKind::Starred { value, .. } => contains_await(value),
        ExprKind::Name { .. } => false,
        ExprKind::List { elts, .. } => elts.iter().any(contains_await),
        ExprKind::Tuple { elts, .. } => elts.iter().any(contains_await),
        ExprKind::Slice { lower, upper, step } => {
            lower.as_ref().map_or(false, |value| contains_await(value))
                || upper.as_ref().map_or(false, |value| contains_await(value))
                || step.as_ref().map_or(false, |value| contains_await(value))
        }
    }
}

/// UP027
pub fn unpack_list_comprehension(checker: &mut Checker, targets: &[Expr], value: &Expr) {
    let Some(target) = targets.get(0) else {
        return;
    };
    if let ExprKind::Tuple { .. } = target.node {
        if let ExprKind::ListComp { elt, generators } = &value.node {
            if generators.iter().any(|generator| generator.is_async > 0) || contains_await(elt) {
                return;
            }

            let mut check = Check::new(
                CheckKind::RewriteListComprehension,
                Range::from_located(value),
            );
            if checker.patch(&CheckCode::UP027) {
                let existing = checker
                    .locator
                    .slice_source_code_range(&Range::from_located(value));

                let mut content = String::with_capacity(existing.len());
                content.push('(');
                content.push_str(&existing[1..existing.len() - 1]);
                content.push(')');
                check.amend(Fix::replacement(
                    content,
                    value.location,
                    value.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
        }
    }
}
