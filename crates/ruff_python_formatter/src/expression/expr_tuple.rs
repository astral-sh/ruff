use crate::builders::optional_parentheses;
use crate::comments::{dangling_node_comments, Comments};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use ruff_formatter::{format_args, write, FormatRuleWithOptions};
use ruff_text_size::TextRange;
use rustpython_parser::ast::ExprTuple;
use rustpython_parser::ast::{Expr, Ranged};

#[derive(Eq, PartialEq, Debug, Default)]
pub enum TupleParentheses {
    /// Effectively `None` in `Option<Parentheses>`
    #[default]
    Default,
    /// Effectively `Some(Parentheses)` in `Option<Parentheses>`
    Expr(Parentheses),
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
        match elts.as_slice() {
            [] => {
                write!(
                    f,
                    [
                        // An empty tuple always needs parentheses, but does not have a comma
                        &text("("),
                        block_indent(&dangling_node_comments(item)),
                        &text(")"),
                    ]
                )
            }
            [single] => {
                write!(
                    f,
                    [group(&format_args![
                        // A single element tuple always needs parentheses and a trailing comma
                        &text("("),
                        soft_block_indent(&format_args![single.format(), &text(",")]),
                        &text(")"),
                    ])]
                )
            }
            // If the tuple has parentheses, we generally want to keep them. The exception are for
            // loops, see `TupleParentheses::StripInsideForLoop` doc comment.
            //
            // Unlike other expression parentheses, tuple parentheses are part of the range of the
            // tuple itself.
            elts if is_parenthesized(*range, elts, f)
                && self.parentheses != TupleParentheses::StripInsideForLoop =>
            {
                write!(
                    f,
                    [group(&format_args![
                        // If there were previously parentheses, keep them
                        &text("("),
                        soft_block_indent(&ExprSequence::new(elts)),
                        &text(")"),
                    ])]
                )
            }
            elts => optional_parentheses(&ExprSequence::new(elts)).fmt(f),
        }
    }

    fn fmt_dangling_comments(&self, _node: &ExprTuple, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}

#[derive(Debug)]
struct ExprSequence<'a> {
    elts: &'a [Expr],
}

impl<'a> ExprSequence<'a> {
    const fn new(elts: &'a [Expr]) -> Self {
        Self { elts }
    }
}

impl Format<PyFormatContext<'_>> for ExprSequence<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        f.join_comma_separated().nodes(self.elts.iter()).finish()
    }
}

impl NeedsParentheses for ExprTuple {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}

/// Check if a tuple has already had parentheses in the input
fn is_parenthesized(
    tuple_range: TextRange,
    elts: &[Expr],
    f: &mut Formatter<PyFormatContext<'_>>,
) -> bool {
    let parentheses = '(';
    let first_char = &f.context().contents()[usize::from(tuple_range.start())..]
        .chars()
        .next();
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
