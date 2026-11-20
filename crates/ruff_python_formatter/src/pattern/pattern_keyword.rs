use crate::prelude::*;
use ruff_formatter::write;
use ruff_python_ast::PatternKeyword;

#[derive(Default)]
pub struct FormatPatternKeyword;

impl FormatNodeRule<ruff_python_ast::kind::PatternKeyword> for FormatPatternKeyword {
    fn fmt_fields(&self, item: PatternKeyword<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let attr = item.attr();
        let pattern = item.pattern();
        write!(f, [attr.format(), token("="), pattern.format()])
    }
}
