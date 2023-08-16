use ruff_formatter::write;
use ruff_python_ast::{Parameters, Ranged, StmtFunctionDef};
use ruff_python_trivia::{lines_after_ignoring_trivia, SimpleTokenKind, SimpleTokenizer};

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{Parentheses, Parenthesize};
use crate::prelude::*;
use crate::statement::suite::{clause_body, contains_only_an_ellipsis, SuiteKind};
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
            write!(f, [item.parameters.format()])?;

            if let Some(return_annotation) = item.returns.as_ref() {
                write!(f, [space(), text("->"), space()])?;

                if return_annotation.is_tuple_expr() {
                    write!(
                        f,
                        [return_annotation.format().with_options(Parentheses::Never)]
                    )?;
                } else if comments.has_trailing_comments(return_annotation.as_ref()) {
                    // Intentionally parenthesize any return annotations with trailing comments.
                    // This avoids an instability in cases like:
                    // ```python
                    // def double(
                    //     a: int
                    // ) -> (
                    //     int  # Hello
                    // ):
                    //     pass
                    // ```
                    // If we allow this to break, it will be formatted as follows:
                    // ```python
                    // def double(
                    //     a: int
                    // ) -> int:  # Hello
                    //     pass
                    // ```
                    // On subsequent formats, the `# Hello` will be interpreted as a dangling
                    // comment on a function, yielding:
                    // ```python
                    // def double(a: int) -> int:  # Hello
                    //     pass
                    // ```
                    // Ideally, we'd reach that final formatting in a single pass, but doing so
                    // requires that the parent be aware of how the child is formatted, which
                    // is challenging. As a compromise, we break those expressions to avoid an
                    // instability.
                    write!(
                        f,
                        [return_annotation.format().with_options(Parentheses::Always)]
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

        // TODO(tjkuson): determine why this is necessary.
        if contains_only_an_ellipsis(&item.body, f.context().comments()) {
            write!(
                f,
                [
                    text(":"),
                    clause_body(&item.body, trailing_definition_comments)
                ]
            )
        } else {
            write!(
                f,
                [
                    text(":"),
                    trailing_comments(trailing_definition_comments),
                    block_indent(&item.body.format().with_options(SuiteKind::Function))
                ]
            )
        }
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
