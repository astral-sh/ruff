use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{CmpOp, ExprCompare};

use crate::comments::SourceComment;
use crate::expression::binary_like::BinaryLike;
use crate::expression::expr_string_literal::is_multiline_string;
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
        dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Node can not have dangling comments
        debug_assert!(dangling_comments.is_empty());
        Ok(())
    }
}

impl NeedsParentheses for ExprCompare {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else if let Some(literal_expr) = self.left.as_literal_expr() {
            // Multiline strings are guaranteed to never fit, avoid adding unnecessary parentheses
            if !literal_expr.is_implicit_concatenated()
                && is_multiline_string(literal_expr.into(), context.source())
                && !context.comments().has(literal_expr)
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
