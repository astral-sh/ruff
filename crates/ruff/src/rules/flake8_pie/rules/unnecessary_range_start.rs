use num_bigint::BigInt;
use ruff_python_ast::{self as ast, Constant, Expr, Ranged};

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::{AlwaysAutofixableViolation, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix::edits::{remove_argument, Parentheses};
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `range` calls with an unnecessary `start` argument.
///
/// ## Why is this bad?
/// `range(0, x)` is equivalent to `range(x)`. The `start` argument is unnecessary.
///
/// ## Example
/// ```python
/// range(0, 3)
/// ```
///
/// Use instead:
/// ```python
/// range(3)
/// ```
#[violation]
pub struct UnnecessaryRangeStart {}

impl AlwaysAutofixableViolation for UnnecessaryRangeStart {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `start` argument in `range`")
    }

    fn autofix_title(&self) -> String {
        format!("Remove `start` argument")
    }
}

/// PIE808
pub(crate) fn unnecessary_range_start(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().is_builtin("range") {
        return;
    };
    let ast::ExprCall {
        func, arguments, ..
    } = call;
    let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
        return
    };
    if id != "range" {
        return;
    };
    let [start, _end] = &arguments.args[..] else {
        return
    };
    let Expr::Constant(ast::ExprConstant { value: Constant::Int(value), .. }) = start else {
        return
    };
    if *value != BigInt::from(0) {
        return;
    };
    let mut diagnostic = Diagnostic::new(UnnecessaryRangeStart {}, start.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            remove_argument(
                &start,
                arguments,
                Parentheses::Remove,
                checker.locator(),
                checker.source_type,
            )
            .map(Fix::automatic)
        });
    }
    checker.diagnostics.push(diagnostic);
}
