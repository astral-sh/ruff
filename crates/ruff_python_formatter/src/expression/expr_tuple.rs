use ruff_formatter::{format_args, write, FormatRuleWithOptions};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprTuple;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::builders::parenthesize_if_expands;
use crate::comments::SourceComment;
use crate::expression::parentheses::{
    empty_parenthesized, optional_parentheses, parenthesized, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum TupleParentheses {
    /// By default tuples with a single element will include parentheses. Tuples with multiple elements
    /// will parenthesize if the expression expands. This means that tuples will often *preserve*
    /// their parentheses, but this differs from `Preserve` in that we may also *introduce*
    /// parentheses as well.
    #[default]
    Default,

    /// Handle special cases where parentheses are to be preserved.
    ///
    /// Black omits parentheses for tuples inside subscripts except if the tuple is already
    /// parenthesized in the source code.
    /// ```python
    /// x[a, :]
    /// x[a, b:]
    /// x[(a, b):]
    /// ```
    Preserve,

    /// The same as [`Self::Default`] except that it uses [`optional_parentheses`] rather than
    /// [`parenthesize_if_expands`]. This avoids adding parentheses if breaking any containing parenthesized
    /// expression makes the tuple fit.
    ///
    /// Avoids adding parentheses around the tuple because breaking the `sum` call expression is sufficient
    /// to make it fit.
    ///
    /// ```python
    /// return len(self.nodeseeeeeeeee), sum(
    //     len(node.parents) for node in self.node_map.values()
    // )
    /// ```
    OptionalParentheses,

    /// Handle the special cases where we don't include parentheses at all.
    ///
    /// Black never formats tuple targets of for loops with parentheses if inside a comprehension.
    /// For example, tuple targets will always be formatted on the same line, except when an element supports
    /// line-breaking in an un-parenthesized context.
    /// ```python
    /// # Input
    /// {k: v for x, (k, v) in this_is_a_very_long_variable_which_will_cause_a_trailing_comma_which_breaks_the_comprehension}
    ///
    /// # Black
    /// {
    ///     k: v
    ///     for x, (
    ///         k,
    ///         v,
    ///     ) in this_is_a_very_long_variable_which_will_cause_a_trailing_comma_which_breaks_the_comprehension
    /// }
    /// ```
    Never,

    /// Handle the special cases where we don't include parentheses if they are not required.
    ///
    /// Normally, black keeps parentheses, but in the case of for loops it formats
    /// ```python
    /// for (a, b) in x:
    ///     pass
    /// ```
    /// to
    /// ```python
    /// for a, b in x:
    ///     pass
    /// ```
    /// Black still does use parentheses in these positions if the group breaks or magic trailing
    /// comma is used.
    ///
    /// Additional examples:
    /// ```python
    /// for (a,) in []:
    /// pass
    /// for a, b in []:
    ///     pass
    /// for a, b in []:  # Strips parentheses
    ///     pass
    /// for (
    ///     aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
    ///     b,
    /// ) in []:
    ///     pass
    /// ```
    NeverPreserve,
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
            elts,
            ctx: _,
            range: _,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

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
                return empty_parenthesized("(", dangling, ")").fmt(f);
            }
            [single] => match self.parentheses {
                TupleParentheses::Preserve
                    if !is_tuple_parenthesized(item, f.context().source()) =>
                {
                    write!(f, [single.format(), token(",")])
                }
                _ =>
                // A single element tuple always needs parentheses and a trailing comma, except when inside of a subscript
                {
                    parenthesized("(", &format_args![single.format(), token(",")], ")")
                        .with_dangling_comments(dangling)
                        .fmt(f)
                }
            },
            // If the tuple has parentheses, we generally want to keep them. The exception are for
            // loops, see `TupleParentheses::NeverPreserve` doc comment.
            //
            // Unlike other expression parentheses, tuple parentheses are part of the range of the
            // tuple itself.
            _ if is_tuple_parenthesized(item, f.context().source())
                && !(self.parentheses == TupleParentheses::NeverPreserve
                    && dangling.is_empty()) =>
            {
                parenthesized("(", &ExprSequence::new(item), ")")
                    .with_dangling_comments(dangling)
                    .fmt(f)
            }
            _ => match self.parentheses {
                TupleParentheses::Never => {
                    let separator =
                        format_with(|f| group(&format_args![token(","), space()]).fmt(f));
                    f.join_with(separator)
                        .entries(elts.iter().formatted())
                        .finish()
                }
                TupleParentheses::Preserve => group(&ExprSequence::new(item)).fmt(f),
                TupleParentheses::NeverPreserve => {
                    optional_parentheses(&ExprSequence::new(item)).fmt(f)
                }
                TupleParentheses::OptionalParentheses if item.elts.len() == 2 => {
                    optional_parentheses(&ExprSequence::new(item)).fmt(f)
                }
                TupleParentheses::Default | TupleParentheses::OptionalParentheses => {
                    parenthesize_if_expands(&ExprSequence::new(item)).fmt(f)
                }
            },
        }
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
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
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

/// Return `true` if a tuple is parenthesized in the source code.
pub(crate) fn is_tuple_parenthesized(tuple: &ExprTuple, source: &str) -> bool {
    let Some(elt) = tuple.elts.first() else {
        return true;
    };

    // Count the number of open parentheses between the start of the tuple and the first element.
    let open_parentheses_count =
        SimpleTokenizer::new(source, TextRange::new(tuple.start(), elt.start()))
            .skip_trivia()
            .filter(|token| token.kind() == SimpleTokenKind::LParen)
            .count();
    if open_parentheses_count == 0 {
        return false;
    }

    // Count the number of parentheses between the end of the first element and its trailing comma.
    let close_parentheses_count =
        SimpleTokenizer::new(source, TextRange::new(elt.end(), tuple.end()))
            .skip_trivia()
            .take_while(|token| token.kind() != SimpleTokenKind::Comma)
            .filter(|token| token.kind() == SimpleTokenKind::RParen)
            .count();

    // If the number of open parentheses is greater than the number of close parentheses, the tuple
    // is parenthesized.
    open_parentheses_count > close_parentheses_count
}
