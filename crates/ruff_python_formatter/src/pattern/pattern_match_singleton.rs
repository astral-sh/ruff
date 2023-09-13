use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Constant, PatternMatchSingleton};

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchSingleton;

impl FormatNodeRule<PatternMatchSingleton> for FormatPatternMatchSingleton {
    fn fmt_fields(&self, item: &PatternMatchSingleton, f: &mut PyFormatter) -> FormatResult<()> {
        match item.value {
            Constant::None => token("None").fmt(f),
            Constant::Bool(true) => token("True").fmt(f),
            Constant::Bool(false) => token("False").fmt(f),
            _ => unreachable!(),
        }
    }
}

impl NeedsParentheses for PatternMatchSingleton {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
