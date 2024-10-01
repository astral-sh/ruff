use crate::comments::format::{
    empty_lines_after_leading_comments, empty_lines_before_trailing_comments,
};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{Parentheses, Parenthesize};
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};
use crate::statement::stmt_class_def::FormatDecorators;
use crate::statement::suite::SuiteKind;
use ruff_formatter::write;
use ruff_python_ast::{NodeKind, StmtFunctionDef};

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

        // If the class contains leading comments, insert newlines before them.
        // For example, given:
        // ```python
        // # comment
        //
        // def func():
        //     ...
        // ```
        //
        // At the top-level in a non-stub file, reformat as:
        // ```python
        // # comment
        //
        //
        // def func():
        //     ...
        // ```
        // Note that this is only really relevant for the specific case in which there's a single
        // newline between the comment and the node, but we _require_ two newlines. If there are
        // _no_ newlines between the comment and the node, we don't insert _any_ newlines; if there
        // are more than two, then `leading_comments` will preserve the correct number of newlines.
        empty_lines_after_leading_comments(comments.leading(item)).fmt(f)?;

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
                clause_body(body, SuiteKind::Function, trailing_definition_comments),
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
        empty_lines_before_trailing_comments(comments.trailing(item), NodeKind::StmtFunctionDef)
            .fmt(f)
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
        type_params.format().fmt(f)?;
    }

    let format_inner = format_with(|f: &mut PyFormatter| {
        parameters.format().fmt(f)?;

        if let Some(return_annotation) = returns.as_deref() {
            write!(f, [space(), token("->"), space()])?;

            if return_annotation.is_tuple_expr() {
                let parentheses = if comments.has_leading(return_annotation) {
                    Parentheses::Always
                } else {
                    Parentheses::Never
                };
                return_annotation.format().with_options(parentheses).fmt(f)
            } else if comments.has_trailing(return_annotation) {
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

                return_annotation
                    .format()
                    .with_options(Parentheses::Always)
                    .fmt(f)
            } else {
                let parenthesize = if parameters.is_empty() && !comments.has(parameters.as_ref()) {
                    // If the parameters are empty, add parentheses around literal expressions
                    // (any non splitable expression) but avoid parenthesizing subscripts and
                    // other parenthesized expressions unless necessary.
                    Parenthesize::IfBreaksParenthesized
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
                };
                maybe_parenthesize_expression(return_annotation, item, parenthesize).fmt(f)
            }
        } else {
            Ok(())
        }
    });

    group(&format_inner).fmt(f)
}
