use ruff_formatter::{write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{BoolOp, Expr, ExprBoolOp};

use crate::comments::leading_comments;
use crate::expression::parentheses::{
    in_parentheses_only_group, in_parentheses_only_soft_line_break_or_space, NeedsParentheses,
    OptionalParentheses,
};
use crate::prelude::*;

use super::parentheses::is_expression_parenthesized;

#[derive(Default)]
pub struct FormatExprBoolOp {
    layout: BoolOpLayout,
}

#[derive(Default, Copy, Clone)]
pub enum BoolOpLayout {
    #[default]
    Default,
    Chained,
}

impl FormatRuleWithOptions<ExprBoolOp, PyFormatContext<'_>> for FormatExprBoolOp {
    type Options = BoolOpLayout;
    fn with_options(mut self, options: Self::Options) -> Self {
        self.layout = options;
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

        let inner = format_with(|f: &mut PyFormatter| {
            let mut values = values.iter();
            let comments = f.context().comments().clone();

            let Some(first) = values.next() else {
                return Ok(());
            };

            FormatValue { value: first }.fmt(f)?;

            for value in values {
                let leading_value_comments = comments.leading(value);
                // Format the expressions leading comments **before** the operator
                if leading_value_comments.is_empty() {
                    write!(f, [in_parentheses_only_soft_line_break_or_space()])?;
                } else {
                    write!(
                        f,
                        [hard_line_break(), leading_comments(leading_value_comments)]
                    )?;
                }

                write!(f, [op.format(), space()])?;

                FormatValue { value }.fmt(f)?;
            }

            Ok(())
        });

        if matches!(self.layout, BoolOpLayout::Chained) {
            // Chained boolean operations should not be given a new group
            inner.fmt(f)
        } else {
            in_parentheses_only_group(&inner).fmt(f)
        }
    }
}

impl NeedsParentheses for ExprBoolOp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}

struct FormatValue<'a> {
    value: &'a Expr,
}

impl Format<PyFormatContext<'_>> for FormatValue<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self.value {
            Expr::BoolOp(bool_op)
                if !is_expression_parenthesized(bool_op.into(), f.context().source()) =>
            {
                // Mark chained boolean operations e.g. `x and y or z` and avoid creating a new group
                write!(f, [bool_op.format().with_options(BoolOpLayout::Chained)])
            }
            _ => write!(f, [in_parentheses_only_group(&self.value.format())]),
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
    fn fmt(&self, item: &BoolOp, f: &mut PyFormatter) -> FormatResult<()> {
        let operator = match item {
            BoolOp::And => "and",
            BoolOp::Or => "or",
        };

        token(operator).fmt(f)
    }
}
