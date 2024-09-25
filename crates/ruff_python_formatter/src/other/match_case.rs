use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::AstNode;
use ruff_python_ast::MatchCase;

use crate::builders::parenthesize_if_expands;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, Parentheses, Parenthesize,
};
use crate::pattern::maybe_parenthesize_pattern;
use crate::prelude::*;
use crate::preview::is_match_case_parentheses_enabled;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};
use crate::statement::suite::SuiteKind;

#[derive(Default)]
pub struct FormatMatchCase {
    last_suite_in_statement: bool,
}

impl FormatRuleWithOptions<MatchCase, PyFormatContext<'_>> for FormatMatchCase {
    type Options = bool;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.last_suite_in_statement = options;
        self
    }
}

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, item: &MatchCase, f: &mut PyFormatter) -> FormatResult<()> {
        let MatchCase {
            range: _,
            pattern,
            guard,
            body,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling(item);

        let format_pattern = format_with(|f| {
            if is_match_case_parentheses_enabled(f.context()) {
                maybe_parenthesize_pattern(pattern, item).fmt(f)
            } else {
                let has_comments =
                    comments.has_leading(pattern) || comments.has_trailing_own_line(pattern);

                if has_comments {
                    pattern.format().with_options(Parentheses::Always).fmt(f)
                } else {
                    match pattern.needs_parentheses(item.as_any_node_ref(), f.context()) {
                        OptionalParentheses::Multiline => parenthesize_if_expands(
                            &pattern.format().with_options(Parentheses::Never),
                        )
                        .fmt(f),
                        OptionalParentheses::Always => {
                            pattern.format().with_options(Parentheses::Always).fmt(f)
                        }
                        OptionalParentheses::Never | OptionalParentheses::BestFit => {
                            pattern.format().with_options(Parentheses::Never).fmt(f)
                        }
                    }
                }
            }
        });

        let format_guard = guard.as_deref().map(|guard| {
            format_with(|f| {
                write!(f, [space(), token("if"), space()])?;

                if is_match_case_parentheses_enabled(f.context()) {
                    maybe_parenthesize_expression(guard, item, Parenthesize::IfBreaksParenthesized)
                        .fmt(f)
                } else {
                    guard.format().fmt(f)
                }
            })
        });

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::MatchCase(item),
                    dangling_item_comments,
                    &format_with(|f| {
                        write!(f, [token("case"), space(), format_pattern, format_guard])
                    }),
                ),
                clause_body(
                    body,
                    SuiteKind::other(self.last_suite_in_statement),
                    dangling_item_comments
                ),
            ]
        )
    }
}
