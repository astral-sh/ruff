use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{CmpOp, Expr, ExprCompare};

use crate::comments::SourceComment;
use crate::expression::binary_like::BinaryLike;
use crate::expression::expr_constant::is_multiline_string;
use crate::expression::has_parentheses;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprCompare;

impl FormatNodeRule<ExprCompare> for FormatExprCompare {
    #[inline]
    fn fmt_fields(&self, item: &ExprCompare, f: &mut PyFormatter) -> FormatResult<()> {
        BinaryLike::Compare(item).fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Node can not have dangling comments
        Ok(())
    }
}

impl NeedsParentheses for ExprCompare {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if let Expr::Constant(constant) = self.left.as_ref() {
            // Multiline strings are guaranteed to never fit, avoid adding unnecessary parentheses
            if !constant.value.is_implicit_concatenated()
                && is_multiline_string(constant, context.source())
                && !context.comments().has(self.left.as_ref())
                && self.comparators.first().is_some_and(|right| {
                    has_parentheses(right, context).is_some() && !context.comments().has(right)
                })
            {
                OptionalParentheses::Never
            } else {
                OptionalParentheses::Multiline
            }
        } else {
            OptionalParentheses::Multiline
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
    fn fmt(&self, item: &CmpOp, f: &mut PyFormatter) -> FormatResult<()> {
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

        token(operator).fmt(f)
    }
}
