use ruff_formatter::{format_args, write};
use ruff_python_ast::StmtMatch;

use crate::comments::leading_alternate_branch_comments;
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::statement::clause::{clause_header, ClauseHeader};

#[derive(Default)]
pub struct FormatStmtMatch;

impl FormatNodeRule<StmtMatch> for FormatStmtMatch {
    fn fmt_fields(&self, item: &StmtMatch, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtMatch {
            range: _,
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

        let mut cases_iter = cases.iter();
        let Some(first) = cases_iter.next() else {
            return Ok(());
        };

        // The new level is for the `case` nodes.
        let mut f = WithNodeLevel::new(NodeLevel::CompoundStatement, f);

        write!(f, [block_indent(&first.format())])?;
        let mut last_case = first;

        for case in cases_iter {
            let last_suite_in_statement = Some(case) == cases.last();
            write!(
                f,
                [block_indent(&format_args!(
                    leading_alternate_branch_comments(
                        comments.leading(case),
                        last_case.body.last(),
                    ),
                    case.format().with_options(last_suite_in_statement)
                ))]
            )?;
            last_case = case;
        }

        Ok(())
    }
}
