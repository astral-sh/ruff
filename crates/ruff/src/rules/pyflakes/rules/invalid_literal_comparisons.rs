use log::error;
use ruff_python_ast::{CmpOp, Expr};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_parser::locate_cmp_ops;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `is` and `is not` comparisons against constant literals, like
/// integers and strings.
///
/// ## Why is this bad?
/// The `is` and `is not` comparators operate on identity, in that they check
/// whether two objects are the same object. If the objects are not the same
/// object, the comparison will always be `False`. Using `is` and `is not` with
/// constant literals often works "by accident", but are not guaranteed to produce
/// the expected result.
///
/// As of Python 3.8, using `is` and `is not` with constant literals will produce
/// a `SyntaxWarning`.
///
/// Instead, use `==` and `!=` to compare constant literals, which will compare
/// the values of the objects instead of their identities.
///
/// ## Example
/// ```python
/// x = 200
/// if x is 200:
///     print("It's 200!")
/// ```
///
/// Use instead:
/// ```python
/// x = 200
/// if x == 200:
///     print("It's 200!")
/// ```
///
/// ## References
/// - [Python documentation: Identity comparisons](https://docs.python.org/3/reference/expressions.html#is-not)
/// - [Python documentation: Value comparisons](https://docs.python.org/3/reference/expressions.html#value-comparisons)
/// - [_Why does Python log a SyntaxWarning for ‘is’ with literals?_ by Adam Johnson](https://adamj.eu/tech/2020/01/21/why-does-python-3-8-syntaxwarning-for-is-literal/)
#[violation]
pub struct IsLiteral {
    cmp_op: IsCmpOp,
}

impl AlwaysAutofixableViolation for IsLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IsLiteral { cmp_op } = self;
        match cmp_op {
            IsCmpOp::Is => format!("Use `==` to compare constant literals"),
            IsCmpOp::IsNot => format!("Use `!=` to compare constant literals"),
        }
    }

    fn autofix_title(&self) -> String {
        let IsLiteral { cmp_op } = self;
        match cmp_op {
            IsCmpOp::Is => "Replace `is` with `==`".to_string(),
            IsCmpOp::IsNot => "Replace `is not` with `!=`".to_string(),
        }
    }
}

/// F632
pub(crate) fn invalid_literal_comparison(
    checker: &mut Checker,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
    expr: &Expr,
) {
    let mut lazy_located = None;
    let mut left = left;
    for (index, (op, right)) in ops.iter().zip(comparators).enumerate() {
        if matches!(op, CmpOp::Is | CmpOp::IsNot)
            && (helpers::is_constant_non_singleton(left)
                || helpers::is_constant_non_singleton(right))
        {
            let mut diagnostic = Diagnostic::new(IsLiteral { cmp_op: op.into() }, expr.range());
            if checker.patch(diagnostic.kind.rule()) {
                if lazy_located.is_none() {
                    lazy_located = Some(locate_cmp_ops(expr, checker.locator().contents()));
                }
                if let Some(located_op) =
                    lazy_located.as_ref().and_then(|located| located.get(index))
                {
                    assert_eq!(located_op.op, *op);
                    if let Some(content) = match located_op.op {
                        CmpOp::Is => Some("==".to_string()),
                        CmpOp::IsNot => Some("!=".to_string()),
                        node => {
                            error!("Failed to fix invalid comparison: {node:?}");
                            None
                        }
                    } {
                        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                            content,
                            located_op.range + expr.start(),
                        )));
                    }
                } else {
                    error!("Failed to fix invalid comparison due to missing op");
                }
            }
            checker.diagnostics.push(diagnostic);
        }
        left = right;
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum IsCmpOp {
    Is,
    IsNot,
}

impl From<&CmpOp> for IsCmpOp {
    fn from(cmp_op: &CmpOp) -> Self {
        match cmp_op {
            CmpOp::Is => IsCmpOp::Is,
            CmpOp::IsNot => IsCmpOp::IsNot,
            _ => panic!("Expected CmpOp::Is | CmpOp::IsNot"),
        }
    }
}
