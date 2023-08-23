use ruff_python_ast::PatternMatchValue;

use crate::expression::parentheses::Parentheses;
use crate::prelude::*;
use crate::{AsFormat, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchValue;

impl FormatNodeRule<PatternMatchValue> for FormatPatternMatchValue {
    fn fmt_fields(&self, item: &PatternMatchValue, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchValue { value, range: _ } = item;
        value.format().with_options(Parentheses::Never).fmt(f)
    }
}
