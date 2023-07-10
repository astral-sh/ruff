//! This module provides helper utilities to format an expression that has a left side, an operator,
//! and a right side (binary like).

use rustpython_parser::ast::{self, Expr};

use ruff_formatter::{format_args, write};

use crate::expression::parentheses::{is_expression_parenthesized, Parentheses};
use crate::prelude::*;

/// Trait to implement a binary like syntax that has a left operand, an operator, and a right operand.
pub(super) trait FormatBinaryLike<'ast> {
    /// The type implementing the formatting of the operator.
    type FormatOperator: Format<PyFormatContext<'ast>>;

    /// Formats the binary like expression to `f`.
    fn fmt_binary(
        &self,
        parentheses: Option<Parentheses>,
        f: &mut PyFormatter<'ast, '_>,
    ) -> FormatResult<()> {
        let left = self.left()?;
        let operator = self.operator();
        let right = self.right()?;

        let layout = if parentheses == Some(Parentheses::Custom) {
            self.binary_layout(f.context().contents())
        } else {
            BinaryLayout::Default
        };

        match layout {
            BinaryLayout::Default => self.fmt_default(f),
            BinaryLayout::ExpandLeft => {
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
            BinaryLayout::ExpandRight => {
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
            BinaryLayout::ExpandRightThenLeft => {
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

    /// Determines which binary layout to use.
    fn binary_layout(&self, source: &str) -> BinaryLayout {
        if let (Ok(left), Ok(right)) = (self.left(), self.right()) {
            BinaryLayout::from_left_right(left, right, source)
        } else {
            BinaryLayout::Default
        }
    }

    /// Formats the node according to the default layout.
    fn fmt_default(&self, f: &mut PyFormatter<'ast, '_>) -> FormatResult<()>;

    /// Returns the left operator
    fn left(&self) -> FormatResult<&Expr>;

    /// Returns the right operator.
    fn right(&self) -> FormatResult<&Expr>;

    /// Returns the object that formats the operator.
    fn operator(&self) -> Self::FormatOperator;
}

fn can_break_expr(expr: &Expr, source: &str) -> bool {
    let can_break = match expr {
        Expr::Tuple(ast::ExprTuple {
            elts: expressions, ..
        })
        | Expr::List(ast::ExprList {
            elts: expressions, ..
        })
        | Expr::Set(ast::ExprSet {
            elts: expressions, ..
        })
        | Expr::Dict(ast::ExprDict {
            values: expressions,
            ..
        }) => !expressions.is_empty(),
        Expr::Call(ast::ExprCall { args, keywords, .. }) => {
            !(args.is_empty() && keywords.is_empty())
        }
        Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::GeneratorExp(_) => true,
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => can_break_expr(operand.as_ref(), source),
        _ => false,
    };

    can_break || is_expression_parenthesized(expr.into(), source)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum BinaryLayout {
    /// Put each operand on their own line if either side expands
    Default,

    /// Try to expand the left to make it fit. Add parentheses if the left or right don't fit.
    ///
    ///```python
    /// [
    ///     a,
    ///     b
    /// ] & c
    ///```
    ExpandLeft,

    /// Try to expand the right to make it fix. Add parentheses if the left or right don't fit.
    ///
    /// ```python
    /// a & [
    ///     b,
    ///     c
    /// ]
    /// ```
    ExpandRight,

    /// Both the left and right side can be expanded. Try in the following order:
    /// * expand the right side
    /// * expand the left side
    /// * expand both sides
    ///
    /// to make the expression fit
    ///
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

impl BinaryLayout {
    pub(super) fn from_left_right(left: &Expr, right: &Expr, source: &str) -> BinaryLayout {
        match (can_break_expr(left, source), can_break_expr(right, source)) {
            (false, false) => BinaryLayout::Default,
            (true, false) => BinaryLayout::ExpandLeft,
            (false, true) => BinaryLayout::ExpandRight,
            (true, true) => BinaryLayout::ExpandRightThenLeft,
        }
    }
}
