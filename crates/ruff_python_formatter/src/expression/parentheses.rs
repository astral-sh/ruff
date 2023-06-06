use crate::trivia::{
    find_first_non_trivia_character_after, find_first_non_trivia_character_before,
};
use ruff_python_ast::node::AnyNodeRef;

pub(crate) trait NeedsParentheses {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses;
}

pub(super) fn default_expression_needs_parentheses(
    node: AnyNodeRef,
    parenthesize: Parenthesize,
    source: &str,
) -> Parentheses {
    debug_assert!(
        node.is_expression(),
        "Should only be called for expressions"
    );

    // `Optional` or `Preserve` and expression has parentheses in source code.
    if !parenthesize.is_if_breaks() && is_expression_parenthesized(node, source) {
        Parentheses::Always
    }
    // `Optional` or `IfBreaks`: Add parentheses if the expression doesn't fit on a line
    else if !parenthesize.is_preserve() {
        Parentheses::Optional
    } else {
        //`Preserve` and expression has no parentheses in the source code
        Parentheses::Never
    }
}

/// Configures if the expression should be parenthesized.
#[derive(Copy, Clone, Debug, Default)]
pub enum Parenthesize {
    /// Parenthesize the expression if it has parenthesis in the source.
    #[default]
    Preserve,

    /// Parenthesizes the expression if it doesn't fit on a line OR if the expression is parenthesized in the source code.
    Optional,

    /// Parenthesizes the expression only if it doesn't fit on a line.
    IfBreaks,
}

impl Parenthesize {
    const fn is_if_breaks(self) -> bool {
        matches!(self, Parenthesize::IfBreaks)
    }

    const fn is_preserve(self) -> bool {
        matches!(self, Parenthesize::Preserve)
    }
}

/// Whether it is necessary to add parentheses around an expression.
/// This is different from [`Parenthesize`] in that it is the resolved representation: It takes into account
/// whether there are parentheses in the source code or not.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Parentheses {
    /// Always create parentheses
    Always,

    /// Only add parentheses when necessary because the expression breaks over multiple lines.
    Optional,

    /// Custom handling by the node's formatter implementation
    Custom,

    /// Never add parentheses
    Never,
}

fn is_expression_parenthesized(expr: AnyNodeRef, contents: &str) -> bool {
    use rustpython_parser::ast::Ranged;

    debug_assert!(
        expr.is_expression(),
        "Should only be called for expressions"
    );

    // Search backwards to avoid ambiguity with `(a, )` and because it's faster
    matches!(
        find_first_non_trivia_character_after(expr.end(), contents),
        Some((_, ')'))
    )
        // Search forwards to confirm that this is not a nested expression `(5 + d * 3)`
        && matches!(
        find_first_non_trivia_character_before(expr.start(), contents),
        Some((_, '('))
    )
}
