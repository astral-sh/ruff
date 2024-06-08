use std::borrow::Cow;

use ruff_formatter::{format_args, write, Buffer, RemoveSoftLinesBuffer};
use ruff_python_ast::{
    ConversionFlag, Expr, FStringElement, FStringExpressionElement, FStringLiteralElement,
    StringFlags,
};
use ruff_text_size::Ranged;

use crate::comments::{dangling_open_parenthesis_comments, trailing_comments};
use crate::context::{FStringState, NodeLevel, WithFStringState, WithNodeLevel};
use crate::prelude::*;
use crate::string::normalize_string;
use crate::verbatim::verbatim_text;

use super::f_string::FStringContext;

/// Formats an f-string element which is either a literal or a formatted expression.
///
/// This delegates the actual formatting to the appropriate formatter.
pub(crate) struct FormatFStringElement<'a> {
    element: &'a FStringElement,
    context: FStringContext,
}

impl<'a> FormatFStringElement<'a> {
    pub(crate) fn new(element: &'a FStringElement, context: FStringContext) -> Self {
        Self { element, context }
    }
}

impl Format<PyFormatContext<'_>> for FormatFStringElement<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self.element {
            FStringElement::Literal(string_literal) => {
                FormatFStringLiteralElement::new(string_literal, self.context).fmt(f)
            }
            FStringElement::Expression(expression) => {
                FormatFStringExpressionElement::new(expression, self.context).fmt(f)
            }
        }
    }
}

/// Formats an f-string literal element.
pub(crate) struct FormatFStringLiteralElement<'a> {
    element: &'a FStringLiteralElement,
    context: FStringContext,
}

impl<'a> FormatFStringLiteralElement<'a> {
    pub(crate) fn new(element: &'a FStringLiteralElement, context: FStringContext) -> Self {
        Self { element, context }
    }
}

impl Format<PyFormatContext<'_>> for FormatFStringLiteralElement<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let literal_content = f.context().locator().slice(self.element.range());
        let normalized = normalize_string(literal_content, 0, self.context.flags(), true);
        match &normalized {
            Cow::Borrowed(_) => source_text_slice(self.element.range()).fmt(f),
            Cow::Owned(normalized) => text(normalized).fmt(f),
        }
    }
}

/// Context representing an f-string expression element.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FStringExpressionElementContext {
    /// The context of the parent f-string containing this expression element.
    parent_context: FStringContext,
    /// Indicates whether this expression element has format specifier or not.
    has_format_spec: bool,
}

impl FStringExpressionElementContext {
    /// Returns the [`FStringContext`] containing this expression element.
    pub(crate) fn f_string(self) -> FStringContext {
        self.parent_context
    }

    /// Returns `true` if the expression element can contain line breaks.
    pub(crate) fn can_contain_line_breaks(self) -> bool {
        self.parent_context.layout().is_multiline()
            // For a triple-quoted f-string, the element can't be formatted into multiline if it
            // has a format specifier because otherwise the newline would be treated as part of the
            // format specifier.
            //
            // Given the following f-string:
            // ```python
            // f"""aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbb ccccccccccc {variable:.3f} ddddddddddddddd eeeeeeee"""
            // ```
            //
            // We can't format it as:
            // ```python
            // f"""aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbb ccccccccccc {
            //     variable:.3f
            // } ddddddddddddddd eeeeeeee"""
            // ```
            //
            // Here, the format specifier string would become ".3f\n", which is not what we want.
            // But, if the original source code already contained a newline, they'll be preserved.
            //
            // The Python version is irrelevant in this case.
            && !(self.parent_context.flags().is_triple_quoted() && self.has_format_spec)
    }
}

/// Formats an f-string expression element.
pub(crate) struct FormatFStringExpressionElement<'a> {
    element: &'a FStringExpressionElement,
    context: FStringExpressionElementContext,
}

