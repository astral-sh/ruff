use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::{MatchCase, Pattern, Ranged};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::TextRange;

use crate::comments::{leading_comments, SourceComment};
use crate::expression::parentheses::parenthesized;
use crate::prelude::*;
use crate::statement::clause::{clause_header, ClauseHeader};
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

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling_comments(item);

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::MatchCase(item),
                    dangling_item_comments,
                    &format_with(|f| {
                        write!(f, [text("case"), space()])?;

                        let leading_pattern_comments = comments.leading_comments(pattern);
                        if !leading_pattern_comments.is_empty() {
                            parenthesized(
                                "(",
                                &format_args![
                                    leading_comments(leading_pattern_comments),
                                    pattern.format()
                                ],
                                ")",
                            )
                            .fmt(f)?;
                        } else if is_match_case_pattern_parenthesized(item, pattern, f.context())? {
                            parenthesized("(", &pattern.format(), ")").fmt(f)?;
                        } else {
                            pattern.format().fmt(f)?;
                        }

                        if let Some(guard) = guard {
                            write!(f, [space(), text("if"), space(), guard.format()])?;
                        }

                        Ok(())
                    }),
                ),
                block_indent(&body.format())
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
        TextRange::new(case.range().start(), pattern.range().start()),
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
