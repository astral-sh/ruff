use ruff_formatter::prelude::format_with;
use ruff_formatter::Format;
use ruff_formatter::FormatResult;
use ruff_python_ast::PatternMatchSequence;

use crate::builders::PyFormatterExtensions;
use crate::expression::parentheses::{empty_parenthesized, parenthesized};
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchSequence;

impl FormatNodeRule<PatternMatchSequence> for FormatPatternMatchSequence {
    fn fmt_fields(&self, item: &PatternMatchSequence, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchSequence { patterns, range } = item;
        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);
        if patterns.is_empty() {
            return empty_parenthesized("[", dangling, "]").fmt(f);
        }
        let items = format_with(|f| {
            f.join_comma_separated(range.end())
                .nodes(patterns.iter())
                .finish()
        });
        parenthesized("[", &items, "]")
            .with_dangling_comments(dangling)
            .fmt(f)
    }
}