impl<'a> FormatFStringExpressionElement<'a> {
    pub(crate) fn new(element: &'a FStringExpressionElement, context: FStringContext) -> Self {
        Self {
            element,
            context: FStringExpressionElementContext {
                parent_context: context,
                has_format_spec: element.format_spec.is_some(),
            },
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatFStringExpressionElement<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let FStringExpressionElement {
            expression,
            debug_text,
            conversion,
            format_spec,
            ..
        } = self.element;

        if let Some(debug_text) = debug_text {
            token("{").fmt(f)?;

            let comments = f.context().comments();

            // If the element has a debug text, preserve the same formatting as
            // in the source code (`verbatim`). This requires us to mark all of
            // the surrounding comments as formatted.
            comments.mark_verbatim_node_comments_formatted(self.element.into());

            // Above method doesn't mark the leading and trailing comments of the element.
            // There can't be any leading comments for an expression element, but there
            // can be trailing comments. For example,
            //
            // ```python
            // f"""foo {
            //     x:.3f
            //     # trailing comment
            // }"""
            // ```
            for trailing_comment in comments.trailing(self.element) {
                trailing_comment.mark_formatted();
            }

            write!(
                f,
                [
                    text(&debug_text.leading),
                    verbatim_text(&**expression),
                    text(&debug_text.trailing),
                ]
            )?;

            // Even if debug text is present, any whitespace between the
            // conversion flag and the format spec doesn't need to be preserved.
            match conversion {
                ConversionFlag::Str => text("!s").fmt(f)?,
                ConversionFlag::Ascii => text("!a").fmt(f)?,
                ConversionFlag::Repr => text("!r").fmt(f)?,
                ConversionFlag::None => (),
            }

            if let Some(format_spec) = format_spec.as_deref() {
                write!(f, [token(":"), verbatim_text(format_spec)])?;
            }

            token("}").fmt(f)
        } else {
            let comments = f.context().comments().clone();
            let dangling_item_comments = comments.dangling(self.element);

            let item = format_with(|f| {
                let bracket_spacing = match expression.as_ref() {
                    // If an expression starts with a `{`, we need to add a space before the
                    // curly brace to avoid turning it into a literal curly with `{{`.
                    //
                    // For example,
                    // ```python
                    // f"{ {'x': 1, 'y': 2} }"
                    // #  ^                ^
                    // ```
                    //
                    // We need to preserve the space highlighted by `^`. The whitespace
                    // before the closing curly brace is not strictly necessary, but it's
                    // added to maintain consistency.
                    Expr::Dict(_) | Expr::DictComp(_) | Expr::Set(_) | Expr::SetComp(_) => {
                        Some(format_with(|f| {
                            if self.context.can_contain_line_breaks() {
                                soft_line_break_or_space().fmt(f)
                            } else {
                                space().fmt(f)
                            }
                        }))
                    }
                    _ => None,
                };

                // Update the context to be inside the f-string expression element.
                let f = &mut WithFStringState::new(
                    FStringState::InsideExpressionElement(self.context),
                    f,
                );

                write!(f, [bracket_spacing, expression.format()])?;

                // Conversion comes first, then the format spec.
                match conversion {
                    ConversionFlag::Str => text("!s").fmt(f)?,
                    ConversionFlag::Ascii => text("!a").fmt(f)?,
                    ConversionFlag::Repr => text("!r").fmt(f)?,
                    ConversionFlag::None => (),
                }

                if let Some(format_spec) = format_spec.as_deref() {
                    token(":").fmt(f)?;

                    f.join()
                        .entries(format_spec.elements.iter().map(|element| {
                            FormatFStringElement::new(element, self.context.f_string())
                        }))
                        .finish()?;

                    // These trailing comments can only occur if the format specifier is
                    // present. For example,
                    //
                    // ```python
                    // f"{
                    //    x:.3f
                    //    # comment
                    // }"
                    // ```
                    //
                    // Any other trailing comments are attached to the expression itself.
                    trailing_comments(comments.trailing(self.element)).fmt(f)?;
                }

                if conversion.is_none() && format_spec.is_none() {
                    bracket_spacing.fmt(f)?;
                }

                Ok(())
            });

            let open_parenthesis_comments = if dangling_item_comments.is_empty() {
                None
            } else {
                Some(dangling_open_parenthesis_comments(dangling_item_comments))
            };

            token("{").fmt(f)?;

            {
                let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);

                if self.context.can_contain_line_breaks() {
                    group(&format_args![
                        open_parenthesis_comments,
                        soft_block_indent(&item)
                    ])
                    .fmt(&mut f)?;
                } else {
                    let mut buffer = RemoveSoftLinesBuffer::new(&mut *f);

                    write!(buffer, [open_parenthesis_comments, item])?;
                }
            }

            token("}").fmt(f)
        }
    }
}
