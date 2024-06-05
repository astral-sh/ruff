use log::error;
use ruff_python_ast::{CmpOp, Expr};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_parser::{TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

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
/// In [preview], this rule will also flag `is` and `is not` comparisons against
/// non-constant literals, like lists, sets, and dictionaries. While such
/// comparisons will not raise a `SyntaxWarning`, they are still likely to be
/// incorrect, as they will compare the identities of the objects instead of
/// their values, which will always evaluate to `False`.
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
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[violation]
pub struct IsLiteral {
    cmp_op: IsCmpOp,
}

impl AlwaysFixableViolation for IsLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IsLiteral { cmp_op } = self;
        match cmp_op {
            IsCmpOp::Is => format!("Use `==` to compare constant literals"),
            IsCmpOp::IsNot => format!("Use `!=` to compare constant literals"),
        }
    }

    fn fix_title(&self) -> String {
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
                || helpers::is_constant_non_singleton(right)
                || (checker.settings.preview.is_enabled()
                    && (helpers::is_mutable_iterable_initializer(left)
                        || helpers::is_mutable_iterable_initializer(right))))
        {
            let mut diagnostic = Diagnostic::new(IsLiteral { cmp_op: op.into() }, expr.range());
            if lazy_located.is_none() {
                lazy_located = Some(locate_cmp_ops(expr, checker.tokens()));
            }
            if let Some(located_op) = lazy_located.as_ref().and_then(|located| located.get(index)) {
                assert_eq!(located_op.op, *op);
                if let Some(content) = match located_op.op {
                    CmpOp::Is => Some("==".to_string()),
                    CmpOp::IsNot => Some("!=".to_string()),
                    node => {
                        error!("Failed to fix invalid comparison: {node:?}");
                        None
                    }
                } {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        content,
                        located_op.range,
                    )));
                }
            } else {
                error!("Failed to fix invalid comparison due to missing op");
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

/// Extract all [`CmpOp`] operators from an expression snippet, with appropriate ranges.
///
/// This method iterates over the token stream and re-identifies [`CmpOp`] nodes, annotating them
/// with valid ranges.
fn locate_cmp_ops(expr: &Expr, tokens: &Tokens) -> Vec<LocatedCmpOp> {
    let mut tok_iter = tokens
        .in_range(expr.range())
        .iter()
        .filter(|token| !token.is_trivia())
        .peekable();

    let mut ops: Vec<LocatedCmpOp> = vec![];

    // Track the nesting level.
    let mut nesting = 0u32;

    loop {
        let Some(token) = tok_iter.next() else {
            break;
        };

        match token.kind() {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => {
                nesting = nesting.saturating_add(1);
            }
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                nesting = nesting.saturating_sub(1);
            }
            _ => {}
        }

        if nesting > 0 {
            continue;
        }

        match token.kind() {
            TokenKind::Not => {
                if let Some(next_token) = tok_iter.next_if(|token| token.kind() == TokenKind::In) {
                    ops.push(LocatedCmpOp::new(
                        TextRange::new(token.start(), next_token.end()),
                        CmpOp::NotIn,
                    ));
                }
            }
            TokenKind::In => {
                ops.push(LocatedCmpOp::new(token.range(), CmpOp::In));
            }
            TokenKind::Is => {
                let op = if let Some(next_token) =
                    tok_iter.next_if(|token| token.kind() == TokenKind::Not)
                {
                    LocatedCmpOp::new(
                        TextRange::new(token.start(), next_token.end()),
                        CmpOp::IsNot,
                    )
                } else {
                    LocatedCmpOp::new(token.range(), CmpOp::Is)
                };
                ops.push(op);
            }
            TokenKind::NotEqual => {
                ops.push(LocatedCmpOp::new(token.range(), CmpOp::NotEq));
            }
            TokenKind::EqEqual => {
                ops.push(LocatedCmpOp::new(token.range(), CmpOp::Eq));
            }
            TokenKind::GreaterEqual => {
                ops.push(LocatedCmpOp::new(token.range(), CmpOp::GtE));
            }
            TokenKind::Greater => {
                ops.push(LocatedCmpOp::new(token.range(), CmpOp::Gt));
            }
            TokenKind::LessEqual => {
                ops.push(LocatedCmpOp::new(token.range(), CmpOp::LtE));
            }
            TokenKind::Less => {
                ops.push(LocatedCmpOp::new(token.range(), CmpOp::Lt));
            }
            _ => {}
        }
    }
    ops
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocatedCmpOp {
    range: TextRange,
    op: CmpOp,
}

impl LocatedCmpOp {
    fn new<T: Into<TextRange>>(range: T, op: CmpOp) -> Self {
        Self {
            range: range.into(),
            op,
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use ruff_python_ast::CmpOp;
    use ruff_python_parser::parse_expression;
    use ruff_text_size::TextSize;

    use super::{locate_cmp_ops, LocatedCmpOp};

    fn extract_cmp_op_locations(source: &str) -> Result<Vec<LocatedCmpOp>> {
        let parsed = parse_expression(source)?;
        Ok(locate_cmp_ops(parsed.expr(), parsed.tokens()))
    }

    #[test]
    fn test_locate_cmp_ops() -> Result<()> {
        let contents = "x == 1";
        assert_eq!(
            extract_cmp_op_locations(contents)?,
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::Eq
            )]
        );

        let contents = "x != 1";
        assert_eq!(
            extract_cmp_op_locations(contents)?,
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::NotEq
            )]
        );

        let contents = "x is 1";
        assert_eq!(
            extract_cmp_op_locations(contents)?,
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::Is
            )]
        );

        let contents = "x is not 1";
        assert_eq!(
            extract_cmp_op_locations(contents)?,
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(8),
                CmpOp::IsNot
            )]
        );

        let contents = "x in 1";
        assert_eq!(
            extract_cmp_op_locations(contents)?,
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::In
            )]
        );

        let contents = "x not in 1";
        assert_eq!(
            extract_cmp_op_locations(contents)?,
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(8),
                CmpOp::NotIn
            )]
        );

        let contents = "x != (1 is not 2)";
        assert_eq!(
            extract_cmp_op_locations(contents)?,
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::NotEq
            )]
        );

        Ok(())
    }
}
