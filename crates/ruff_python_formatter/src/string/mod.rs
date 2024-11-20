pub(crate) use normalize::{normalize_string, NormalizedString, StringNormalizer};
use ruff_python_ast::str::Quote;
use ruff_python_ast::visitor::source_order::{
    walk_f_string_element, SourceOrderVisitor, TraversalSignal,
};
use ruff_python_ast::AstNode;
use ruff_python_ast::{
    self as ast,
    str_prefix::{AnyStringPrefix, StringLiteralPrefix},
    AnyStringFlags, StringFlags,
};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

use crate::expression::expr_f_string::f_string_quoting;
use crate::prelude::*;
use crate::preview::is_f_string_formatting_enabled;
use crate::QuoteStyle;

pub(crate) mod docstring;
pub(crate) mod implicit;
mod normalize;

#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum Quoting {
    #[default]
    CanChange,
    Preserve,
}

impl Format<PyFormatContext<'_>> for AnyStringPrefix {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        // Remove the unicode prefix `u` if any because it is meaningless in Python 3+.
        if !matches!(
            self,
            AnyStringPrefix::Regular(StringLiteralPrefix::Empty | StringLiteralPrefix::Unicode)
        ) {
            token(self.as_str()).fmt(f)?;
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct StringQuotes {
    triple: bool,
    quote_char: Quote,
}

impl Format<PyFormatContext<'_>> for StringQuotes {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let quotes = match (self.quote_char, self.triple) {
            (Quote::Single, false) => "'",
            (Quote::Single, true) => "'''",
            (Quote::Double, false) => "\"",
            (Quote::Double, true) => "\"\"\"",
        };

        token(quotes).fmt(f)
    }
}

impl From<AnyStringFlags> for StringQuotes {
    fn from(value: AnyStringFlags) -> Self {
        Self {
            triple: value.is_triple_quoted(),
            quote_char: value.quote_style(),
        }
    }
}

impl TryFrom<QuoteStyle> for Quote {
    type Error = ();

    fn try_from(style: QuoteStyle) -> Result<Quote, ()> {
        match style {
            QuoteStyle::Single => Ok(Quote::Single),
            QuoteStyle::Double => Ok(Quote::Double),
            QuoteStyle::Preserve => Err(()),
        }
    }
}

impl From<Quote> for QuoteStyle {
    fn from(value: Quote) -> Self {
        match value {
            Quote::Single => QuoteStyle::Single,
            Quote::Double => QuoteStyle::Double,
        }
    }
}

// Extension trait that adds formatter specific helper methods to `StringLike`.
pub(crate) trait StringLikeExtensions {
    fn quoting(&self, source: &str) -> Quoting;

    fn is_multiline(&self, context: &PyFormatContext) -> bool;
}

impl StringLikeExtensions for ast::StringLike<'_> {
    fn quoting(&self, source: &str) -> Quoting {
        match self {
            Self::String(_) | Self::Bytes(_) => Quoting::CanChange,
            Self::FString(f_string) => f_string_quoting(f_string, source),
        }
    }

    fn is_multiline(&self, context: &PyFormatContext) -> bool {
        match self {
            Self::String(_) | Self::Bytes(_) => self.parts().any(|part| {
                part.flags().is_triple_quoted()
                    && context.source().contains_line_break(self.range())
            }),
            Self::FString(expr) => {
                let mut visitor = FStringMultilineVisitor::new(context);
                expr.visit_source_order(&mut visitor);
                visitor.is_multiline
            }
        }
    }
}

struct FStringMultilineVisitor<'a> {
    context: &'a PyFormatContext<'a>,
    is_multiline: bool,
}

impl<'a> FStringMultilineVisitor<'a> {
    fn new(context: &'a PyFormatContext<'a>) -> Self {
        Self {
            context,
            is_multiline: false,
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for FStringMultilineVisitor<'a> {
    fn enter_node(&mut self, _node: ruff_python_ast::AnyNodeRef<'a>) -> TraversalSignal {
        if self.is_multiline {
            TraversalSignal::Skip
        } else {
            TraversalSignal::Traverse
        }
    }

    fn visit_string_literal(&mut self, string_literal: &'a ast::StringLiteral) {
        if string_literal.flags.is_triple_quoted()
            && self
                .context
                .source()
                .contains_line_break(string_literal.range())
        {
            self.is_multiline = true;
        }
    }

    fn visit_f_string_element(&mut self, f_string_element: &'a ast::FStringElement) {
        let is_multiline = match f_string_element {
            ast::FStringElement::Literal(literal) => {
                self.context.source().contains_line_break(literal.range())
            }
            ast::FStringElement::Expression(expression) => {
                if is_f_string_formatting_enabled(self.context) {
                    // Expressions containing comments can't be joined.
                    self.context.comments().contains_comments(expression.into())
                } else {
                    // Multiline f-string expressions can't be joined if the f-string formatting is disabled because
                    // the string gets inserted in verbatim preserving the newlines.
                    self.context
                        .source()
                        .contains_line_break(expression.range())
                }
            }
        };
        if is_multiline {
            self.is_multiline = true;
        } else {
            walk_f_string_element(self, f_string_element);
        }
    }
}
