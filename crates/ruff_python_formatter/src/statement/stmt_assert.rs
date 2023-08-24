use ruff_formatter::{format_args, write};
use ruff_python_ast::{Constant, Expr, ExprConstant, StmtAssert};

use crate::builders::parenthesize_if_expands;
use crate::comments::{SourceComment, SuppressionKind};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::{OptionalParentheses, Parentheses, Parenthesize};
use crate::expression::{can_omit_optional_parentheses, maybe_parenthesize_expression};
use crate::prelude::*;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtAssert;

impl FormatNodeRule<StmtAssert> for FormatStmtAssert {
    fn fmt_fields(&self, item: &StmtAssert, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAssert {
            range: _,
            test,
            msg,
        } = item;

        write!(f, [token("assert"), space()])?;

        let parenthesize_test = maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks);

        if let Some(
            msg @ (Expr::FString(_)
            | Expr::Constant(ExprConstant {
                value: Constant::Str(_) | Constant::Bytes(_),
                ..
            })),
        ) = msg.as_deref()
        {
            let parenthesize_message =
                maybe_parenthesize_expression(msg, item, Parenthesize::IfBreaks);

            let comments = f.context().comments();
            let test_comments = comments.leading_dangling_trailing(test.as_ref());
            let msg_comments = comments.leading_dangling_trailing(msg);

            // TODO limit to can omit parentheses and has own parentheses
            if parenthesize_test.needs_parentheses(f.context(), &test_comments)
                == OptionalParentheses::Multiline
                && parenthesize_message.needs_parentheses(f.context(), &msg_comments)
                    == OptionalParentheses::BestFit
                && can_omit_optional_parentheses(test, f.context())
            {
                let test_group_id = f.group_id("optional_parentheses");

                let mut format_test = test.format().with_options(Parentheses::Never).memoized();
                let mut format_msg = msg.format().with_options(Parentheses::Never).memoized();

                let test_breaks = {
                    let f = &mut WithNodeLevel::new(NodeLevel::Expression(Some(test_group_id)), f);
                    format_test.inspect(f)?.will_break()
                };

                return if test_breaks || format_msg.inspect(f)?.will_break() {
                    todo!()
                } else {
                    best_fitting![
                        // ---------------------------------------------------------------------
                        // Variant 1:
                        // Try to fit both expressions without parentheses
                        format_args![
                            group(&format_test).with_group_id(Some(test_group_id)),
                            token(","),
                            space(),
                            format_msg
                        ],
                        // ---------------------------------------------------------------------
                        // Variant 2:
                        // Try to parenthesize the string, but don't parenthesize the test just yet
                        format_args![
                            group(&format_test).with_group_id(Some(test_group_id)),
                            token(","),
                            space(),
                            parenthesize_if_expands(&format_msg).should_expand(true)
                        ],
                        // ---------------------------------------------------------------------
                        // Variant 3:
                        // Try to parenthesize both test and message
                        format_args![
                            parenthesize_if_expands(&format_test)
                                .with_group_id(test_group_id)
                                .should_expand(true),
                            token(","),
                            space(),
                            parenthesize_if_expands(&format_msg).should_expand(true)
                        ],
                        // ---------------------------------------------------------------------
                        // Variant 4:
                        // If it wasn't possible to make test and message fit by now, fallback to the first variant
                        // and omit any optional parentheses.
                        format_args![
                            // Create an empty group that always fits. Necessary because the content
                            // of format_test might refer the group id.
                            format_with(|f| {
                                f.write_element(FormatElement::GroupMode {
                                    id: test_group_id,
                                    mode: PrintMode::Flat,
                                });
                                Ok(())
                            }),
                            format_test,
                            token(","),
                            space(),
                            format_msg
                        ],
                    ]
                    .with_mode(BestFittingMode::AllLines)
                    .fmt(f)
                };
            }
        }

        maybe_parenthesize_expression(test, item, Parenthesize::IfBreaks).fmt(f)?;

        if let Some(msg) = msg {
            write!(
                f,
                [
                    token(","),
                    space(),
                    maybe_parenthesize_expression(msg, item, Parenthesize::IfBreaks),
                ]
            )?;
        }

        Ok(())
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        SuppressionKind::has_skip_comment(trailing_comments, context.source())
    }
}
