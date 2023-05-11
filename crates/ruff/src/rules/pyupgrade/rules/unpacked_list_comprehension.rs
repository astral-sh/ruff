use rustpython_parser::ast::{self, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UnpackedListComprehension;

impl AlwaysAutofixableViolation for UnpackedListComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace unpacked list comprehension with a generator expression")
    }

    fn autofix_title(&self) -> String {
        "Replace with generator expression".to_string()
    }
}

/// Returns `true` if `expr` contains an `ExprKind::Await`.
fn contains_await(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Await(_) => true,
        ExprKind::BoolOp(ast::ExprBoolOp { values, .. }) => values.iter().any(contains_await),
        ExprKind::NamedExpr(ast::ExprNamedExpr { target, value }) => {
            contains_await(target) || contains_await(value)
        }
        ExprKind::BinOp(ast::ExprBinOp { left, right, .. }) => {
            contains_await(left) || contains_await(right)
        }
        ExprKind::UnaryOp(ast::ExprUnaryOp { operand, .. }) => contains_await(operand),
        ExprKind::Lambda(ast::ExprLambda { body, .. }) => contains_await(body),
        ExprKind::IfExp(ast::ExprIfExp { test, body, orelse }) => {
            contains_await(test) || contains_await(body) || contains_await(orelse)
        }
        ExprKind::Dict(ast::ExprDict { keys, values }) => keys
            .iter()
            .flatten()
            .chain(values.iter())
            .any(contains_await),
        ExprKind::Set(ast::ExprSet { elts }) => elts.iter().any(contains_await),
        ExprKind::ListComp(ast::ExprListComp { elt, .. }) => contains_await(elt),
        ExprKind::SetComp(ast::ExprSetComp { elt, .. }) => contains_await(elt),
        ExprKind::DictComp(ast::ExprDictComp { key, value, .. }) => {
            contains_await(key) || contains_await(value)
        }
        ExprKind::GeneratorExp(ast::ExprGeneratorExp { elt, .. }) => contains_await(elt),
        ExprKind::Yield(ast::ExprYield { value }) => {
            value.as_ref().map_or(false, |value| contains_await(value))
        }
        ExprKind::YieldFrom(ast::ExprYieldFrom { value }) => contains_await(value),
        ExprKind::Compare(ast::ExprCompare {
            left, comparators, ..
        }) => contains_await(left) || comparators.iter().any(contains_await),
        ExprKind::Call(ast::ExprCall {
            func,
            args,
            keywords,
        }) => {
            contains_await(func)
                || args.iter().any(contains_await)
                || keywords
                    .iter()
                    .any(|keyword| contains_await(&keyword.node.value))
        }
        ExprKind::FormattedValue(ast::ExprFormattedValue {
            value, format_spec, ..
        }) => {
            contains_await(value)
                || format_spec
                    .as_ref()
                    .map_or(false, |value| contains_await(value))
        }
        ExprKind::JoinedStr(ast::ExprJoinedStr { values }) => values.iter().any(contains_await),
        ExprKind::Constant(_) => false,
        ExprKind::Attribute(ast::ExprAttribute { value, .. }) => contains_await(value),
        ExprKind::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            contains_await(value) || contains_await(slice)
        }
        ExprKind::Starred(ast::ExprStarred { value, .. }) => contains_await(value),
        ExprKind::Name(_) => false,
        ExprKind::List(ast::ExprList { elts, .. }) => elts.iter().any(contains_await),
        ExprKind::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().any(contains_await),
        ExprKind::Slice(ast::ExprSlice { lower, upper, step }) => {
            lower.as_ref().map_or(false, |value| contains_await(value))
                || upper.as_ref().map_or(false, |value| contains_await(value))
                || step.as_ref().map_or(false, |value| contains_await(value))
        }
    }
}

/// UP027
pub(crate) fn unpacked_list_comprehension(checker: &mut Checker, targets: &[Expr], value: &Expr) {
    let Some(target) = targets.get(0) else {
        return;
    };
    if let ExprKind::Tuple(_) = target.node {
        if let ExprKind::ListComp(ast::ExprListComp { elt, generators }) = &value.node {
            if generators.iter().any(|generator| generator.is_async) || contains_await(elt) {
                return;
            }

            let mut diagnostic = Diagnostic::new(UnpackedListComprehension, value.range());
            if checker.patch(diagnostic.kind.rule()) {
                let existing = checker.locator.slice(value.range());

                let mut content = String::with_capacity(existing.len());
                content.push('(');
                content.push_str(&existing[1..existing.len() - 1]);
                content.push(')');
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                    content,
                    value.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
