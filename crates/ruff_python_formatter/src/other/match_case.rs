use ruff_formatter::{FormatRuleWithOptions, format_args, write};
use ruff_python_ast::MatchCase;
use ruff_python_ast::flat;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::pattern::maybe_parenthesize_pattern;
use crate::prelude::*;
use crate::statement::clause::{ClauseHeader, clause};
use crate::statement::suite::SuiteKind;

#[derive(Default)]
pub struct FormatMatchCase {
    last_suite_in_statement: bool,
}

impl FormatRuleWithOptions<flat::View<'_, MatchCase>, PyFormatContext<'_>> for FormatMatchCase {
    type Options = bool;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.last_suite_in_statement = options;
        self
    }
}

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, item: flat::View<'_, MatchCase>, f: &mut PyFormatter) -> FormatResult<()> {
        let pattern = item.pattern();
        let guard = item.guard();
        let body = item.body();

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling(item);

        let format_guard = guard.map(|guard| {
            format_with(move |f| {
                write!(f, [space(), token("if"), space()])?;

                maybe_parenthesize_expression(guard, item, Parenthesize::IfBreaksParenthesized)
                    .fmt(f)
            })
        });

        write!(
            f,
            [clause(
                ClauseHeader::MatchCase(item),
                &format_args![
                    token("case"),
                    space(),
                    maybe_parenthesize_pattern(pattern, item),
                    format_guard
                ],
                dangling_item_comments,
                body,
                SuiteKind::other(self.last_suite_in_statement),
            )]
        )
    }
}
