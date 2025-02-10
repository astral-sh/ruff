use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{PatternMatchSingleton, Singleton};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchSingleton;

impl FormatNodeRule<PatternMatchSingleton> for FormatPatternMatchSingleton {
    fn fmt_fields(&self, item: &PatternMatchSingleton, f: &mut PyFormatter) -> FormatResult<()> {
        match item.value {
            Singleton::None => token("None").fmt(f),
            Singleton::True => token("True").fmt(f),
            Singleton::False => token("False").fmt(f),
        }
    }
}

impl NeedsParentheses for PatternMatchSingleton {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
