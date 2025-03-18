use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::rules::refurb::helpers::replace_with_identity_check;

/// ## What it does
/// Checks for uses of `type` that compare the type of an object to the type of `None`.
///
/// ## Why is this bad?
/// There is only ever one instance of `None`, so it is more efficient and
/// readable to use the `is` operator to check if an object is `None`.
///
/// ## Example
/// ```python
/// type(obj) is type(None)
/// ```
///
/// Use instead:
/// ```python
/// obj is None
/// ```
///
/// ## Fix safety
/// If the fix might remove comments, it will be marked as unsafe.
///
/// ## References
/// - [Python documentation: `isinstance`](https://docs.python.org/3/library/functions.html#isinstance)
/// - [Python documentation: `None`](https://docs.python.org/3/library/constants.html#None)
/// - [Python documentation: `type`](https://docs.python.org/3/library/functions.html#type)
/// - [Python documentation: Identity comparisons](https://docs.python.org/3/reference/expressions.html#is-not)
#[derive(ViolationMetadata)]
pub(crate) struct TypeNoneComparison {
    replacement: IdentityCheck,
}

impl AlwaysFixableViolation for TypeNoneComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "When checking against `None`, use `{}` instead of comparison with `type(None)`",
            self.replacement.op()
        )
    }

    fn fix_title(&self) -> String {
        format!("Replace with `{} None`", self.replacement.op())
    }
}

/// FURB169
pub(crate) fn type_none_comparison(checker: &Checker, compare: &ast::ExprCompare) {
    let ([op], [right]) = (&*compare.ops, &*compare.comparators) else {
        return;
    };

    let replacement = match op {
        CmpOp::Is | CmpOp::Eq => IdentityCheck::Is,
        CmpOp::IsNot | CmpOp::NotEq => IdentityCheck::IsNot,
        _ => return,
    };

    let Some(left_arg) = type_call_arg(&compare.left, checker.semantic()) else {
        return;
    };
    let Some(right_arg) = type_call_arg(right, checker.semantic()) else {
        return;
    };

    let other_arg = match (left_arg, right_arg) {
        (Expr::NoneLiteral(_), _) => right_arg,
        (_, Expr::NoneLiteral(_)) => left_arg,
        _ => return,
    };

    let diagnostic = Diagnostic::new(TypeNoneComparison { replacement }, compare.range);

    let negate = replacement == IdentityCheck::IsNot;
    let fix = replace_with_identity_check(other_arg, compare.range, negate, checker);

    checker.report_diagnostic(diagnostic.with_fix(fix));
}

/// Returns the object passed to the function, if the expression is a call to
/// `type` with a single argument.
fn type_call_arg<'a>(expr: &'a Expr, semantic: &'a SemanticModel) -> Option<&'a Expr> {
    // The expression must be a single-argument call to `type`.
    let ast::ExprCall {
        func, arguments, ..
    } = expr.as_call_expr()?;
    if arguments.len() != 1 {
        return None;
    }
    // The function itself must be the builtin `type`.
    if !semantic.match_builtin_expr(func, "type") {
        return None;
    }
    arguments.find_positional(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentityCheck {
    Is,
    IsNot,
}

impl IdentityCheck {
    fn op(self) -> CmpOp {
        match self {
            Self::Is => CmpOp::Is,
            Self::IsNot => CmpOp::IsNot,
        }
    }
}
