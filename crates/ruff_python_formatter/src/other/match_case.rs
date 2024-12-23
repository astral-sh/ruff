use ruff_formatter::{format_args, write, FormatRuleWithOptions};
use ruff_python_ast::MatchCase;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::pattern::maybe_parenthesize_pattern;
use crate::prelude::*;
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

        let format_guard = guard.as_deref().map(|guard| {
            format_with(|f| {
                write!(f, [space(), token("if"), space()])?;

                maybe_parenthesize_expression(guard, item, Parenthesize::IfBreaksParenthesized)
                    .fmt(f)
            })
        });

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::MatchCase(item),
                    dangling_item_comments,
                    &format_args![
                        token("case"),
                        space(),
                        maybe_parenthesize_pattern(pattern, item),
                        format_guard
                    ],
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
