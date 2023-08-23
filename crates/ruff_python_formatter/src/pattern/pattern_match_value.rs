use ruff_python_ast::PatternMatchValue;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchValue;

impl FormatNodeRule<PatternMatchValue> for FormatPatternMatchValue {
    fn fmt_fields(&self, item: &PatternMatchValue, f: &mut PyFormatter) -> FormatResult<()> {
        // TODO(charlie): Avoid double parentheses for parenthesized top-level `PatternMatchValue`.
        let PatternMatchValue { value, range: _ } = item;
        value.format().fmt(f)
    }
}
