use ruff_text_size::TextRange;
use rustpython_parser::ast::ExprTuple;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_formatter::{format_args, write, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;

use crate::builders::parenthesize_if_expands;
use crate::comments::{dangling_comments, CommentLinePosition};
use crate::expression::parentheses::{
    parenthesized, NeedsParentheses, OptionalParentheses, Parentheses,
};
use crate::prelude::*;

#[derive(Eq, PartialEq, Debug, Default)]
pub enum TupleParentheses {
    /// Black omits parentheses for tuples inside of comprehensions.
    Comprehension,

    /// Effectively `None` in `Option<Parentheses>`
    #[default]
    Default,
    /// Effectively `Some(Parentheses)` in `Option<Parentheses>`
    Expr(Parentheses),

    /// Black omits parentheses for tuples inside of subscripts except if the tuple is parenthesized
    /// in the source code.
    Subscript,

    /// Handle the special case where we remove parentheses even if they were initially present
    ///
    /// Normally, black keeps parentheses, but in the case of loops it formats
    /// ```python
    /// for (a, b) in x:
    ///     pass
    /// ```
    /// to
    /// ```python
    /// for a, b in x:
    ///     pass
    /// ```
    /// Black still does use parentheses in this position if the group breaks or magic trailing
    /// comma is used.
    StripInsideForLoop,
}

#[derive(Default)]
pub struct FormatExprTuple {
    parentheses: TupleParentheses,
}

impl FormatRuleWithOptions<ExprTuple, PyFormatContext<'_>> for FormatExprTuple {
    type Options = TupleParentheses;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatNodeRule<ExprTuple> for FormatExprTuple {
    fn fmt_fields(&self, item: &ExprTuple, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprTuple {
            range,
            elts,
            ctx: _,
        } = item;

        // Handle the edge cases of an empty tuple and a tuple with one element
        //
        // there can be dangling comments, and they can be in two
        // positions:
        // ```python
        // a3 = (  # end-of-line
        //     # own line
        // )
        // ```
        // In all other cases comments get assigned to a list element
        match elts.as_slice() {
            [] => {
                let comments = f.context().comments().clone();
                let dangling = comments.dangling_comments(item);
                let end_of_line_split = dangling.partition_point(|comment| {
                    comment.line_position() == CommentLinePosition::EndOfLine
                });
                debug_assert!(dangling[end_of_line_split..]
                    .iter()
                    .all(|comment| comment.line_position() == CommentLinePosition::OwnLine));
                write!(
                    f,
                    [group(&format_args![
                        text("("),
                        dangling_comments(&dangling[..end_of_line_split]),
                        soft_block_indent(&dangling_comments(&dangling[end_of_line_split..])),
                        text(")")
                    ])]
                )
            }
            [single] => match self.parentheses {
                TupleParentheses::Subscript
                    if !is_parenthesized(*range, elts, f.context().source()) =>
                {
                    write!(f, [single.format(), text(",")])
                }
                _ =>
                // A single element tuple always needs parentheses and a trailing comma, except when inside of a subscript
                {
                    parenthesized("(", &format_args![single.format(), text(",")], ")").fmt(f)
                }
            },
            // If the tuple has parentheses, we generally want to keep them. The exception are for
            // loops, see `TupleParentheses::StripInsideForLoop` doc comment.
            //
            // Unlike other expression parentheses, tuple parentheses are part of the range of the
            // tuple itself.
            _ if is_parenthesized(*range, elts, f.context().source())
                && self.parentheses != TupleParentheses::StripInsideForLoop =>
            {
                parenthesized("(", &ExprSequence::new(item), ")").fmt(f)
            }
            _ => match self.parentheses {
                TupleParentheses::Comprehension => {
                    let separator =
                        format_with(|f| group(&format_args![text(","), space()]).fmt(f));
                    f.join_with(separator)
                        .entries(elts.iter().formatted())
                        .finish()
                }
                TupleParentheses::Subscript => group(&ExprSequence::new(item)).fmt(f),
                _ => parenthesize_if_expands(&ExprSequence::new(item)).fmt(f),
            },
        }
    }

    fn fmt_dangling_comments(&self, _node: &ExprTuple, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}

#[derive(Debug)]
struct ExprSequence<'a> {
    tuple: &'a ExprTuple,
}

impl<'a> ExprSequence<'a> {
    const fn new(expr: &'a ExprTuple) -> Self {
        Self { tuple: expr }
    }
}

impl Format<PyFormatContext<'_>> for ExprSequence<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        f.join_comma_separated(self.tuple.end())
            .nodes(&self.tuple.elts)
            .finish()
    }
}

impl NeedsParentheses for ExprTuple {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}

/// Check if a tuple has already had parentheses in the input
fn is_parenthesized(tuple_range: TextRange, elts: &[Expr], source: &str) -> bool {
    let parentheses = '(';
    let first_char = &source[usize::from(tuple_range.start())..].chars().next();
    let Some(first_char) = first_char else {
        return false;
    };
    if *first_char != parentheses {
        return false;
    }

    // Consider `a = (1, 2), 3`: The first char of the current expr starts is a parentheses, but
    // it's not its own but that of its first tuple child. We know that it belongs to the child
    // because if it wouldn't, the child would start (at least) a char later
    let Some(first_child) = elts.first() else {
        return false;
    };
    first_child.range().start() != tuple_range.start()
}
