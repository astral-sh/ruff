use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::PatternMatchValue;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses};
use crate::prelude::*;
use crate::preview::is_match_case_parentheses_enabled;

#[derive(Default)]
pub struct FormatPatternMatchValue;

impl FormatNodeRule<PatternMatchValue> for FormatPatternMatchValue {
    fn fmt_fields(&self, item: &PatternMatchValue, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchValue { value, range: _ } = item;
        value.format().with_options(Parentheses::Never).fmt(f)
    }
}

impl NeedsParentheses for PatternMatchValue {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if is_match_case_parentheses_enabled(context) {
            self.value.needs_parentheses(parent, context)
        } else {
            OptionalParentheses::Never
        }
    }
}
