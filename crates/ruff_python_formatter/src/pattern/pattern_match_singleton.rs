use ruff_python_ast::{PatternMatchSingleton, Singleton, SyntaxNode};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchSingleton;

impl FormatNodeRule<ruff_python_ast::kind::PatternMatchSingleton> for FormatPatternMatchSingleton {
    fn fmt_fields(&self, item: PatternMatchSingleton<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        match item.value() {
            Singleton::None => token("None").fmt(f),
            Singleton::True => token("True").fmt(f),
            Singleton::False => token("False").fmt(f),
        }
    }
}

impl NeedsParentheses for PatternMatchSingleton<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
