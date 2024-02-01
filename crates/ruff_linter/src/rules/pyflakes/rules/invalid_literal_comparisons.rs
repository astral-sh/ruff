use log::error;
use ruff_python_ast::{CmpOp, Expr};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_parser::{lexer, Mode, Tok};
use ruff_text_size::{Ranged, TextRange, TextSize};

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
                lazy_located = Some(locate_cmp_ops(expr, checker.locator().contents()));
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
                        located_op.range + expr.start(),
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

/// Extract all [`CmpOp`] operators from an expression snippet, with appropriate
/// ranges.
///
/// `RustPython` doesn't include line and column information on [`CmpOp`] nodes.
/// `CPython` doesn't either. This method iterates over the token stream and
/// re-identifies [`CmpOp`] nodes, annotating them with valid ranges.
fn locate_cmp_ops(expr: &Expr, source: &str) -> Vec<LocatedCmpOp> {
    // If `Expr` is a multi-line expression, we need to parenthesize it to
    // ensure that it's lexed correctly.
    let contents = &source[expr.range()];
    let parenthesized_contents = format!("({contents})");
    let mut tok_iter = lexer::lex(&parenthesized_contents, Mode::Expression)
        .flatten()
        .skip(1)
        .map(|(tok, range)| (tok, range - TextSize::from(1)))
        .filter(|(tok, _)| !matches!(tok, Tok::NonLogicalNewline | Tok::Comment(_)))
        .peekable();

    let mut ops: Vec<LocatedCmpOp> = vec![];

    // Track the bracket depth.
    let mut par_count = 0u32;
    let mut sqb_count = 0u32;
    let mut brace_count = 0u32;

    loop {
        let Some((tok, range)) = tok_iter.next() else {
            break;
        };

        match tok {
            Tok::Lpar => {
                par_count = par_count.saturating_add(1);
            }
            Tok::Rpar => {
                par_count = par_count.saturating_sub(1);
            }
            Tok::Lsqb => {
                sqb_count = sqb_count.saturating_add(1);
            }
            Tok::Rsqb => {
                sqb_count = sqb_count.saturating_sub(1);
            }
            Tok::Lbrace => {
                brace_count = brace_count.saturating_add(1);
            }
            Tok::Rbrace => {
                brace_count = brace_count.saturating_sub(1);
            }
            _ => {}
        }

        if par_count > 0 || sqb_count > 0 || brace_count > 0 {
            continue;
        }

        match tok {
            Tok::Not => {
                if let Some((_, next_range)) = tok_iter.next_if(|(tok, _)| tok.is_in()) {
                    ops.push(LocatedCmpOp::new(
                        TextRange::new(range.start(), next_range.end()),
                        CmpOp::NotIn,
                    ));
                }
            }
            Tok::In => {
                ops.push(LocatedCmpOp::new(range, CmpOp::In));
            }
            Tok::Is => {
                let op = if let Some((_, next_range)) = tok_iter.next_if(|(tok, _)| tok.is_not()) {
                    LocatedCmpOp::new(
                        TextRange::new(range.start(), next_range.end()),
                        CmpOp::IsNot,
                    )
                } else {
                    LocatedCmpOp::new(range, CmpOp::Is)
                };
                ops.push(op);
            }
            Tok::NotEqual => {
                ops.push(LocatedCmpOp::new(range, CmpOp::NotEq));
            }
            Tok::EqEqual => {
                ops.push(LocatedCmpOp::new(range, CmpOp::Eq));
            }
            Tok::GreaterEqual => {
                ops.push(LocatedCmpOp::new(range, CmpOp::GtE));
            }
            Tok::Greater => {
                ops.push(LocatedCmpOp::new(range, CmpOp::Gt));
            }
            Tok::LessEqual => {
                ops.push(LocatedCmpOp::new(range, CmpOp::LtE));
            }
            Tok::Less => {
                ops.push(LocatedCmpOp::new(range, CmpOp::Lt));
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

    #[test]
    fn extract_cmp_op_location() -> Result<()> {
        let contents = "x == 1";
        let expr = parse_expression(contents)?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::Eq
            )]
        );

        let contents = "x != 1";
        let expr = parse_expression(contents)?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::NotEq
            )]
        );

        let contents = "x is 1";
        let expr = parse_expression(contents)?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::Is
            )]
        );

        let contents = "x is not 1";
        let expr = parse_expression(contents)?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(8),
                CmpOp::IsNot
            )]
        );

        let contents = "x in 1";
        let expr = parse_expression(contents)?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::In
            )]
        );

        let contents = "x not in 1";
        let expr = parse_expression(contents)?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(8),
                CmpOp::NotIn
            )]
        );

        let contents = "x != (1 is not 2)";
        let expr = parse_expression(contents)?;
        assert_eq!(
            locate_cmp_ops(&expr, contents),
            vec![LocatedCmpOp::new(
                TextSize::from(2)..TextSize::from(4),
                CmpOp::NotEq
            )]
        );

        Ok(())
    }
}
