use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, Operator, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::Pep604Operator;

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

fn optional(expr: &Expr) -> Expr {
    Expr::BinOp(ast::ExprBinOp {
        left: Box::new(expr.clone()),
        op: Operator::BitOr,
        right: Box::new(Expr::Constant(ast::ExprConstant {
            value: Constant::None,
            kind: None,
            range: TextRange::default(),
        })),
        range: TextRange::default(),
    })
}

fn union(elts: &[Expr]) -> Expr {
    if elts.len() == 1 {
        elts[0].clone()
    } else {
        Expr::BinOp(ast::ExprBinOp {
            left: Box::new(union(&elts[..elts.len() - 1])),
            op: Operator::BitOr,
            right: Box::new(elts[elts.len() - 1].clone()),
            range: TextRange::default(),
        })
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
                diagnostic.set_fix(Fix::manual(Edit::range_replacement(
                    checker.generator().expr(&optional(slice)),
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
                        diagnostic.set_fix(Fix::manual(Edit::range_replacement(
                            checker.generator().expr(&union(elts)),
                            expr.range(),
                        )));
                    }
                    _ => {
                        // Single argument.
                        diagnostic.set_fix(Fix::manual(Edit::range_replacement(
                            checker.generator().expr(slice),
                            expr.range(),
                        )));
                    }
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
