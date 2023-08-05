use crate::builders::PyFormatterExtensions;
use crate::comments::trailing_comments;
use crate::expression::parentheses::parenthesized;
use crate::prelude::*;
use ruff_formatter::write;
use ruff_formatter::FormatResult;
use ruff_python_ast::node::AstNode;

use ruff_python_ast::TypeParams;

#[derive(Default)]
pub struct FormatTypeParams;

/// Formats a sequence of [`TypeParam`] nodes.
impl FormatNodeRule<TypeParams> for FormatTypeParams {
    fn fmt_fields(&self, item: &TypeParams, f: &mut PyFormatter) -> FormatResult<()> {
        // A dangling comment indicates a comment on the same line as the opening bracket, e.g.:
        // ```python
        // type foo[  # This type parameter clause has a dangling comment.
        //     a,
        //     b,
        //     c,
        // ] = ...
        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(item.as_any_node_ref());
        write!(f, [trailing_comments(dangling_comments)])?;

        let items = format_with(|f| {
            f.join_comma_separated(item.range.end())
                .nodes(item.type_params.iter())
                .finish()
        });

        parenthesized("[", &items, "]").fmt(f)
    }

    fn fmt_dangling_comments(&self, _node: &TypeParams, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
