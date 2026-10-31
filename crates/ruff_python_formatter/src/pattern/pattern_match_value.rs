use ruff_python_ast::PatternMatchValue;
use ruff_python_ast::flat;
use ruff_python_ast::flat::AnyNodeRef;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchValue;

impl FormatNodeRule<PatternMatchValue> for FormatPatternMatchValue {
    fn fmt_fields(
        &self,
        item: flat::View<'_, PatternMatchValue>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let value = item.value();
        value.format().with_options(Parentheses::Never).fmt(f)
    }
}

impl NeedsParentheses for flat::View<'_, PatternMatchValue> {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        self.value().needs_parentheses(parent, context)
    }
}
