use crate::comments::leading_comments;
use crate::expression::parentheses::{
    in_parentheses_only_group, in_parentheses_only_soft_line_break_or_space, NeedsParentheses,
    OptionalParentheses, Parentheses,
};
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{CmpOp, ExprCompare};

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
        let ExprCompare {
            range: _,
            left,
            ops,
            comparators,
        } = item;

        let comments = f.context().comments().clone();

        let inner = format_with(|f| {
            write!(f, [in_parentheses_only_group(&left.format())])?;

            assert_eq!(comparators.len(), ops.len());

            for (operator, comparator) in ops.iter().zip(comparators) {
                let leading_comparator_comments = comments.leading_comments(comparator);
                if leading_comparator_comments.is_empty() {
                    write!(f, [in_parentheses_only_soft_line_break_or_space()])?;
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

                write!(
                    f,
                    [
                        operator.format(),
                        space(),
                        in_parentheses_only_group(&comparator.format())
                    ]
                )?;
            }

            Ok(())
        });

        in_parentheses_only_group(&inner).fmt(f)
    }
}

impl NeedsParentheses for ExprCompare {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
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
