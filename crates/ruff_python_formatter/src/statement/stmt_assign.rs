use ruff_python_ast::{Expr, StmtAssign};

use ruff_formatter::{format_args, write, FormatError};

use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::{Parentheses, Parenthesize};
use crate::expression::{has_own_parentheses, maybe_parenthesize_expression};
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtAssign;

impl FormatNodeRule<StmtAssign> for FormatStmtAssign {
    fn fmt_fields(&self, item: &StmtAssign, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAssign {
            range: _,
            targets,
            value,
        } = item;

        let (first, rest) = targets.split_first().ok_or(FormatError::syntax_error(
            "Expected at least on assignment target",
        ))?;

        write!(
            f,
            [
                first.format(),
                space(),
                text("="),
                space(),
                FormatTargets { targets: rest }
            ]
        )?;

        write!(
            f,
            [maybe_parenthesize_expression(
                value,
                item,
                Parenthesize::IfBreaks
            )]
        )
    }
}

struct FormatTargets<'a> {
    targets: &'a [Expr],
}

impl Format<PyFormatContext<'_>> for FormatTargets<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        if let Some((first, rest)) = self.targets.split_first() {
            let can_omit_parentheses = has_own_parentheses(first);

            let group_id = if can_omit_parentheses {
                Some(f.group_id("assignment_parentheses"))
            } else {
                None
            };

            let format_first = format_with(|f: &mut PyFormatter| {
                let mut f = WithNodeLevel::new(NodeLevel::Expression(group_id), f);
                if can_omit_parentheses {
                    write!(f, [first.format().with_options(Parentheses::Never)])
                } else {
                    write!(
                        f,
                        [
                            if_group_breaks(&text("(")),
                            soft_block_indent(&first.format().with_options(Parentheses::Never)),
                            if_group_breaks(&text(")"))
                        ]
                    )
                }
            });

            write!(
                f,
                [group(&format_args![
                    format_first,
                    space(),
                    text("="),
                    space(),
                    FormatTargets { targets: rest }
                ])
                .with_group_id(group_id)]
            )
        } else {
            Ok(())
        }
    }
}
