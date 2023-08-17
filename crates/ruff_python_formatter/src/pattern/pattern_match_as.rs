use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchAs;

use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchAs;

impl FormatNodeRule<PatternMatchAs> for FormatPatternMatchAs {
    fn fmt_fields(&self, item: &PatternMatchAs, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchAs {
            range: _,
            pattern,
            name,
        } = item;

        if let Some(name) = name {
            if let Some(pattern) = pattern {
                write!(f, [pattern.format(), space(), text("as"), space()])?;
            }
            name.format().fmt(f)
        } else {
            debug_assert!(pattern.is_none());
            text("_").fmt(f)
        }
    }
}
