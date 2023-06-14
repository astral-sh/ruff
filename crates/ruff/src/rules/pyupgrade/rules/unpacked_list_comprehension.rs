use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for list comprehensions that are immediately unpacked.
///
/// ## Why is this bad?
/// There is no reason to use a list comprehension if the result is immediately
/// unpacked. Instead, use a generator expression, which is more efficient as
/// it avoids allocating an intermediary list.
///
/// ## Example
/// ```python
/// a, b, c = [foo(x) for x in items]
/// ```
///
/// Use instead:
/// ```python
/// a, b, c = (foo(x) for x in items)
/// ```
///
/// ## References
/// - [Python documentation: Generator expressions](https://docs.python.org/3/reference/expressions.html#generator-expressions)
/// - [Python documentation: List comprehensions](https://docs.python.org/3/tutorial/datastructures.html#list-comprehensions)
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

/// Returns `true` if `expr` contains an `Expr::Await`.
fn contains_await(expr: &Expr) -> bool {
    match expr {
        Expr::Await(_) => true,
        Expr::BoolOp(ast::ExprBoolOp { values, .. }) => values.iter().any(contains_await),
        Expr::NamedExpr(ast::ExprNamedExpr {
            target,
            value,
            range: _,
        }) => contains_await(target) || contains_await(value),
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            contains_await(left) || contains_await(right)
        }
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => contains_await(operand),
        Expr::Lambda(ast::ExprLambda { body, .. }) => contains_await(body),
        Expr::IfExp(ast::ExprIfExp {
            test,
            body,
            orelse,
            range: _,
        }) => contains_await(test) || contains_await(body) || contains_await(orelse),
        Expr::Dict(ast::ExprDict {
            keys,
            values,
            range: _,
        }) => keys
            .iter()
            .flatten()
            .chain(values.iter())
            .any(contains_await),
        Expr::Set(ast::ExprSet { elts, range: _ }) => elts.iter().any(contains_await),
        Expr::ListComp(ast::ExprListComp { elt, .. }) => contains_await(elt),
        Expr::SetComp(ast::ExprSetComp { elt, .. }) => contains_await(elt),
        Expr::DictComp(ast::ExprDictComp { key, value, .. }) => {
            contains_await(key) || contains_await(value)
        }
        Expr::GeneratorExp(ast::ExprGeneratorExp { elt, .. }) => contains_await(elt),
        Expr::Yield(ast::ExprYield { value, range: _ }) => {
            value.as_ref().map_or(false, |value| contains_await(value))
        }
        Expr::YieldFrom(ast::ExprYieldFrom { value, range: _ }) => contains_await(value),
        Expr::Compare(ast::ExprCompare {
            left, comparators, ..
        }) => contains_await(left) || comparators.iter().any(contains_await),
        Expr::Call(ast::ExprCall {
            func,
            args,
            keywords,
            range: _,
        }) => {
            contains_await(func)
                || args.iter().any(contains_await)
                || keywords
                    .iter()
                    .any(|keyword| contains_await(&keyword.value))
        }
        Expr::FormattedValue(ast::ExprFormattedValue {
            value, format_spec, ..
        }) => {
            contains_await(value)
                || format_spec
                    .as_ref()
                    .map_or(false, |value| contains_await(value))
        }
        Expr::JoinedStr(ast::ExprJoinedStr { values, range: _ }) => {
            values.iter().any(contains_await)
        }
        Expr::Constant(_) => false,
        Expr::Attribute(ast::ExprAttribute { value, .. }) => contains_await(value),
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            contains_await(value) || contains_await(slice)
        }
        Expr::Starred(ast::ExprStarred { value, .. }) => contains_await(value),
        Expr::Name(_) => false,
        Expr::List(ast::ExprList { elts, .. }) => elts.iter().any(contains_await),
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().any(contains_await),
        Expr::Slice(ast::ExprSlice {
            lower,
            upper,
            step,
            range: _,
        }) => {
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
    if let Expr::Tuple(_) = target {
        if let Expr::ListComp(ast::ExprListComp {
            elt,
            generators,
            range: _,
        }) = value
        {
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
