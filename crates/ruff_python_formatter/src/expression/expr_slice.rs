use ruff_formatter::{write, FormatError};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{Expr, ExprSlice, ExprUnaryOp, UnaryOp};
use ruff_python_trivia::{SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{dangling_comments, SourceComment};
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprSlice;

impl FormatNodeRule<ExprSlice> for FormatExprSlice {
    /// This implementation deviates from black in that comments are attached to the section of the
    /// slice they originate in
    fn fmt_fields(&self, item: &ExprSlice, f: &mut PyFormatter) -> FormatResult<()> {
        // `[lower:upper:step]`
        let ExprSlice {
            lower,
            upper,
            step,
            range,
        } = item;

        let (first_colon, second_colon) = find_colons(
            f.context().source(),
            *range,
            lower.as_deref(),
            upper.as_deref(),
        )?;

        // Handle comment placement
        // In placements.rs, we marked comment for None nodes a dangling and associated all others
        // as leading or dangling wrt to a node. That means we either format a node and only have
        // to handle newlines and spacing, or the node is None and we insert the corresponding
        // slice of dangling comments
        let comments = f.context().comments().clone();
        let slice_dangling_comments = comments.dangling(item);
        // Put the dangling comments (where the nodes are missing) into buckets
        let first_colon_partition_index =
            slice_dangling_comments.partition_point(|x| x.start() < first_colon.start());
        let (dangling_lower_comments, dangling_upper_step_comments) =
            slice_dangling_comments.split_at(first_colon_partition_index);
        let (dangling_upper_comments, dangling_step_comments) =
            if let Some(second_colon) = &second_colon {
                let second_colon_partition_index = dangling_upper_step_comments
                    .partition_point(|x| x.start() < second_colon.start());
                dangling_upper_step_comments.split_at(second_colon_partition_index)
            } else {
                // Without a second colon they remaining dangling comments belong between the first
                // colon and the closing parentheses
                (dangling_upper_step_comments, [].as_slice())
            };

        // Ensure there a no dangling comments for a node if the node is present
        debug_assert!(lower.is_none() || dangling_lower_comments.is_empty());
        debug_assert!(upper.is_none() || dangling_upper_comments.is_empty());
        debug_assert!(step.is_none() || dangling_step_comments.is_empty());

        // Handle spacing around the colon(s)
        // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#slices
        let lower_simple = lower.as_ref().is_none_or(|expr| is_simple_expr(expr));
        let upper_simple = upper.as_ref().is_none_or(|expr| is_simple_expr(expr));
        let step_simple = step.as_ref().is_none_or(|expr| is_simple_expr(expr));
        let all_simple = lower_simple && upper_simple && step_simple;

        // lower
        if let Some(lower) = lower {
            write!(f, [lower.format(), line_suffix_boundary()])?;
        } else {
            dangling_comments(dangling_lower_comments).fmt(f)?;
        }

        // First colon
        // The spacing after the colon depends on both the lhs and the rhs:
        // ```
        // e00 = x[:]
        // e01 = x[:1]
        // e02 = x[: a()]
        // e10 = x[1:]
        // e11 = x[1:1]
        // e12 = x[1 : a()]
        // e20 = x[a() :]
        // e21 = x[a() : 1]
        // e22 = x[a() : a()]
        // e200 = "e"[a() : :]
        // e201 = "e"[a() :: 1]
        // e202 = "e"[a() :: a()]
        // ```
        if !all_simple && lower.is_some() {
            space().fmt(f)?;
        }
        token(":").fmt(f)?;
        // No upper node, no need for a space, e.g. `x[a() :]`
        if !all_simple && upper.is_some() {
            space().fmt(f)?;
        }

        // Upper
        if let Some(upper) = upper {
            let upper_leading_comments = comments.leading(upper.as_ref());
            leading_comments_spacing(f, upper_leading_comments)?;
            write!(f, [upper.format(), line_suffix_boundary()])?;
        } else {
            if let Some(first) = dangling_upper_comments.first() {
                // Here the spacing for end-of-line comments works but own line comments need
                // explicit spacing
                if first.line_position().is_own_line() {
                    hard_line_break().fmt(f)?;
                }
            }
            dangling_comments(dangling_upper_comments).fmt(f)?;
        }

        // (optionally) step
        if second_colon.is_some() {
            // Same spacing rules as for the first colon, except for the strange case when the
            // second colon exists, but neither upper nor step
            // ```
            // e200 = "e"[a() : :]
            // e201 = "e"[a() :: 1]
            // e202 = "e"[a() :: a()]
            // ```
            if !all_simple && (upper.is_some() || step.is_none()) {
                space().fmt(f)?;
            }
            token(":").fmt(f)?;
            // No step node, no need for a space
            if !all_simple && step.is_some() {
                space().fmt(f)?;
            }
            if let Some(step) = step {
                let step_leading_comments = comments.leading(step.as_ref());
                leading_comments_spacing(f, step_leading_comments)?;
                step.format().fmt(f)?;
            } else if !dangling_step_comments.is_empty() {
                // Put the colon and comments on their own lines
                write!(
                    f,
                    [hard_line_break(), dangling_comments(dangling_step_comments)]
                )?;
            }
        } else {
            debug_assert!(step.is_none(), "step can't exist without a second colon");
        }
        Ok(())
    }
}

/// We're in a slice, so we know there's a first colon, but with have to look into the source
/// to find out whether there is a second one, too, e.g. `[1:2]` and `[1:10:2]`.
///
/// Returns the first and optionally the second colon.
pub(crate) fn find_colons(
    contents: &str,
    range: TextRange,
    lower: Option<&Expr>,
    upper: Option<&Expr>,
) -> FormatResult<(SimpleToken, Option<SimpleToken>)> {
    let after_lower = lower.as_ref().map_or(range.start(), Ranged::end);
    let mut tokens = SimpleTokenizer::new(contents, TextRange::new(after_lower, range.end()))
        .skip_trivia()
        .skip_while(|token| token.kind == SimpleTokenKind::RParen);
    let first_colon = tokens.next().ok_or(FormatError::syntax_error(
        "Didn't find any token for slice first colon",
    ))?;
    if first_colon.kind != SimpleTokenKind::Colon {
        return Err(FormatError::syntax_error(
            "Slice first colon token was not a colon",
        ));
    }

    let after_upper = upper.as_ref().map_or(first_colon.end(), Ranged::end);
    let mut tokens = SimpleTokenizer::new(contents, TextRange::new(after_upper, range.end()))
        .skip_trivia()
        .skip_while(|token| token.kind == SimpleTokenKind::RParen);
    let second_colon = if let Some(token) = tokens.next() {
        if token.kind != SimpleTokenKind::Colon {
            return Err(FormatError::syntax_error(
                "Expected a colon for the second colon token",
            ));
        }
        Some(token)
    } else {
        None
    };
    Ok((first_colon, second_colon))
}

/// Determines whether this expression needs a space around the colon
/// <https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#slices>
fn is_simple_expr(expr: &Expr) -> bool {
    // Unary op expressions except `not` can be simple.
    if let Some(ExprUnaryOp {
        op: UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert,
        operand,
        ..
    }) = expr.as_unary_op_expr()
    {
        is_simple_expr(operand)
    } else {
        expr.is_literal_expr() || expr.is_name_expr()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExprSliceCommentSection {
    Lower,
    Upper,
    Step,
}

/// Assigns a comment to lower/upper/step in `[lower:upper:step]`.
///
/// ```python
/// "sliceable"[
///     # lower comment
///     :
///     # upper comment
///     :
///     # step comment
/// ]
/// ```
pub(crate) fn assign_comment_in_slice(
    comment: TextRange,
    contents: &str,
    expr_slice: &ExprSlice,
) -> ExprSliceCommentSection {
    let ExprSlice {
        lower,
        upper,
        step: _,
        range,
    } = expr_slice;

    let (first_colon, second_colon) =
        find_colons(contents, *range, lower.as_deref(), upper.as_deref())
            .expect("SyntaxError when trying to parse slice");

    if comment.start() < first_colon.start() {
        ExprSliceCommentSection::Lower
    } else {
        // We are to the right of the first colon
        if let Some(second_colon) = second_colon {
            if comment.start() < second_colon.start() {
                ExprSliceCommentSection::Upper
            } else {
                ExprSliceCommentSection::Step
            }
        } else {
            // No second colon means there is no step
            ExprSliceCommentSection::Upper
        }
    }
}

/// Manual spacing for the leading comments of upper and step
fn leading_comments_spacing(
    f: &mut PyFormatter,
    leading_comments: &[SourceComment],
) -> FormatResult<()> {
    if let Some(first) = leading_comments.first() {
        if first.line_position().is_own_line() {
            // Insert a newline after the colon so the comment ends up on its own line
            hard_line_break().fmt(f)?;
        } else {
            // Insert the two spaces between the colon and the end-of-line comment after the colon
            write!(f, [space(), space()])?;
        }
    }
    Ok(())
}

impl NeedsParentheses for ExprSlice {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
