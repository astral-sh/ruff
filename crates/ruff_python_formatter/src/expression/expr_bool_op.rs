use crate::comments::{leading_comments, Comments};
use crate::expression::binary_like::{BinaryLayout, FormatBinaryLike};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use ruff_formatter::{
    write, FormatError, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions,
};
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
        item.fmt_binary(self.parentheses, f)
    }
}

impl<'ast> FormatBinaryLike<'ast> for ExprBoolOp {
    type FormatOperator = FormatOwnedWithRule<BoolOp, FormatBoolOp, PyFormatContext<'ast>>;

    fn binary_layout(&self, source: &str) -> BinaryLayout {
        match self.values.as_slice() {
            [left, right] => BinaryLayout::from_left_right(left, right, source),
            [..] => BinaryLayout::Default,
        }
    }

    fn fmt_default(&self, f: &mut PyFormatter<'ast, '_>) -> FormatResult<()> {
        let ExprBoolOp {
            range: _,
            op,
            values,
        } = self;

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

    fn left(&self) -> FormatResult<&Expr> {
        self.values.first().ok_or(FormatError::SyntaxError)
    }

    fn right(&self) -> FormatResult<&Expr> {
        self.values.last().ok_or(FormatError::SyntaxError)
    }

    fn operator(&self) -> Self::FormatOperator {
        self.op.into_format()
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
            Parentheses::Optional => match self.binary_layout(source) {
                BinaryLayout::Default => Parentheses::Optional,

                BinaryLayout::ExpandRight
                | BinaryLayout::ExpandLeft
                | BinaryLayout::ExpandRightThenLeft
                    if self
                        .values
                        .last()
                        .map_or(false, |right| comments.has_leading_comments(right)) =>
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
