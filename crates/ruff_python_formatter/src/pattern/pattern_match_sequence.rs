use ruff_formatter::{format_args, Format, FormatResult};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::PatternMatchSequence;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::expression::parentheses::{
    empty_parenthesized, optional_parentheses, parenthesized, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatPatternMatchSequence;

impl FormatNodeRule<PatternMatchSequence> for FormatPatternMatchSequence {
    fn fmt_fields(&self, item: &PatternMatchSequence, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchSequence { patterns, range } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        let sequence_type = SequenceType::from_pattern(item, f.context().source());

        match (patterns.as_slice(), sequence_type) {
            // If the sequence is empty, format the empty parentheses, along with any dangling
            // comments.
            ([], SequenceType::Tuple | SequenceType::TupleNoParens) => {
                return empty_parenthesized("(", dangling, ")").fmt(f)
            }
            ([], SequenceType::List) => return empty_parenthesized("[", dangling, "]").fmt(f),

            // A single-element tuple should always be parenthesized, and the trailing comma
            // should never cause it to expand.
            ([elt], SequenceType::Tuple | SequenceType::TupleNoParens) => {
                return parenthesized("(", &format_args![elt.format(), token(",")], ")")
                    .with_dangling_comments(dangling)
                    .fmt(f)
            }

            _ => {}
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

impl NeedsParentheses for PatternMatchSequence {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum SequenceType {
    /// A list literal, e.g., `[1, 2, 3]`.
    List,
    /// A parenthesized tuple literal, e.g., `(1, 2, 3)`.
    Tuple,
    /// A tuple literal without parentheses, e.g., `1, 2, 3`.
    TupleNoParens,
}

impl SequenceType {
    pub(crate) fn from_pattern(pattern: &PatternMatchSequence, source: &str) -> SequenceType {
        if source[pattern.range()].starts_with('[') {
            SequenceType::List
        } else if source[pattern.range()].starts_with('(') {
            // If the pattern is empty, it must be a parenthesized tuple with no members. (This
            // branch exists to differentiate between a tuple with and without its own parentheses,
            // but a tuple without its own parentheses must have at least one member.)
            let Some(elt) = pattern.patterns.first() else {
                return SequenceType::Tuple;
            };

            // Count the number of open parentheses between the start of the pattern and the first
            // element, and the number of close parentheses between the first element and its
            // trailing comma. If the number of open parentheses is greater than the number of close
            // parentheses,
            // the pattern is parenthesized. For example, here, we have two parentheses before the
            // first element, and one after it:
            // ```python
            // ((a), b, c)
            // ```
            //
            // This algorithm successfully avoids false positives for cases like:
            // ```python
            // (a), b, c
            // ```
            let open_parentheses_count =
                SimpleTokenizer::new(source, TextRange::new(pattern.start(), elt.start()))
                    .skip_trivia()
                    .filter(|token| token.kind() == SimpleTokenKind::LParen)
                    .count();

            // Count the number of close parentheses.
            let close_parentheses_count =
                SimpleTokenizer::new(source, TextRange::new(elt.end(), elt.end()))
                    .skip_trivia()
                    .take_while(|token| token.kind() != SimpleTokenKind::Comma)
                    .filter(|token| token.kind() == SimpleTokenKind::RParen)
                    .count();

            if open_parentheses_count > close_parentheses_count {
                SequenceType::Tuple
            } else {
                SequenceType::TupleNoParens
            }
        } else {
            SequenceType::TupleNoParens
        }
    }

    pub(crate) fn is_parenthesized(self) -> bool {
        matches!(self, SequenceType::List | SequenceType::Tuple)
    }
}
