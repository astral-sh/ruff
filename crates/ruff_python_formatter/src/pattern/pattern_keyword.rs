use crate::prelude::*;
use ruff_formatter::write;
use ruff_python_ast::PatternKeyword;

#[derive(Default)]
pub struct FormatPatternKeyword;

impl FormatNodeRule<PatternKeyword> for FormatPatternKeyword {
    fn fmt_fields(&self, item: &PatternKeyword, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternKeyword {
            range: _,
            attr,
            pattern,
        } = item;
        write!(f, [attr.format(), token("="), pattern.format()])
    }
}
