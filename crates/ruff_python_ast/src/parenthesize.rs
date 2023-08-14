use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{TextRange, TextSize};
use std::ops::Sub;

use crate::node::AnyNodeRef;
use crate::Ranged;

/// A wrapper around an expression that may be parenthesized.
#[derive(Debug)]
pub struct ParenthesizedExpression<'a> {
    /// The underlying AST node.
    expr: AnyNodeRef<'a>,
    /// The range of the expression including parentheses, if the expression is parenthesized;
    /// or `None`, if the expression is not parenthesized.
    range: Option<TextRange>,
}

impl<'a> ParenthesizedExpression<'a> {
    /// Given an expression and its parent, returns a parenthesized expression.
    pub fn from_expr(expr: AnyNodeRef<'a>, parent: AnyNodeRef<'a>, contents: &str) -> Self {
        Self {
            expr,
            range: parenthesized_range(expr, parent, contents),
        }
    }

    /// Returns `true` if the expression is parenthesized.
    pub fn is_parenthesized(&self) -> bool {
        self.range.is_some()
    }
}

impl Ranged for ParenthesizedExpression<'_> {
    fn range(&self) -> TextRange {
        self.range.unwrap_or_else(|| self.expr.range())
    }
}

/// Returns the [`TextRange`] of a given expression including parentheses, if the expression is
/// parenthesized; or `None`, if the expression is not parenthesized.
fn parenthesized_range(expr: AnyNodeRef, parent: AnyNodeRef, contents: &str) -> Option<TextRange> {
    // If the parent is an `arguments` node, then the range of the expression includes the closing
    // parenthesis, so exclude it from our test range.
    let exclusive_parent_end = if parent.is_arguments() {
        parent.end().sub(TextSize::new(1))
    } else {
        parent.end()
    };

    // First, test if there's a closing parenthesis because it tends to be cheaper.
    let tokenizer =
        SimpleTokenizer::new(contents, TextRange::new(expr.end(), exclusive_parent_end));
    let right = tokenizer.skip_trivia().next()?;

    if right.kind == SimpleTokenKind::RParen {
        // Next, test for the opening parenthesis.
        let mut tokenizer =
            SimpleTokenizer::up_to_without_back_comment(expr.start(), contents).skip_trivia();
        let left = tokenizer.next_back()?;
        if left.kind == SimpleTokenKind::LParen {
            return Some(TextRange::new(left.start(), right.end()));
        }
    }

    None
}
