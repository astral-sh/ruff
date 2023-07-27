use crate::comments::leading_comments;
use crate::expression::parentheses::{
    in_parentheses_only_group, in_parentheses_only_soft_line_break_or_space, NeedsParentheses,
    OptionalParentheses, Parentheses,
};
use crate::prelude::*;
use ruff_formatter::{write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{BoolOp, ExprBoolOp};

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

        let inner = format_with(|f: &mut PyFormatter| {
            let mut values = values.iter();
            let comments = f.context().comments().clone();

            let Some(first) = values.next() else {
                return Ok(());
            };

            write!(f, [in_parentheses_only_group(&first.format())])?;

            for value in values {
                let leading_value_comments = comments.leading_comments(value);
                // Format the expressions leading comments **before** the operator
                if leading_value_comments.is_empty() {
                    write!(f, [in_parentheses_only_soft_line_break_or_space()])?;
                } else {
                    write!(
                        f,
                        [hard_line_break(), leading_comments(leading_value_comments)]
                    )?;
                }

                write!(
                    f,
                    [
                        op.format(),
                        space(),
                        in_parentheses_only_group(&value.format())
                    ]
                )?;
            }

            Ok(())
        });

        in_parentheses_only_group(&inner).fmt(f)
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
