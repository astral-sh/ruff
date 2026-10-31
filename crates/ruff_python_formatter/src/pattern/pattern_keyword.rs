use crate::prelude::*;
use ruff_formatter::write;
use ruff_python_ast::PatternKeyword;
use ruff_python_ast::flat;

#[derive(Default)]
pub struct FormatPatternKeyword;

impl FormatNodeRule<PatternKeyword> for FormatPatternKeyword {
    fn fmt_fields(
        &self,
        item: flat::View<'_, PatternKeyword>,
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        let attr = item.attr();
        let pattern = item.pattern();
        write!(f, [attr.format(), token("="), pattern.format()])
    }
}
