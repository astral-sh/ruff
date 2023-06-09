//! This module provides helper utilities to format an expression that has a left side, an operator,
//! and a right side (binary like).

use crate::prelude::*;
use ruff_formatter::{format_args, write};
use rustpython_parser::ast::{BoolOp, Expr, ExprBinOp, Operator};

#[derive(Clone, Debug)]
pub(super) struct FormatBinaryLike<'a> {
    layout: BinaryLikeLayout,
    binary: BinaryLike<'a>,
}

impl<'a> FormatBinaryLike<'a> {
    pub(super) fn expand_left(binary: BinaryLike<'a>) -> Self {
        Self {
            layout: BinaryLikeLayout::ExpandLeft,
            binary,
        }
    }

    pub(super) fn expand_right(binary: BinaryLike<'a>) -> Self {
        Self {
            layout: BinaryLikeLayout::ExpandRight,
            binary,
        }
    }

    pub(super) fn expand_right_then_left(binary: BinaryLike<'a>) -> Self {
        Self {
            layout: BinaryLikeLayout::ExpandRightThenLeft,
            binary,
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatBinaryLike<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let left = self.binary.left();
        let operator = self.binary.operator();
        let right = self.binary.right();

        match self.layout {
            BinaryLikeLayout::ExpandLeft => {
                let left = left.format().memoized();
                let right = right.format().memoized();
                write!(
                    f,
                    [best_fitting![
                        // Everything on a single line
                        format_args![group(&left), space(), operator, space(), right],
                        // Break the left over multiple lines, keep the right flat
                        format_args![
                            group(&left).should_expand(true),
                            space(),
                            operator,
                            space(),
                            right
                        ],
                        // The content doesn't fit, indent the content and break before the operator.
                        format_args![
                            text("("),
                            block_indent(&format_args![
                                left,
                                hard_line_break(),
                                operator,
                                space(),
                                right
                            ]),
                            text(")")
                        ]
                    ]
                    .with_mode(BestFittingMode::AllLines)]
                )
            }
            BinaryLikeLayout::ExpandRight => {
                let left_group = f.group_id("BinaryLeft");

                write!(
                    f,
                    [
                        // Wrap the left in a group and gives it an id. The printer first breaks the
                        // right side if `right` contains any line break because the printer breaks
                        // sequences of groups from right to left.
                        // Indents the left side if the group breaks.
                        group(&format_args![
                            if_group_breaks(&text("(")),
                            indent_if_group_breaks(
                                &format_args![
                                    soft_line_break(),
                                    left.format(),
                                    soft_line_break_or_space(),
                                    operator,
                                    space()
                                ],
                                left_group
                            )
                        ])
                        .with_group_id(Some(left_group)),
                        // Wrap the right in a group and indents its content but only if the left side breaks
                        group(&indent_if_group_breaks(&right.format(), left_group)),
                        // If the left side breaks, insert a hard line break to finish the indent and close the open paren.
                        if_group_breaks(&format_args![hard_line_break(), text(")")])
                            .with_group_id(Some(left_group))
                    ]
                )
            }
            BinaryLikeLayout::ExpandRightThenLeft => {
                // The formatter expands group-sequences from right to left, and expands both if
                // there isn't enough space when expanding only one of them.
                write!(
                    f,
                    [
                        group(&left.format()),
                        space(),
                        operator,
                        space(),
                        group(&right.format())
                    ]
                )
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[allow(clippy::enum_variant_names)]
enum BinaryLikeLayout {
    /// Tries to fit both the left and the right side on a single line by expanding the left side if necessary.
    ///
    ///```python
    /// [
    ///     a,
    ///     b
    /// ] & c
    ///```
    ExpandLeft,

    /// Tries to fit both the left and the right side on a single line by expanding the right side if necessary.
    /// ```python
    /// a & [
    ///     b,
    ///     c
    /// ]
    /// ```
    ExpandRight,

    /// Tries to fit both the `left` and the `right` side on a single line by
    /// * expanding the right side if necessary
    /// * expanding the left side if necessary
    /// * expanding the left and the right side if necessary
    /// ```python
    /// [
    ///     a,
    ///     b
    /// ] & [
    ///     c,
    ///     d
    /// ]
    /// ```
    ExpandRightThenLeft,
}

#[derive(Copy, Clone, Debug)]
pub(super) enum BinaryLike<'a> {
    BinaryExpression(&'a ExprBinOp),
    BooleanExpression {
        left: &'a Expr,
        operator: BoolOp,
        right: &'a Expr,
    },
}

impl BinaryLike<'_> {
    fn left(&self) -> &Expr {
        match self {
            BinaryLike::BinaryExpression(ExprBinOp { left, .. }) => left.as_ref(),
            BinaryLike::BooleanExpression { left, .. } => left,
        }
    }

    fn right(&self) -> &Expr {
        match self {
            BinaryLike::BinaryExpression(ExprBinOp { right, .. }) => right.as_ref(),
            BinaryLike::BooleanExpression { right, .. } => right,
        }
    }

    fn operator(&self) -> BinaryLikeOperator {
        match self {
            BinaryLike::BinaryExpression(ExprBinOp { op, .. }) => BinaryLikeOperator::Binary(*op),
            BinaryLike::BooleanExpression { operator, .. } => {
                BinaryLikeOperator::Boolean(*operator)
            }
        }
    }
}

#[derive(Copy, Clone)]
enum BinaryLikeOperator {
    Binary(Operator),
    Boolean(BoolOp),
}

impl Format<PyFormatContext<'_>> for BinaryLikeOperator {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self {
            BinaryLikeOperator::Binary(binary) => binary.format().fmt(f),
            BinaryLikeOperator::Boolean(operator) => operator.format().fmt(f),
        }
    }
}

pub(super) fn can_break_expr(expr: &Expr) -> bool {
    use ruff_python_ast::prelude::*;

    match expr {
        Expr::Tuple(ExprTuple {
            elts: expressions, ..
        })
        | Expr::List(ExprList {
            elts: expressions, ..
        })
        | Expr::Set(ExprSet {
            elts: expressions, ..
        })
        | Expr::Dict(ExprDict {
            values: expressions,
            ..
        }) => !expressions.is_empty(),
        Expr::Call(ExprCall { args, keywords, .. }) => !(args.is_empty() && keywords.is_empty()),
        Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::GeneratorExp(_) => true,
        Expr::UnaryOp(ExprUnaryOp { operand, .. }) => match operand.as_ref() {
            Expr::BinOp(_) => true,
            _ => can_break_expr(operand.as_ref()),
        },
        _ => false,
    }
}
