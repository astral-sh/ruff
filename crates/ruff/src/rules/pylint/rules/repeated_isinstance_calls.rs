use itertools::Itertools;
use ruff_python_ast::{self as ast, Arguments, BoolOp, Expr};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::autofix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::hashable::HashableExpr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for repeated `isinstance` calls on the same object.
///
/// ## Why is this bad?
/// Repeated `isinstance` calls on the same object can be merged into a
/// single call.
///
/// ## Example
/// ```python
/// def is_number(x):
///     return isinstance(x, int) or isinstance(x, float) or isinstance(x, complex)
/// ```
///
/// Use instead:
/// ```python
/// def is_number(x):
///     return isinstance(x, (int, float, complex))
/// ```
///
/// Or, for Python 3.10 and later:
///
/// ```python
/// def is_number(x):
///     return isinstance(x, int | float | complex)
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `isinstance`](https://docs.python.org/3/library/functions.html#isinstance)
#[violation]
pub struct RepeatedIsinstanceCalls {
    expression: SourceCodeSnippet,
}

impl AlwaysAutofixableViolation for RepeatedIsinstanceCalls {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RepeatedIsinstanceCalls { expression } = self;
        if let Some(expression) = expression.full_display() {
            format!("Merge `isinstance` calls: `{expression}`")
        } else {
            format!("Merge `isinstance` calls")
        }
    }

    fn autofix_title(&self) -> String {
        let RepeatedIsinstanceCalls { expression } = self;
        if let Some(expression) = expression.full_display() {
            format!("Replace with `{expression}`")
        } else {
            format!("Replace with merged `isinstance` call")
        }
    }
}

/// PLR1701
pub(crate) fn repeated_isinstance_calls(
    checker: &mut Checker,
    expr: &Expr,
    op: BoolOp,
    values: &[Expr],
) {
    if !op.is_or() {
        return;
    }

    let mut obj_to_types: FxHashMap<HashableExpr, (usize, FxHashSet<HashableExpr>)> =
        FxHashMap::default();
    for value in values {
        let Expr::Call(ast::ExprCall {
            func,
            arguments: Arguments { args, .. },
            ..
        }) = value
        else {
            continue;
        };
        if !matches!(func.as_ref(), Expr::Name(ast::ExprName { id, .. }) if id == "isinstance") {
            continue;
        }
        let [obj, types] = &args[..] else {
            continue;
        };
        if !checker.semantic().is_builtin("isinstance") {
            return;
        }
        let (num_calls, matches) = obj_to_types
            .entry(obj.into())
            .or_insert_with(|| (0, FxHashSet::default()));

        *num_calls += 1;
        matches.extend(match types {
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                elts.iter().map(HashableExpr::from_expr).collect()
            }
            _ => {
                vec![types.into()]
            }
        });
    }

    for (obj, (num_calls, types)) in obj_to_types {
        if num_calls > 1 && types.len() > 1 {
            let call = merged_isinstance_call(
                &checker.generator().expr(obj.as_expr()),
                types
                    .iter()
                    .map(HashableExpr::as_expr)
                    .map(|expr| checker.generator().expr(expr))
                    .sorted(),
                checker.settings.target_version,
            );
            let mut diagnostic = Diagnostic::new(
                RepeatedIsinstanceCalls {
                    expression: SourceCodeSnippet::new(call.clone()),
                },
                expr.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(call, expr.range())));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn merged_isinstance_call(
    obj: &str,
    types: impl IntoIterator<Item = String>,
    target_version: PythonVersion,
) -> String {
    if target_version >= PythonVersion::Py310 {
        format!("isinstance({}, {})", obj, types.into_iter().join(" | "))
    } else {
        format!("isinstance({}, ({}))", obj, types.into_iter().join(", "))
    }
}
