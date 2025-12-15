use std::borrow::Cow;

use ruff_formatter::{Buffer, FormatOptions as _, RemoveSoftLinesBuffer, format_args, write};
use ruff_python_ast::{
    AnyStringFlags, ConversionFlag, Expr, InterpolatedElement, InterpolatedStringElement,
    InterpolatedStringLiteralElement,
};
use ruff_text_size::{Ranged, TextSlice};

use crate::comments::dangling_open_parenthesis_comments;
use crate::context::{
    InterpolatedStringState, NodeLevel, WithInterpolatedStringState, WithNodeLevel,
};
use crate::expression::left_most;
use crate::prelude::*;
use crate::string::normalize_string;
use crate::verbatim::verbatim_text;

use super::interpolated_string::InterpolatedStringContext;

/// Formats an f-string element which is either a literal or a formatted expression.
///
/// This delegates the actual formatting to the appropriate formatter.
pub(crate) struct FormatInterpolatedStringElement<'a> {
    element: &'a InterpolatedStringElement,
    context: InterpolatedStringContext,
}

impl<'a> FormatInterpolatedStringElement<'a> {
    pub(crate) fn new(
        element: &'a InterpolatedStringElement,
        context: InterpolatedStringContext,
    ) -> Self {
        Self { element, context }
    }
}

impl Format<PyFormatContext<'_>> for FormatInterpolatedStringElement<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self.element {
            InterpolatedStringElement::Literal(string_literal) => {
                FormatFStringLiteralElement::new(string_literal, self.context.flags()).fmt(f)
            }
            InterpolatedStringElement::Interpolation(expression) => {
                FormatInterpolatedElement::new(expression, self.context).fmt(f)
            }
        }
    }
}

/// Formats an f-string literal element.
pub(crate) struct FormatFStringLiteralElement<'a> {
    element: &'a InterpolatedStringLiteralElement,
    /// Flags of the enclosing F-string part
    fstring_flags: AnyStringFlags,
}

impl<'a> FormatFStringLiteralElement<'a> {
    pub(crate) fn new(
        element: &'a InterpolatedStringLiteralElement,
        fstring_flags: AnyStringFlags,
    ) -> Self {
        Self {
            element,
            fstring_flags,
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatFStringLiteralElement<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let literal_content = f.context().source().slice(self.element);
        let normalized = normalize_string(literal_content, 0, self.fstring_flags, false);
        match &normalized {
            Cow::Borrowed(_) => source_text_slice(self.element.range()).fmt(f),
            Cow::Owned(normalized) => text(normalized).fmt(f),
        }
    }
}

/// Formats an f-string expression element.
pub(crate) struct FormatInterpolatedElement<'a> {
    element: &'a InterpolatedElement,
    context: InterpolatedStringContext,
}

impl<'a> FormatInterpolatedElement<'a> {
    pub(crate) fn new(
        element: &'a InterpolatedElement,
        context: InterpolatedStringContext,
    ) -> Self {
        Self { element, context }
    }
}

impl Format<PyFormatContext<'_>> for FormatInterpolatedElement<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let InterpolatedElement {
            expression,
            debug_text,
            conversion,
            format_spec,
            ..
        } = self.element;

        let expression = &**expression;

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
                    NormalizedDebugText(&debug_text.leading),
                    verbatim_text(expression),
                    NormalizedDebugText(&debug_text.trailing),
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

            let multiline = self.context.is_multiline();

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
            let bracket_spacing =
                needs_bracket_spacing(expression, f.context()).then_some(format_with(|f| {
                    if multiline {
                        soft_line_break_or_space().fmt(f)
                    } else {
                        space().fmt(f)
                    }
                }));

            let item = format_with(|f: &mut PyFormatter| {
                // Update the context to be inside the f-string expression element.
                let state = match f.context().interpolated_string_state() {
                    InterpolatedStringState::InsideInterpolatedElement(_)
                    | InterpolatedStringState::NestedInterpolatedElement(_) => {
                        InterpolatedStringState::NestedInterpolatedElement(self.context)
                    }
                    InterpolatedStringState::Outside => {
                        InterpolatedStringState::InsideInterpolatedElement(self.context)
                    }
                };
                let f = &mut WithInterpolatedStringState::new(state, f);

                write!(f, [bracket_spacing, expression.format()])?;

                // Conversion comes first, then the format spec.
                match conversion {
                    ConversionFlag::Str => text("!s").fmt(f)?,
                    ConversionFlag::Ascii => text("!a").fmt(f)?,
                    ConversionFlag::Repr => text("!r").fmt(f)?,
                    ConversionFlag::None => (),
                }

                if let Some(format_spec) = format_spec.as_deref() {
                    // ```py
                    // f"{
                    //     foo
                    //     # comment 27
                    //    :test}"
                    // ```
                    if comments.has_trailing(expression) {
                        soft_line_break().fmt(f)?;
                    }

                    token(":").fmt(f)?;

                    for element in &format_spec.elements {
                        FormatInterpolatedStringElement::new(element, self.context).fmt(f)?;
                    }
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

                if self.context.is_multiline() {
                    if format_spec.is_none() {
                        group(&format_args![
                            open_parenthesis_comments,
                            soft_block_indent(&item)
                        ])
                        .fmt(&mut f)?;
                    } else {
                        // For strings ending with a format spec, don't add a newline between the end of the format spec
                        // and closing curly brace because that is invalid syntax for single quoted strings and
                        // the newline is preserved as part of the format spec for triple quoted strings.

                        group(&format_args![
                            open_parenthesis_comments,
                            indent(&format_args![soft_line_break(), item])
                        ])
                        .fmt(&mut f)?;
                    }
                } else {
                    let mut buffer = RemoveSoftLinesBuffer::new(&mut *f);

                    write!(buffer, [open_parenthesis_comments, item])?;
                }
            }

            token("}").fmt(f)
        }
    }
}

fn needs_bracket_spacing(expr: &Expr, context: &PyFormatContext) -> bool {
    // Ruff parenthesizes single element tuples, that's why we shouldn't insert
    // a space around the curly braces for those.
    if expr
        .as_tuple_expr()
        .is_some_and(|tuple| !tuple.parenthesized && tuple.elts.len() == 1)
    {
        return false;
    }

    matches!(
        left_most(expr, context.comments().ranges(), context.source()),
        Expr::Dict(_) | Expr::DictComp(_) | Expr::Set(_) | Expr::SetComp(_)
    )
}

struct NormalizedDebugText<'a>(&'a str);

impl Format<PyFormatContext<'_>> for NormalizedDebugText<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let normalized = normalize_newlines(self.0, ['\r']);

        f.write_element(FormatElement::Text {
            text_width: TextWidth::from_text(&normalized, f.options().indent_width()),
            text: normalized.into_owned().into_boxed_str(),
        });

        Ok(())
    }
}
