use crate::comments::format::empty_lines_before_trailing_comments;
use ruff_formatter::write;
use ruff_python_ast::{Parameters, StmtFunctionDef};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::Ranged;

use crate::comments::SourceComment;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{Parentheses, Parenthesize};
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};
use crate::statement::stmt_class_def::FormatDecorators;
use crate::statement::suite::SuiteKind;

#[derive(Default)]
pub struct FormatStmtFunctionDef;

impl FormatNodeRule<StmtFunctionDef> for FormatStmtFunctionDef {
    fn fmt_fields(&self, item: &StmtFunctionDef, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtFunctionDef {
            decorator_list,
            body,
            ..
        } = item;

        let comments = f.context().comments().clone();

        let dangling_comments = comments.dangling(item);
        let trailing_definition_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_own_line());

        let (leading_definition_comments, trailing_definition_comments) =
            dangling_comments.split_at(trailing_definition_comments_start);

        write!(
            f,
            [
                FormatDecorators {
                    decorators: decorator_list,
                    leading_definition_comments,
                },
                clause_header(
                    ClauseHeader::Function(item),
                    trailing_definition_comments,
                    &format_with(|f| format_function_header(f, item)),
                ),
                clause_body(body, trailing_definition_comments).with_kind(SuiteKind::Function),
            ]
        )?;

        // If the function contains trailing comments, insert newlines before them.
        // For example, given:
        // ```python
        // def func():
        //     ...
        // # comment
        // ```
        //
        // At the top-level in a non-stub file, reformat as:
        // ```python
        // def func():
        //     ...
        //
        //
        // # comment
        // ```
        empty_lines_before_trailing_comments(f, comments.trailing(item)).fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}

fn format_function_header(f: &mut PyFormatter, item: &StmtFunctionDef) -> FormatResult<()> {
    let StmtFunctionDef {
        range: _,
        is_async,
        decorator_list: _,
        name,
        type_params,
        parameters,
        returns,
        body: _,
    } = item;

    let comments = f.context().comments().clone();

    if *is_async {
        write!(f, [token("async"), space()])?;
    }

    write!(f, [token("def"), space(), name.format()])?;

    if let Some(type_params) = type_params.as_ref() {
        write!(f, [type_params.format()])?;
    }

    let format_inner = format_with(|f: &mut PyFormatter| {
        write!(f, [parameters.format()])?;

        if let Some(return_annotation) = returns.as_ref() {
            write!(f, [space(), token("->"), space()])?;

            if return_annotation.is_tuple_expr() {
                let parentheses = if comments.has_leading(return_annotation.as_ref()) {
                    Parentheses::Always
                } else {
                    Parentheses::Never
                };
                write!(f, [return_annotation.format().with_options(parentheses)])?;
            } else if comments.has_trailing(return_annotation.as_ref()) {
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
                        if empty_parameters(parameters, f.context().source()) {
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

    group(&format_inner).fmt(f)
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
