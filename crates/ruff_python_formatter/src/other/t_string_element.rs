use std::borrow::Cow;

use ruff_formatter::{format_args, write, Buffer, RemoveSoftLinesBuffer};
use ruff_python_ast::{
    AnyStringFlags, ConversionFlag, Expr, StringFlags, TStringElement, TStringInterpolationElement,
    TStringLiteralElement,
};
use ruff_text_size::{Ranged, TextSlice};

use crate::comments::{dangling_open_parenthesis_comments, trailing_comments};
use crate::context::{FTStringState, NodeLevel, WithFTStringState, WithNodeLevel};
use crate::expression::left_most;
use crate::prelude::*;
use crate::string::normalize_string;
use crate::verbatim::verbatim_text;

use super::t_string::TStringContext;

/// Formats a t-string element which is either a literal or interpolation
/// element.
///
/// This delegates the actual formatting to the appropriate formatter.
pub(crate) struct FormatTStringElement<'a> {
    element: &'a TStringElement,
    context: TStringContext,
}

impl<'a> FormatTStringElement<'a> {
    pub(crate) fn new(element: &'a TStringElement, context: TStringContext) -> Self {
        Self { element, context }
    }
}

impl Format<PyFormatContext<'_>> for FormatTStringElement<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self.element {
            TStringElement::Literal(string_literal) => {
                FormatTStringLiteralElement::new(string_literal, self.context.flags()).fmt(f)
            }
            TStringElement::Interpolation(interpolation) => {
                FormatTStringInterpolationElement::new(interpolation, self.context).fmt(f)
            }
        }
    }
}

/// Formats a t-string literal element.
pub(crate) struct FormatTStringLiteralElement<'a> {
    element: &'a TStringLiteralElement,
    /// Flags of the enclosing t-string part
    fstring_flags: AnyStringFlags,
}

impl<'a> FormatTStringLiteralElement<'a> {
    pub(crate) fn new(element: &'a TStringLiteralElement, fstring_flags: AnyStringFlags) -> Self {
        Self {
            element,
            fstring_flags,
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatTStringLiteralElement<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let literal_content = f.context().source().slice(self.element);
        let normalized = normalize_string(literal_content, 0, self.fstring_flags, false);
        match &normalized {
            Cow::Borrowed(_) => source_text_slice(self.element.range()).fmt(f),
            Cow::Owned(normalized) => text(normalized).fmt(f),
        }
    }
}

/// Context representing an t-string interpolation element.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TStringInterpolationElementContext {
    /// The context of the parent t-string containing this interpolation element.
    parent_context: TStringContext,
    /// Indicates whether this interpolation element has format specifier or not.
    has_format_spec: bool,
}

impl TStringInterpolationElementContext {
    /// Returns the [`TStringContext`] containing this interpolation element.
    pub(crate) fn t_string(self) -> TStringContext {
        self.parent_context
    }

    /// Returns `true` if the interpolation element can contain line breaks.
    pub(crate) fn can_contain_line_breaks(self) -> bool {
        self.parent_context.layout().is_multiline()
            // For a triple-quoted t-string, the element can't be formatted into multiline if it
            // has a format specifier because otherwise the newline would be treated as part of the
            // format specifier.
            //
            // Given the following t-string:
            // ```python
            // t"""aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbb ccccccccccc {variable:.3f} ddddddddddddddd eeeeeeee"""
            // ```
            //
            // We can't format it as:
            // ```python
            // t"""aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbb ccccccccccc {
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

/// Formats an t-string expression element.
pub(crate) struct FormatTStringInterpolationElement<'a> {
    element: &'a TStringInterpolationElement,
    context: TStringInterpolationElementContext,
}

impl<'a> FormatTStringInterpolationElement<'a> {
    pub(crate) fn new(element: &'a TStringInterpolationElement, context: TStringContext) -> Self {
        Self {
            element,
            context: TStringInterpolationElementContext {
                parent_context: context,
                has_format_spec: element.format_spec.is_some(),
            },
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatTStringInterpolationElement<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let TStringInterpolationElement {
            interpolation,
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
            // t"""foo {
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
                    verbatim_text(&**interpolation),
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

            // If an interpolation starts with a `{`, we need to add a space before the
            // curly brace to avoid turning it into a literal curly with `{{`.
            //
            // For example,
            // ```python
            // t"{ {'x': 1, 'y': 2} }"
            // #  ^                ^
            // ```
            //
            // We need to preserve the space highlighted by `^`. The whitespace
            // before the closing curly brace is not strictly necessary, but it's
            // added to maintain consistency.
            let bracket_spacing =
                needs_bracket_spacing(interpolation, f.context()).then_some(format_with(|f| {
                    if self.context.can_contain_line_breaks() {
                        soft_line_break_or_space().fmt(f)
                    } else {
                        space().fmt(f)
                    }
                }));

            let item = format_with(|f: &mut PyFormatter| {
                // Update the context to be inside the t-string interpolation element.
                let f = &mut WithFTStringState::new(
                    FTStringState::InsideInterpolationElement(self.context),
                    f,
                );

                write!(f, [bracket_spacing, interpolation.format()])?;

                // Conversion comes first, then the format spec.
                match conversion {
                    ConversionFlag::Str => text("!s").fmt(f)?,
                    ConversionFlag::Ascii => text("!a").fmt(f)?,
                    ConversionFlag::Repr => text("!r").fmt(f)?,
                    ConversionFlag::None => (),
                }

                if let Some(format_spec) = format_spec.as_deref() {
                    token(":").fmt(f)?;

                    for element in &format_spec.elements {
                        FormatTStringElement::new(element, self.context.t_string()).fmt(f)?;
                    }

                    // These trailing comments can only occur if the format specifier is
                    // present. For example,
                    //
                    // ```python
                    // t"{
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
