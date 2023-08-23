use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::{MatchCase, Pattern, Ranged};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::TextRange;

use crate::comments::SourceComment;
use crate::expression::parentheses::parenthesized;
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};
use crate::{FormatError, FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatMatchCase;

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, item: &MatchCase, f: &mut PyFormatter) -> FormatResult<()> {
        let MatchCase {
            range: _,
            pattern,
            guard,
            body,
        } = item;

        // Distinguish dangling comments that appear on the open parenthesis from those that
        // appear on the trailing colon.
        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling(item);
        let (open_parenthesis_comments, trailing_colon_comments) = dangling_item_comments.split_at(
            dangling_item_comments.partition_point(|comment| comment.start() < pattern.start()),
        );

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::MatchCase(item),
                    dangling_item_comments,
                    &format_with(|f| {
                        write!(f, [text("case"), space()])?;

                        if is_match_case_pattern_parenthesized(item, pattern, f.context())? {
                            parenthesized("(", &pattern.format(), ")")
                                .with_dangling_comments(open_parenthesis_comments)
                                .fmt(f)?;
                        } else {
                            pattern.format().fmt(f)?;
                        }

                        if let Some(guard) = guard {
                            write!(f, [space(), text("if"), space(), guard.format()])?;
                        }

                        Ok(())
                    }),
                ),
                clause_body(body, trailing_colon_comments),
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}

fn is_match_case_pattern_parenthesized(
    case: &MatchCase,
    pattern: &Pattern,
    context: &PyFormatContext,
) -> FormatResult<bool> {
    let mut tokenizer = SimpleTokenizer::new(
        context.source(),
        TextRange::new(case.start(), pattern.start()),
    )
    .skip_trivia();

    let case_keyword = tokenizer.next().ok_or(FormatError::syntax_error(
        "Expected a `case` keyword, didn't find any token",
    ))?;

    debug_assert_eq!(
        case_keyword.kind(),
        SimpleTokenKind::Case,
        "Expected `case` keyword but at {case_keyword:?}"
    );

    match tokenizer.next() {
        Some(left_paren) => {
            debug_assert_eq!(left_paren.kind(), SimpleTokenKind::LParen);
            Ok(true)
        }
        None => Ok(false),
    }
}
