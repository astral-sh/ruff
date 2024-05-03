use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{CmpOp, ExprCompare};

use crate::expression::binary_like::BinaryLike;
use crate::expression::has_parentheses;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;
use crate::string::AnyString;

#[derive(Default)]
pub struct FormatExprCompare;

impl FormatNodeRule<ExprCompare> for FormatExprCompare {
    #[inline]
    fn fmt_fields(&self, item: &ExprCompare, f: &mut PyFormatter) -> FormatResult<()> {
        BinaryLike::Compare(item).fmt(f)
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
        } else if let Some(string) = AnyString::from_expression(&self.left) {
            // Multiline strings are guaranteed to never fit, avoid adding unnecessary parentheses
            if !string.is_implicit_concatenated()
                && string.is_multiline(context.source())
                && !context.comments().has(string)
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
