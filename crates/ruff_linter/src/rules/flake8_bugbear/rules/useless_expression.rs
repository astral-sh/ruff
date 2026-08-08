use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Expr;
use ruff_python_ast::helpers::contains_effect;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::preview::is_useless_expression_strings_enabled;

use crate::rules::flake8_bugbear::helpers::at_last_top_level_expression_in_cell;

/// ## What it does
/// Checks for useless expressions.
///
/// ## Why is this bad?
/// Useless expressions have no effect on the program, and are often included
/// by mistake. Assign a useless expression to a variable, or remove it
/// entirely.
///
/// ## Example
/// ```python
/// 1 + 1
/// ```
///
/// Use instead:
/// ```python
/// foo = 1 + 1
/// ```
///
/// In [preview], this rule also flags string and f-string literals that are
/// not used as docstrings or [attribute docstrings] (a string immediately
/// following an assignment at the module, class, or `__init__` level). A
/// string immediately following the docstring (an "additional docstring" in
/// [PEP 257]'s terms) is discarded at runtime and is also flagged.
///
/// ## Notebook behavior
/// For Jupyter Notebooks, this rule is not applied to the last top-level expression in a cell.
/// This is because it's common to have a notebook cell that ends with an expression,
/// which will result in the `repr` of the evaluated expression being printed as the cell's output.
///
/// ## Known problems
/// This rule ignores expression types that are commonly used for their side
/// effects, such as function calls.
///
/// However, if a seemingly useless expression (like an attribute access) is
/// needed to trigger a side effect, consider assigning it to an anonymous
/// variable, to indicate that the return value is intentionally ignored.
///
/// For example, given:
/// ```python
/// with errors.ExceptionRaisedContext():
///     obj.attribute
/// ```
///
/// Use instead:
/// ```python
/// with errors.ExceptionRaisedContext():
///     _ = obj.attribute
/// ```
///
/// [preview]: https://docs.astral.sh/ruff/preview/
/// [attribute docstrings]: https://peps.python.org/pep-0257/#what-is-a-docstring
/// [PEP 257]: https://peps.python.org/pep-0257/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.100")]
pub(crate) struct UselessExpression {
    kind: Kind,
}

impl Violation for UselessExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.kind {
            Kind::Expression => {
                "Found useless expression. Either assign it to a variable or remove it.".to_string()
            }
            Kind::Attribute => {
                "Found useless attribute access. Either assign it to a variable or remove it."
                    .to_string()
            }
            Kind::String => {
                "Found useless string statement. Either convert it to a comment or remove it."
                    .to_string()
            }
        }
    }
}

/// B018
pub(crate) fn useless_expression(checker: &Checker, value: &Expr) {
    // Ignore comparisons, as they're handled by `useless_comparison`.
    if value.is_compare_expr() {
        return;
    }

    if value.is_ellipsis_literal_expr() {
        return;
    }

    if matches!(value, Expr::FString(_) | Expr::StringLiteral(_)) {
        // In preview, ignore only docstrings and attribute docstrings; any
        // other standalone string has no effect. On stable, ignore all
        // strings to avoid false positives with docstrings.
        if !is_useless_expression_strings_enabled(checker.settings())
            || checker.semantic().in_pep_257_docstring()
            || checker.semantic().in_attribute_docstring()
        {
            return;
        }
    }

    if checker.source_type.is_ipynb()
        && at_last_top_level_expression_in_cell(
            checker.semantic(),
            checker.locator(),
            checker.cell_offsets(),
        )
    {
        return;
    }

    // Ignore statements that have side effects.
    if contains_effect(value, |id| checker.semantic().has_builtin_binding(id)) {
        // Flag attributes as useless expressions, even if they're attached to calls or other
        // expressions.
        if value.is_attribute_expr() {
            checker.report_diagnostic(
                UselessExpression {
                    kind: Kind::Attribute,
                },
                value.range(),
            );
        }
        return;
    }

    let kind = if matches!(value, Expr::FString(_) | Expr::StringLiteral(_)) {
        Kind::String
    } else {
        Kind::Expression
    };
    checker.report_diagnostic(UselessExpression { kind }, value.range());
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Kind {
    Expression,
    Attribute,
    String,
}
