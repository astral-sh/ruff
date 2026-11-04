use ruff_formatter::{FormatRuleWithOptions, format_args, write};
use ruff_python_ast::{AnyNodeRef, ElifElseClause, StmtIf};
use ruff_text_size::Ranged;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::preview::is_pyi_overload_group_blank_lines_enabled;
use crate::statement::clause::{ClauseHeader, clause};
use crate::statement::stub;
use crate::statement::suite::SuiteKind;

#[derive(Default)]
pub struct FormatStmtIf {
    stub_overload_groups: Vec<bool>,
}

impl FormatRuleWithOptions<StmtIf, PyFormatContext<'_>> for FormatStmtIf {
    type Options = Vec<bool>;

    fn with_options(mut self, stub_overload_groups: Vec<bool>) -> Self {
        self.stub_overload_groups = stub_overload_groups;
        self
    }
}

impl FormatNodeRule<StmtIf> for FormatStmtIf {
    fn fmt_fields(&self, item: &StmtIf, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtIf {
            range: _,
            node_index: _,
            test,
            body,
            elif_else_clauses,
        } = item;

        let comments = f.context().comments().clone();
        let trailing_colon_comment = comments.dangling(item);
        let default_groups;
        let groups = if f.options().source_type().is_stub()
            && is_pyi_overload_group_blank_lines_enabled(f.context())
        {
            if self.stub_overload_groups.is_empty() {
                default_groups = stub::conditional_groups(item, None, None, f.context());
                default_groups.as_slice()
            } else {
                &self.stub_overload_groups
            }
        } else {
            &[]
        };

        write!(
            f,
            [clause(
                ClauseHeader::If(item),
                &format_args![
                    token("if"),
                    space(),
                    maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks),
                ],
                trailing_colon_comment,
                body,
                SuiteKind::other(elif_else_clauses.is_empty()).with_stub_overload_groups(
                    groups.first().copied().unwrap_or(false),
                    groups.get(1).copied().unwrap_or(false),
                ),
            )]
        )?;

        let mut last_node = body.last().unwrap().into();
        for (index, clause) in elif_else_clauses.iter().enumerate() {
            format_elif_else_clause(
                clause,
                f,
                (!groups.get(index + 1).copied().unwrap_or(false)).then_some(last_node),
                SuiteKind::other(clause == elif_else_clauses.last().unwrap())
                    .with_stub_overload_groups(
                        groups.get(index + 1).copied().unwrap_or(false),
                        groups.get(index + 2).copied().unwrap_or(false),
                    ),
            )?;
            last_node = clause.body.last().unwrap().into();
        }

        Ok(())
    }
}

/// Extracted so we can implement `FormatElifElseClause` but also pass in `last_node` from
/// `FormatStmtIf`
pub(crate) fn format_elif_else_clause(
    item: &ElifElseClause,
    f: &mut PyFormatter,
    last_node: Option<AnyNodeRef>,
    suite_kind: SuiteKind,
) -> FormatResult<()> {
    let ElifElseClause {
        range: _,
        node_index: _,
        test,
        body,
    } = item;

    let comments = f.context().comments().clone();
    let trailing_colon_comment = comments.dangling(item);
    let leading_comments = comments.leading(item);

    write!(
        f,
        [
            clause(
                ClauseHeader::ElifElse(item),
                &format_with(|f: &mut PyFormatter| {
                    f.options()
                        .source_map_generation()
                        .is_enabled()
                        .then_some(source_position(item.start()))
                        .fmt(f)?;
                    if let Some(test) = test {
                        write!(
                            f,
                            [
                                token("elif"),
                                space(),
                                maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks),
                            ]
                        )
                    } else {
                        token("else").fmt(f)
                    }
                }),
                trailing_colon_comment,
                body,
                suite_kind,
            )
            .with_leading_comments(leading_comments, last_node),
            f.options()
                .source_map_generation()
                .is_enabled()
                .then_some(source_position(item.end()))
        ]
    )
}
