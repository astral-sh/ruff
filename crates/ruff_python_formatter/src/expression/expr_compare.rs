use crate::comments::{leading_comments, Comments};
use crate::expression::binary_like::{BinaryLayout, FormatBinaryLike};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{
    write, FormatError, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions,
};
use rustpython_parser::ast::Expr;
use rustpython_parser::ast::{CmpOp, ExprCompare};

#[derive(Default)]
pub struct FormatExprCompare {
    parentheses: Option<Parentheses>,
}

impl FormatRuleWithOptions<ExprCompare, PyFormatContext<'_>> for FormatExprCompare {
    type Options = Option<Parentheses>;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatNodeRule<ExprCompare> for FormatExprCompare {
    fn fmt_fields(&self, item: &ExprCompare, f: &mut PyFormatter) -> FormatResult<()> {
        item.fmt_binary(self.parentheses, f)
    }
}

impl<'ast> FormatBinaryLike<'ast> for ExprCompare {
    type FormatOperator = FormatOwnedWithRule<CmpOp, FormatCmpOp, PyFormatContext<'ast>>;

    fn binary_layout(&self, source: &str) -> BinaryLayout {
        if self.ops.len() == 1 {
            match self.comparators.as_slice() {
                [right] => BinaryLayout::from_left_right(&self.left, right, source),
                [..] => BinaryLayout::Default,
            }
        } else {
            BinaryLayout::Default
        }
    }

    fn fmt_default(&self, f: &mut PyFormatter<'ast, '_>) -> FormatResult<()> {
        let ExprCompare {
            range: _,
            left,
            ops,
            comparators,
        } = self;

        let comments = f.context().comments().clone();

        write!(f, [group(&left.format())])?;

        assert_eq!(comparators.len(), ops.len());

        for (operator, comparator) in ops.iter().zip(comparators) {
            let leading_comparator_comments = comments.leading_comments(comparator);
            if leading_comparator_comments.is_empty() {
                write!(f, [soft_line_break_or_space()])?;
            } else {
                // Format the expressions leading comments **before** the operator
                write!(
                    f,
                    [
                        hard_line_break(),
                        leading_comments(leading_comparator_comments)
                    ]
                )?;
            }

            write!(f, [operator.format(), space(), group(&comparator.format())])?;
        }

        Ok(())
    }

    fn left(&self) -> FormatResult<&Expr> {
        Ok(self.left.as_ref())
    }

    fn right(&self) -> FormatResult<&Expr> {
        self.comparators.last().ok_or(FormatError::SyntaxError)
    }

    fn operator(&self) -> Self::FormatOperator {
        let op = *self.ops.first().unwrap();
        op.into_format()
    }
}

impl NeedsParentheses for ExprCompare {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            parentheses @ Parentheses::Optional => match self.binary_layout(source) {
                BinaryLayout::Default => parentheses,

                BinaryLayout::ExpandRight
                | BinaryLayout::ExpandLeft
                | BinaryLayout::ExpandRightThenLeft
                    if self
                        .comparators
                        .last()
                        .map_or(false, |right| comments.has_leading_comments(right)) =>
                {
                    parentheses
                }
                _ => Parentheses::Custom,
            },
            parentheses => parentheses,
        }
    }
}

#[derive(Copy, Clone)]
pub struct FormatCmpOp;

impl<'ast> AsFormat<PyFormatContext<'ast>> for CmpOp {
    type Format<'a> = FormatRefWithRule<'a, CmpOp, FormatCmpOp, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatCmpOp)
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for CmpOp {
    type Format = FormatOwnedWithRule<CmpOp, FormatCmpOp, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatCmpOp)
    }
}

impl FormatRule<CmpOp, PyFormatContext<'_>> for FormatCmpOp {
    fn fmt(&self, item: &CmpOp, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let operator = match item {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        };

        text(operator).fmt(f)
    }
}
