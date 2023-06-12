use crate::{not_yet_implemented, FormatNodeRule, PyFormatter};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::MatchCase;

#[derive(Default)]
pub struct FormatMatchCase;

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, item: &MatchCase, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [not_yet_implemented(item)])
    }
}
