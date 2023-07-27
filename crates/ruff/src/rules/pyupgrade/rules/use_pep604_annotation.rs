use itertools::Either::{Left, Right};
use itertools::Itertools;
use ruff_python_ast::{self as ast, Expr, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::Pep604Operator;
use ruff_source_file::Locator;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Check for type annotations that can be rewritten based on [PEP 604] syntax.
///
/// ## Why is this bad?
/// [PEP 604] introduced a new syntax for union type annotations based on the
/// `|` operator. This syntax is more concise and readable than the previous
/// `typing.Union` and `typing.Optional` syntaxes.
///
/// ## Example
/// ```python
/// from typing import Union
///
/// foo: Union[int, str] = 1
/// ```
///
/// Use instead:
/// ```python
/// foo: int | str = 1
/// ```
///
/// ## Options
/// - `target-version`
/// - `pyupgrade.keep-runtime-typing`
///
/// [PEP 604]: https://peps.python.org/pep-0604/
#[violation]
pub struct NonPEP604Annotation;

impl Violation for NonPEP604Annotation {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `X | Y` for type annotations")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Convert to `X | Y`".to_string())
    }
}

/// UP007
pub(crate) fn use_pep604_annotation(
    checker: &mut Checker,
    expr: &Expr,
    slice: &Expr,
    operator: Pep604Operator,
) {
    // Avoid fixing forward references, or types not in an annotation.
    let fixable = checker.semantic().in_type_definition()
        && !checker.semantic().in_complex_string_type_definition();

    match operator {
        Pep604Operator::Optional => {
            let mut diagnostic = Diagnostic::new(NonPEP604Annotation, expr.range());
            if fixable && checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    optional(slice, checker.locator()),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
        Pep604Operator::Union => {
            let mut diagnostic = Diagnostic::new(NonPEP604Annotation, expr.range());
            if fixable && checker.patch(diagnostic.kind.rule()) {
                match slice {
                    Expr::Slice(_) => {
                        // Invalid type annotation.
                    }
                    Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                            union(elts, checker.locator()),
                            expr.range(),
                        )));
                    }
                    _ => {
                        // Single argument.
                        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                            checker.locator().slice(slice.range()).to_string(),
                            expr.range(),
                        )));
                    }
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Format the expression as a PEP 604-style optional.
fn optional(expr: &Expr, locator: &Locator) -> String {
    format!("{} | None", locator.slice(expr.range()))
}

/// Format the expressions as a PEP 604-style union.
fn union(elts: &[Expr], locator: &Locator) -> String {
    let mut elts = elts
        .iter()
        .flat_map(|expr| match expr {
            Expr::Tuple(ast::ExprTuple { elts, .. }) => Left(elts.iter()),
            _ => Right(std::iter::once(expr)),
        })
        .peekable();
    if elts.peek().is_none() {
        "()".to_string()
    } else {
        elts.map(|expr| locator.slice(expr.range())).join(" | ")
    }
}
