use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::{BoolOp, ExprBoolOp, SyntaxNode};

use crate::expression::binary_like::BinaryLike;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprBoolOp;

impl FormatNodeRule<ruff_python_ast::kind::ExprBoolOp> for FormatExprBoolOp {
    #[inline]
    fn fmt_fields(&self, item: ExprBoolOp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        BinaryLike::Bool(item).fmt(f)
    }
}

impl NeedsParentheses for ExprBoolOp<'_> {
    fn needs_parentheses(
        &self,
        parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Multiline
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
