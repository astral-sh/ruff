use ruff_formatter::write;
use ruff_python_ast::{Expr, Parameters, Ranged, StmtFunctionDef};
use ruff_python_trivia::{lines_after_ignoring_trivia, SimpleTokenKind, SimpleTokenizer};

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::parentheses::{Parentheses, Parenthesize};
use crate::expression::{has_own_parentheses, maybe_parenthesize_expression};
use crate::prelude::*;
use crate::statement::suite::SuiteKind;
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatStmtFunctionDef;

impl FormatNodeRule<StmtFunctionDef> for FormatStmtFunctionDef {
    fn fmt_fields(&self, item: &StmtFunctionDef, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let dangling_comments = comments.dangling_comments(item);
        let trailing_definition_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_own_line());

        let (leading_definition_comments, trailing_definition_comments) =
            dangling_comments.split_at(trailing_definition_comments_start);

        if let Some(last_decorator) = item.decorator_list.last() {
            f.join_with(hard_line_break())
                .entries(item.decorator_list.iter().formatted())
                .finish()?;

            if leading_definition_comments.is_empty() {
                write!(f, [hard_line_break()])?;
            } else {
                // Write any leading definition comments (between last decorator and the header)
                // while maintaining the right amount of empty lines between the comment
                // and the last decorator.
                let leading_line =
                    if lines_after_ignoring_trivia(last_decorator.end(), f.context().source()) <= 1
                    {
                        hard_line_break()
                    } else {
                        empty_line()
                    };

                write!(
                    f,
                    [leading_line, leading_comments(leading_definition_comments)]
                )?;
            }
        }

        if item.is_async {
            write!(f, [text("async"), space()])?;
        }

        write!(f, [text("def"), space(), item.name.format()])?;

        if let Some(type_params) = item.type_params.as_ref() {
            write!(f, [type_params.format()])?;
        }

        let format_inner = format_with(|f: &mut PyFormatter| {
            if should_group_function_parameters(
                &item.parameters,
                item.returns.as_deref(),
                f.context(),
            ) {
                write!(f, [group(&item.parameters.format())])?;
            } else {
                write!(f, [item.parameters.format()])?;
            }

            if let Some(return_annotation) = item.returns.as_ref() {
                write!(f, [space(), text("->"), space()])?;

                if return_annotation.is_tuple_expr() {
                    write!(
                        f,
                        [return_annotation.format().with_options(Parentheses::Never)]
                    )?;
                } else {
                    write!(
                        f,
                        [maybe_parenthesize_expression(
                            return_annotation,
                            item,
                            if empty_parameters(&item.parameters, f.context().source()) {
                                // If the parameters are empty, add parentheses if the return annotation
                                // breaks at all.
                                Parenthesize::IfBreaksOrIfRequired
                            } else {
                                // Otherwise, use our normal rules for parentheses, which allows us to break
                                // like:
                                // ```python
                                // def f(
                                //     x,
                                // ) -> Tuple[
                                //     int,
                                //     int,
                                // ]:
                                //     ...
                                // ```
                                Parenthesize::IfBreaks
                            },
                        )]
                    )?;
                }
            }
            Ok(())
        });

        write!(f, [group(&format_inner)])?;

        write!(
            f,
            [
                text(":"),
                trailing_comments(trailing_definition_comments),
                block_indent(&item.body.format().with_options(SuiteKind::Function))
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _node: &StmtFunctionDef,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}

/// Returns `true` if [`Parameters`] is empty (no parameters, no comments, etc.).
fn empty_parameters(parameters: &Parameters, source: &str) -> bool {
    let mut tokenizer = SimpleTokenizer::new(source, parameters.range())
        .filter(|token| !matches!(token.kind, SimpleTokenKind::Whitespace));

    let Some(lpar) = tokenizer.next() else {
        return false;
    };
    if !matches!(lpar.kind, SimpleTokenKind::LParen) {
        return false;
    }

    let Some(rpar) = tokenizer.next() else {
        return false;
    };
    if !matches!(rpar.kind, SimpleTokenKind::RParen) {
        return false;
    }

    true
}

/// Returns `true` if the [`Parameters`] should be wrapped in their own group.
///
/// This exists to support cases like:
/// ```python
/// def double(a: int) -> (
///     int # Hello
/// ):
///     return 2*a
/// ```
///
/// In this case, we want to put the parameters in their own group, to avoid formatting
/// as follows:
/// ```python
/// def double(
///     a: int
/// ) -> int:  # Hello
///     return 2 * a
/// ```
///
/// The trailing comment on the return annotation causes a break, which causes the
/// parameters to expand, unless they're placed in their own group.
fn should_group_function_parameters(
    parameters: &Parameters,
    returns: Option<&Expr>,
    context: &PyFormatContext,
) -> bool {
    let Some(returns) = returns else {
        return false;
    };

    // If the parameters are empty, don't group, since they'll never break anyway.
    if empty_parameters(parameters, context.source()) {
        return false;
    }

    // Does the return type have any trailing comments? If so, don't group.
    if context.comments().has_trailing_own_line_comments(returns) {
        return false;
    }

    // Only omit a group for a selected set of expressions that don't have their own
    // breakpoints.
    matches!(returns, Expr::Name(_) | Expr::Attribute(_))
}
