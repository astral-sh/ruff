use crate::comments::{leading_comments, Comments};
use crate::expression::binary_like::{can_break_expr, BinaryLike, FormatBinaryLike};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use ruff_formatter::{write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use rustpython_parser::ast::{BoolOp, Expr, ExprBoolOp};

#[derive(Default)]
pub struct FormatExprBoolOp {
    parentheses: Option<Parentheses>,
}

impl FormatRuleWithOptions<ExprBoolOp, PyFormatContext<'_>> for FormatExprBoolOp {
    type Options = Option<Parentheses>;
    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatNodeRule<ExprBoolOp> for FormatExprBoolOp {
    fn fmt_fields(&self, item: &ExprBoolOp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprBoolOp {
            range: _,
            op,
            values,
        } = item;

        let layout = if self.parentheses == Some(Parentheses::Custom) {
            BoolOpLayout::from_expr(item)
        } else {
            BoolOpLayout::Default
        };

        match layout {
            BoolOpLayout::Default => {
                let mut values = values.iter();
                let comments = f.context().comments().clone();

                let Some(first) = values.next() else {
                    return Ok(())
                };

                write!(f, [group(&first.format())])?;

                for value in values {
                    let leading_value_comments = comments.leading_comments(value);
                    // Format the expressions leading comments **before** the operator
                    if leading_value_comments.is_empty() {
                        write!(f, [soft_line_break_or_space()])?;
                    } else {
                        write!(
                            f,
                            [hard_line_break(), leading_comments(leading_value_comments)]
                        )?;
                    }

                    write!(f, [op.format(), space(), group(&value.format())])?;
                }

                Ok(())
            }
            BoolOpLayout::ExpandRight { left, right } => {
                FormatBinaryLike::expand_right(BinaryLike::BooleanExpression {
                    left,
                    operator: *op,
                    right,
                })
                .fmt(f)
            }
            BoolOpLayout::ExpandLeft { left, right } => {
                FormatBinaryLike::expand_left(BinaryLike::BooleanExpression {
                    left,
                    operator: *op,
                    right,
                })
                .fmt(f)
            }
            BoolOpLayout::ExpandRightThenLeft { left, right } => {
                FormatBinaryLike::expand_right_then_left(BinaryLike::BooleanExpression {
                    left,
                    operator: *op,
                    right,
                })
                .fmt(f)
            }
        }
    }
}

impl NeedsParentheses for ExprBoolOp {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => match BoolOpLayout::from_expr(self) {
                BoolOpLayout::Default => Parentheses::Optional,

                BoolOpLayout::ExpandRight { right, .. }
                | BoolOpLayout::ExpandLeft { right, .. }
                | BoolOpLayout::ExpandRightThenLeft { right, .. }
                    if comments.has_leading_comments(right) =>
                {
                    Parentheses::Optional
                }
                _ => Parentheses::Custom,
            },
            parentheses => parentheses,
        }
    }
}

#[derive(Copy, Clone)]
pub struct FormatBoolOp;

impl<'ast> AsFormat<PyFormatContext<'ast>> for BoolOp {
    type Format<'a> = FormatRefWithRule<'a, BoolOp, FormatBoolOp, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatBoolOp)
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for BoolOp {
    type Format = FormatOwnedWithRule<BoolOp, FormatBoolOp, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatBoolOp)
    }
}

impl FormatRule<BoolOp, PyFormatContext<'_>> for FormatBoolOp {
    fn fmt(&self, item: &BoolOp, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let operator = match item {
            BoolOp::And => "and",
            BoolOp::Or => "or",
        };

        text(operator).fmt(f)
    }
}

#[derive(Clone, Debug)]
enum BoolOpLayout<'a> {
    /// Put each operand on their own line if either side expands. For boolean operations with more than two
    /// operands, or boolean operations where the right side has a leading comment.
    Default,
    /// Try to expand the left to make it fit. Add parentheses if the left or right don't fit.
    ///
    ///```python
    /// [
    ///     a,
    ///     b
    /// ] and c
    ///```
    ///
    ExpandLeft { left: &'a Expr, right: &'a Expr },

    /// Try to expand the right to make it fix. Add parentheses if the left or right don't fit.
    ///
    /// ```python
    /// a and [
    ///     b,
    ///     c
    /// ]
    /// ```
    ExpandRight { left: &'a Expr, right: &'a Expr },

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
    /// ] and [
    ///     c,
    ///     d
    /// ]
    /// ```
    ExpandRightThenLeft { left: &'a Expr, right: &'a Expr },
}

impl<'a> BoolOpLayout<'a> {
    fn from_expr(expr: &'a ExprBoolOp) -> BoolOpLayout {
        match expr.values.as_slice() {
            [left, right] => match (can_break_expr(left), can_break_expr(right)) {
                (false, false) => BoolOpLayout::Default,
                (true, false) => BoolOpLayout::ExpandLeft { left, right },
                (false, true) => BoolOpLayout::ExpandRight { left, right },
                (true, true) => BoolOpLayout::ExpandRightThenLeft { left, right },
            },
            [..] => BoolOpLayout::Default,
        }
    }
}
