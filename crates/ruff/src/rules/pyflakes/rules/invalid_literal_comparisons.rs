use itertools::izip;
use log::error;
use once_cell::unsync::Lazy;
use rustpython_parser::ast::{Cmpop, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;

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
    cmpop: IsCmpop,
}

impl AlwaysAutofixableViolation for IsLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IsLiteral { cmpop } = self;
        match cmpop {
            IsCmpop::Is => format!("Use `==` to compare constant literals"),
            IsCmpop::IsNot => format!("Use `!=` to compare constant literals"),
        }
    }

    fn autofix_title(&self) -> String {
        let IsLiteral { cmpop } = self;
        match cmpop {
            IsCmpop::Is => "Replace `is` with `==`".to_string(),
            IsCmpop::IsNot => "Replace `is not` with `!=`".to_string(),
        }
    }
}

/// F632
pub(crate) fn invalid_literal_comparison(
    checker: &mut Checker,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    expr: &Expr,
) {
    let located = Lazy::new(|| helpers::locate_cmpops(expr, checker.locator));
    let mut left = left;
    for (index, (op, right)) in izip!(ops, comparators).enumerate() {
        if matches!(op, Cmpop::Is | Cmpop::IsNot)
            && (helpers::is_constant_non_singleton(left)
                || helpers::is_constant_non_singleton(right))
        {
            let mut diagnostic = Diagnostic::new(IsLiteral { cmpop: op.into() }, expr.range());
            if checker.patch(diagnostic.kind.rule()) {
                if let Some(located_op) = &located.get(index) {
                    assert_eq!(located_op.op, *op);
                    if let Some(content) = match located_op.op {
                        Cmpop::Is => Some("==".to_string()),
                        Cmpop::IsNot => Some("!=".to_string()),
                        node => {
                            error!("Failed to fix invalid comparison: {node:?}");
                            None
                        }
                    } {
                        #[allow(deprecated)]
                        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
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
enum IsCmpop {
    Is,
    IsNot,
}

impl From<&Cmpop> for IsCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Is => IsCmpop::Is,
            Cmpop::IsNot => IsCmpop::IsNot,
            _ => panic!("Expected Cmpop::Is | Cmpop::IsNot"),
        }
    }
}
