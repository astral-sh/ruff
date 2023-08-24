use ruff_formatter::{write, FormatResult};
use ruff_python_ast::PatternMatchClass;

use crate::expression::parentheses::parenthesized;
use crate::prelude::*;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchClass;

impl FormatNodeRule<PatternMatchClass> for FormatPatternMatchClass {
    fn fmt_fields(&self, item: &PatternMatchClass, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchClass {
            range,
            cls,
            patterns,
            kwd_attrs,
            kwd_patterns,
        } = item;

        let items = format_with(|f| {
            let mut join = f.join_comma_separated(range.end());

            if !patterns.is_empty() {
                join.nodes(patterns.iter());
            }

            if !kwd_attrs.is_empty() {
                for (key, value) in kwd_attrs.iter().zip(kwd_patterns.iter()) {
                    join.entry(
                        key,
                        &format_with(|f| write!(f, [key.format(), text("="), value.format()])),
                    );
                }
            }
            join.finish()
        });

        write!(f, [cls.format(), parenthesized("(", &items, ")")])
    }
}
