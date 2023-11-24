use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::PatternMatchInvalid;

use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchInvalid;

impl FormatNodeRule<PatternMatchInvalid> for FormatPatternMatchInvalid {
    fn fmt_fields(&self, item: &PatternMatchInvalid, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [source_text_slice(item.range)])
    }
}

impl NeedsParentheses for PatternMatchInvalid {
    fn needs_parentheses(&self, _parent: AnyNodeRef, _context: &PyFormatContext) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
