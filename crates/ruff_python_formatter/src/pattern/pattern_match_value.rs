use ruff_python_ast::{PatternMatchValue, SyntaxNode};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchValue;

impl FormatNodeRule<ruff_python_ast::kind::PatternMatchValue> for FormatPatternMatchValue {
    fn fmt_fields(&self, item: PatternMatchValue<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let value = item.value();
        value.format().with_options(Parentheses::Never).fmt(f)
    }
}

impl NeedsParentheses for PatternMatchValue<'_> {
    fn needs_parentheses(
        &self,
        parent: SyntaxNode,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        self.value().needs_parentheses(parent, context)
    }
}
