use ruff_python_ast::flat::AnyNodeRef;
use ruff_python_ast::{PatternMatchSingleton, Singleton, flat};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchSingleton;

impl FormatNodeRule<PatternMatchSingleton> for FormatPatternMatchSingleton {
    fn fmt_fields(
        &self,
        item: flat::View<'_, PatternMatchSingleton>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        match item.value() {
            Singleton::None => token("None").fmt(f),
            Singleton::True => token("True").fmt(f),
            Singleton::False => token("False").fmt(f),
        }
    }
}

impl NeedsParentheses for flat::View<'_, PatternMatchSingleton> {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::BestFit
    }
}
