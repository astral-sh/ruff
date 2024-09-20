use ruff_formatter::{write, FormatRuleWithOptions};
use ruff_python_ast::WithItem;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::{
    is_expression_parenthesized, parenthesized, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::preview::is_with_single_item_pre_39_enabled;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum WithItemLayout {
    /// A with item that is the `with`s only context manager and its context expression is parenthesized.
    ///
    /// ```python
    /// with (
    ///     a + b
    /// ) as b:
    ///     ...
    /// ```
    ///
    /// This layout is used independent of the target version.
    SingleParenthesizedContextManager,

    /// A with item that is the `with`s only context manager and it has no `target`.
    ///
    /// ```python
    /// with a + b:
    ///     ...
    /// ```
    ///
    /// In this case, use [`maybe_parenthesize_expression`] to get the same formatting as when
    /// formatting any other statement with a clause header.
    ///
    /// This layout is only used for Python 3.9+.
    ///
    /// Be careful that [`Self::SingleParenthesizedContextManager`] and this layout are compatible because
    /// removing optional parentheses or adding parentheses will make the formatter pick the opposite layout
    /// the second time the file gets formatted.
    SingleWithoutTarget,

    /// This layout is used when the target python version doesn't support parenthesized context managers and
    /// it's either a single, unparenthesized with item or multiple items.
    ///
    /// ```python
    /// with a + b:
    ///     ...
    ///
    /// with a, b:
    ///     ...
    /// ```
    Python38OrOlder { single: bool },

    /// A with item where the `with` formatting adds parentheses around all context managers if necessary.
    ///
    /// ```python
    /// with (
    ///     a,
    ///     b,
    /// ): pass
    /// ```
    ///
    /// This layout is generally used when the target version is Python 3.9 or newer, but it is used
    /// for Python 3.8 if the with item has a leading or trailing comment.
    ///
    /// ```python
    /// with (
    ///     # leading
    ///     a
    // ): ...
    /// ```
    #[default]
    ParenthesizedContextManagers,
}

#[derive(Default)]
pub struct FormatWithItem {
    layout: WithItemLayout,
}

impl FormatRuleWithOptions<WithItem, PyFormatContext<'_>> for FormatWithItem {
    type Options = WithItemLayout;

    fn with_options(self, options: Self::Options) -> Self {
        Self { layout: options }
    }
}

impl FormatNodeRule<WithItem> for FormatWithItem {
    fn fmt_fields(&self, item: &WithItem, f: &mut PyFormatter) -> FormatResult<()> {
        let WithItem {
            range: _,
            context_expr,
            optional_vars,
        } = item;

        let comments = f.context().comments().clone();
        let trailing_as_comments = comments.dangling(item);

        let is_parenthesized = is_expression_parenthesized(
            context_expr.into(),
            f.context().comments().ranges(),
            f.context().source(),
        );

        match self.layout {
            // Remove the parentheses of the `with_items` if the with statement adds parentheses
            WithItemLayout::ParenthesizedContextManagers => {
                if is_parenthesized {
                    // ...except if the with item is parenthesized, then use this with item as a preferred breaking point
                    // or when it has comments, then parenthesize it to prevent comments from moving.
                    maybe_parenthesize_expression(
                        context_expr,
                        item,
                        Parenthesize::IfBreaksParenthesizedNested,
                    )
                    .fmt(f)?;
                } else {
                    context_expr
                        .format()
                        .with_options(Parentheses::Never)
                        .fmt(f)?;
                }
            }

            WithItemLayout::SingleParenthesizedContextManager
            | WithItemLayout::SingleWithoutTarget => {
                write!(
                    f,
                    [maybe_parenthesize_expression(
                        context_expr,
                        item,
                        Parenthesize::IfBreaks
                    )]
                )?;
            }

            WithItemLayout::Python38OrOlder { single } => {
                let parenthesize = if (single && is_with_single_item_pre_39_enabled(f.context()))
                    || is_parenthesized
                {
                    Parenthesize::IfBreaks
                } else {
                    Parenthesize::IfRequired
                };
                write!(
                    f,
                    [maybe_parenthesize_expression(
                        context_expr,
                        item,
                        parenthesize
                    )]
                )?;
            }
        }

        if let Some(optional_vars) = optional_vars {
            write!(f, [space(), token("as"), space()])?;

            if trailing_as_comments.is_empty() {
                write!(f, [optional_vars.format()])?;
            } else {
                write!(
                    f,
                    [parenthesized(
                        "(",
                        &optional_vars.format().with_options(Parentheses::Never),
                        ")",
                    )
                    .with_dangling_comments(trailing_as_comments)]
                )?;
            }
        }

        Ok(())
    }
}
