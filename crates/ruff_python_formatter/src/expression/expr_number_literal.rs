use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{ExprNumberLiteral, Number};

use crate::comments::SourceComment;
use crate::expression::number::{FormatComplex, FormatFloat, FormatInt};
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprNumberLiteral;

impl FormatNodeRule<ExprNumberLiteral> for FormatExprNumberLiteral {
    fn fmt_fields(&self, item: &ExprNumberLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        match item.value {
            Number::Int(_) => FormatInt::new(item).fmt(f),
            Number::Float(_) => FormatFloat::new(item).fmt(f),
            Number::Complex { .. } => FormatComplex::new(item).fmt(f),
        }
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        Ok(())
    }
}

impl NeedsParentheses for ExprNumberLiteral {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
