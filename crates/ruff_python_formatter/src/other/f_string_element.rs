use std::borrow::Cow;
use std::num::NonZeroU16;

use ruff_formatter::{format_args, write, Buffer, RemoveSoftLinesBuffer};
use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::{
    self as ast, AnyNodeRef, ConversionFlag, Expr, FStringElement, FStringExpressionElement,
    FStringLiteralElement,
};
use ruff_text_size::Ranged;

use crate::comments::{dangling_open_parenthesis_comments, trailing_comments};
use crate::context::{FStringState, NodeLevel, WithFStringState, WithNodeLevel};
use crate::options::MagicTrailingComma;
use crate::prelude::*;
use crate::preview::is_hex_codes_in_unicode_sequences_enabled;
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
        let normalized = normalize_string(
            literal_content,
            self.context.quotes(),
            self.context.prefix(),
            is_hex_codes_in_unicode_sequences_enabled(f.context()),
        );
        match &normalized {
            Cow::Borrowed(_) => source_text_slice(self.element.range()).fmt(f),
            Cow::Owned(normalized) => text(normalized).fmt(f),
        }
    }
}

/// Formats an f-string expression element.
pub(crate) struct FormatFStringExpressionElement<'a> {
    element: &'a FStringExpressionElement,
    context: FStringContext,
}

impl<'a> FormatFStringExpressionElement<'a> {
    pub(crate) fn new(element: &'a FStringExpressionElement, context: FStringContext) -> Self {
        Self { element, context }
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
                            if self.context.layout().is_flat() {
                                space().fmt(f)
                            } else {
                                soft_line_break_or_space().fmt(f)
                            }
                        }))
                    }
                    _ => None,
                };

                bracket_spacing.fmt(f)?;

                // Update the context to be inside the f-string.
                let f = &mut WithFStringState::new(FStringState::Inside(self.context.quotes()), f);

                // If we're going to remove the soft line breaks, then there's a chance
                // that there will be trailing commas in the formatted expression. For
                // example, if the expression is a collection which exceeds the line length:
                //
                // ```python
                // xxxxxxx = f"aaaaaaaa {['aaaaaaaaaaaaa', 'bbbbbbbbbbbb', 'cccccccccccc', 'dddddddddddd']} aaaaaaaaaa"
                // ```
                //
                // Currently, it's difficult to remove that conditionally, because the
                // context would need to be passed down to all the expressions and the
                // magic trailing comma builder would need to be updated to handle this.
                //
                // So, we'll manually format the expression with the maximum line width
                // and disabling the magic trailing comma. This will ensure that even if
                // a trailing comma was added by the user, they're removed. This is expensive
                // so we've implemented some heuristics to avoid this in cases where the
                // expression can't contain a trailing comma.
                if self.context.layout().is_flat() && {
                    let visitor = &mut CanContainTrailingCommaVisitor::default();
                    visitor.visit_expr(expression);
                    visitor.can_have_trailing_comma
                } {
                    let options = f
                        .options()
                        .clone()
                        .with_line_width(NonZeroU16::MAX.into())
                        .with_magic_trailing_comma(MagicTrailingComma::Ignore);
                    let context = f.context().clone().with_options(options);
                    let formatted = crate::format!(context, [expression.format()])?;
                    text(formatted.print()?.as_code()).fmt(f)?;
                } else {
                    expression.format().fmt(f)?;
                }

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
                        .entries(
                            format_spec
                                .elements
                                .iter()
                                .map(|element| FormatFStringElement::new(element, self.context)),
                        )
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

                bracket_spacing.fmt(f)
            });

            let open_parenthesis_comments = if dangling_item_comments.is_empty() {
                None
            } else {
                Some(dangling_open_parenthesis_comments(dangling_item_comments))
            };

            token("{").fmt(f)?;

            {
                let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);

                if self.context.layout().is_flat() {
                    let mut buffer = RemoveSoftLinesBuffer::new(&mut *f);

                    write!(buffer, [open_parenthesis_comments, item])?;
                } else {
                    group(&format_args![
                        open_parenthesis_comments,
                        soft_block_indent(&item)
                    ])
                    .fmt(&mut f)?;
                }
            }

            token("}").fmt(f)
        }
    }
}

/// A visitor to check if an expression can contain a trailing comma.
#[derive(Default)]
struct CanContainTrailingCommaVisitor {
    can_have_trailing_comma: bool,
}

impl<'a> PreorderVisitor<'a> for CanContainTrailingCommaVisitor {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        match node {
            AnyNodeRef::ExprList(ast::ExprList { elts, .. })
            | AnyNodeRef::ExprTuple(ast::ExprTuple { elts, .. })
            | AnyNodeRef::ExprSet(ast::ExprSet { elts, .. }) => {
                if !elts.is_empty() {
                    self.can_have_trailing_comma = true;
                    return TraversalSignal::Skip;
                }
            }
            AnyNodeRef::ExprDict(ast::ExprDict { keys, values, .. }) => {
                if !keys.is_empty() || !values.is_empty() {
                    self.can_have_trailing_comma = true;
                    return TraversalSignal::Skip;
                }
            }
            AnyNodeRef::Arguments(arguments) => {
                if !arguments.is_empty() {
                    self.can_have_trailing_comma = true;
                    return TraversalSignal::Skip;
                }
            }
            _ => (),
        }

        // Any other expression with a trailing comma, assuming that it's a
        // valid syntax, is basically a tuple. So, we need to traverse into
        // it to check the inner expressions.
        TraversalSignal::Traverse
    }
}
