use ruff_formatter::{format_args, write};
use ruff_python_ast::{AnyNodeRef, StmtMatch};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::format::format_comment;
use crate::comments::{leading_alternate_branch_comments, leading_comments, trailing_comments};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::statement::clause::{ClauseHeader, clause_header};
use crate::verbatim::{FormatVerbatimStatementRange, Indentation};

#[derive(Default)]
pub struct FormatStmtMatch;

impl FormatNodeRule<StmtMatch> for FormatStmtMatch {
    fn fmt_fields(&self, item: &StmtMatch, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtMatch {
            range: _,
            node_index: _,
            subject,
            cases,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling(item);

        // There can be at most one dangling comment after the colon in a match statement.
        debug_assert!(dangling_item_comments.len() <= 1);

        clause_header(
            ClauseHeader::Match(item),
            dangling_item_comments,
            &format_args![
                token("match"),
                space(),
                maybe_parenthesize_expression(subject, item, Parenthesize::IfBreaks),
            ],
        )
        .fmt(f)?;

        if cases.is_empty() {
            return Ok(());
        }

        // The new level is for the `case` nodes.
        let mut f = WithNodeLevel::new(NodeLevel::CompoundStatement, f);

        let source = f.context().source();
        let mut case_index = 0;

        while let Some(case) = cases.get(case_index) {
            let leading_case_comments = comments.leading(case);
            let Some(format_off_index) = leading_case_comments.iter().position(|comment| {
                comment.is_unformatted()
                    && comment.line_position().is_own_line()
                    && comment.is_suppression_off_comment(source)
            }) else {
                let last_suite_in_statement = Some(case) == cases.last();
                if case_index == 0 {
                    write!(f, [block_indent(&case.format())])?;
                } else {
                    let last_case = &cases[case_index - 1];
                    write!(
                        f,
                        [block_indent(&format_args!(
                            leading_alternate_branch_comments(
                                leading_case_comments,
                                last_case.body.last(),
                            ),
                            case.format().with_options(last_suite_in_statement)
                        ))]
                    )?;
                }
                case_index += 1;
                continue;
            };

            let format_off_comment = &leading_case_comments[format_off_index];
            let mut format_on = None;

            for (index, suppressed_case) in cases.iter().enumerate().skip(case_index) {
                let leading_comments = comments.leading(suppressed_case);
                let leading_start = if index == case_index {
                    format_off_index + 1
                } else {
                    0
                };

                if let Some(comment_index) = leading_comments
                    .iter()
                    .enumerate()
                    .skip(leading_start)
                    .find_map(|(index, comment)| {
                        (comment.line_position().is_own_line()
                            && comment.is_suppression_on_comment(source))
                        .then_some(index)
                    })
                {
                    format_on = Some((index, false, comment_index));
                    break;
                }

                if let Some(comment_index) =
                    comments
                        .trailing(suppressed_case)
                        .iter()
                        .position(|comment| {
                            comment.line_position().is_own_line()
                                && comment.is_suppression_on_comment(source)
                        })
                {
                    format_on = Some((index, true, comment_index));
                    break;
                }
            }

            let (verbatim_end, next_case_index) =
                if let Some((on_case_index, is_trailing, on_index)) = format_on {
                    let on_comments = if is_trailing {
                        comments.trailing(&cases[on_case_index])
                    } else {
                        comments.leading(&cases[on_case_index])
                    };
                    let format_on_comment = &on_comments[on_index];
                    let last_suppressed_case = is_trailing
                        .then_some(on_case_index)
                        .or_else(|| on_case_index.checked_sub(1))
                        .filter(|index| *index >= case_index);

                    if let Some(last_suppressed_case) = last_suppressed_case {
                        for (index, suppressed_case) in
                            cases[case_index..=last_suppressed_case].iter().enumerate()
                        {
                            comments.mark_verbatim_node_comments_formatted(suppressed_case.into());

                            let leading_comments = comments.leading(suppressed_case);
                            let leading_start = if index == 0 { format_off_index + 1 } else { 0 };
                            for comment in &leading_comments[leading_start..] {
                                comment.mark_formatted();
                            }

                            let trailing_comments = comments.trailing(suppressed_case);
                            let trailing_end = if is_trailing && case_index + index == on_case_index
                            {
                                on_index
                            } else {
                                trailing_comments.len()
                            };
                            for comment in &trailing_comments[..trailing_end] {
                                comment.mark_formatted();
                            }
                        }
                    }

                    if is_trailing {
                        for comment in &on_comments[on_index..] {
                            comment.mark_unformatted();
                        }
                    } else {
                        let leading_start = if on_case_index == case_index {
                            format_off_index + 1
                        } else {
                            0
                        };
                        for comment in &on_comments[leading_start..on_index] {
                            comment.mark_formatted();
                        }
                    }

                    (
                        format_on_comment.start(),
                        if is_trailing {
                            on_case_index + 1
                        } else {
                            on_case_index
                        },
                    )
                } else {
                    for (index, suppressed_case) in cases[case_index..].iter().enumerate() {
                        comments.mark_verbatim_node_comments_formatted(suppressed_case.into());

                        let leading_comments = comments.leading(suppressed_case);
                        let leading_start = if index == 0 { format_off_index + 1 } else { 0 };
                        for comment in &leading_comments[leading_start..] {
                            comment.mark_formatted();
                        }

                        for comment in comments.trailing(suppressed_case) {
                            comment.mark_formatted();
                        }
                    }

                    let mut current = AnyNodeRef::from(cases.last().unwrap());
                    let end = loop {
                        if let Some(comment) = comments.trailing(current).last() {
                            break comment.end();
                        } else if let Some(child) = current.last_child_in_body() {
                            current = child;
                        } else {
                            break current.end();
                        }
                    };

                    (end, cases.len())
                };

            format_off_comment.mark_formatted();
            let indentation = Indentation::from_range(case, source);
            write!(
                f,
                [block_indent(&format_with(|f| {
                    if case_index == 0 {
                        leading_comments(&leading_case_comments[..format_off_index]).fmt(f)?;
                    } else {
                        leading_alternate_branch_comments(
                            &leading_case_comments[..format_off_index],
                            cases[case_index - 1].body.last(),
                        )
                        .fmt(f)?;
                    }

                    format_comment(format_off_comment).fmt(f)?;
                    FormatVerbatimStatementRange {
                        verbatim_range: TextRange::new(format_off_comment.end(), verbatim_end),
                        indentation,
                    }
                    .fmt(f)?;

                    if let Some((on_case_index, is_trailing, on_index)) = format_on {
                        let on_comments = if is_trailing {
                            comments.trailing(&cases[on_case_index])
                        } else {
                            comments.leading(&cases[on_case_index])
                        };
                        let following_format_off = on_comments[on_index + 1..]
                            .iter()
                            .position(|comment| {
                                comment.line_position().is_own_line()
                                    && comment.is_suppression_off_comment(source)
                            })
                            .map_or(on_comments.len(), |index| on_index + 1 + index);

                        if is_trailing {
                            trailing_comments(&on_comments[on_index..following_format_off])
                                .fmt(f)?;
                        } else {
                            leading_comments(&on_comments[on_index..following_format_off])
                                .fmt(f)?;
                        }
                    }

                    Ok(())
                }))]
            )?;

            case_index = next_case_index;
        }

        Ok(())
    }
}
