use ruff_formatter::prelude::format_with;
use ruff_formatter::{Format, FormatResult};
use ruff_python_ast::PatternMatchSequence;

use crate::builders::PyFormatterExtensions;
use crate::expression::parentheses::{empty_parenthesized, optional_parentheses, parenthesized};
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchSequence;

#[derive(Debug)]
enum SequenceType {
    Tuple,
    TupleNoParens,
    List,
}

impl FormatNodeRule<PatternMatchSequence> for FormatPatternMatchSequence {
    fn fmt_fields(&self, item: &PatternMatchSequence, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchSequence { patterns, range } = item;
        let sequence_type = match &f.context().source()[*range].chars().next() {
            Some('(') => SequenceType::Tuple,
            Some('[') => SequenceType::List,
            _ => SequenceType::TupleNoParens,
        };
        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);
        if patterns.is_empty() {
            return match sequence_type {
                SequenceType::Tuple => empty_parenthesized("(", dangling, ")").fmt(f),
                SequenceType::List => empty_parenthesized("[", dangling, "]").fmt(f),
                SequenceType::TupleNoParens => {
                    unreachable!("If empty, it should be either tuple or list")
                }
            };
        }
        let items = format_with(|f| {
            f.join_comma_separated(range.end())
                .nodes(patterns.iter())
                .finish()
        });
        match sequence_type {
            SequenceType::Tuple => parenthesized("(", &items, ")")
                .with_dangling_comments(dangling)
                .fmt(f),
            SequenceType::List => parenthesized("[", &items, "]")
                .with_dangling_comments(dangling)
                .fmt(f),
            SequenceType::TupleNoParens => optional_parentheses(&items).fmt(f),
        }
    }
}
