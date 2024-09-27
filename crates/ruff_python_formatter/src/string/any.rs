use std::iter::FusedIterator;

use memchr::memchr2;

use ruff_python_ast::{
    self as ast, AnyNodeRef, AnyStringFlags, Expr, ExprBytesLiteral, ExprFString,
    ExprStringLiteral, ExpressionRef, StringFlags, StringLiteral,
};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::expression::expr_f_string::f_string_quoting;
use crate::other::f_string::FormatFString;
use crate::other::string_literal::{FormatStringLiteral, StringLiteralKind};
use crate::prelude::*;
use crate::string::Quoting;

/// Represents any kind of string expression. This could be either a string,
/// bytes or f-string.
#[derive(Copy, Clone, Debug)]
pub(crate) enum AnyString<'a> {
    String(&'a ExprStringLiteral),
    Bytes(&'a ExprBytesLiteral),
    FString(&'a ExprFString),
}

impl<'a> AnyString<'a> {
    /// Creates a new [`AnyString`] from the given [`Expr`].
    ///
    /// Returns `None` if the expression is not either a string, bytes or f-string.
    pub(crate) fn from_expression(expression: &'a Expr) -> Option<AnyString<'a>> {
        match expression {
            Expr::StringLiteral(string) => Some(AnyString::String(string)),
            Expr::BytesLiteral(bytes) => Some(AnyString::Bytes(bytes)),
            Expr::FString(fstring) => Some(AnyString::FString(fstring)),
            _ => None,
        }
    }

    /// Returns `true` if the string is implicitly concatenated.
    pub(crate) fn is_implicit_concatenated(self) -> bool {
        match self {
            Self::String(ExprStringLiteral { value, .. }) => value.is_implicit_concatenated(),
            Self::Bytes(ExprBytesLiteral { value, .. }) => value.is_implicit_concatenated(),
            Self::FString(ExprFString { value, .. }) => value.is_implicit_concatenated(),
        }
    }

    /// Returns the quoting to be used for this string.
    pub(super) fn quoting(self, locator: &Locator<'_>) -> Quoting {
        match self {
            Self::String(_) | Self::Bytes(_) => Quoting::CanChange,
            Self::FString(f_string) => f_string_quoting(f_string, locator),
        }
    }

    /// Returns a vector of all the [`AnyStringPart`] of this string.
    pub(super) fn parts(self, quoting: Quoting) -> AnyStringPartsIter<'a> {
        match self {
            Self::String(ExprStringLiteral { value, .. }) => {
                AnyStringPartsIter::String(value.iter())
            }
            Self::Bytes(ExprBytesLiteral { value, .. }) => AnyStringPartsIter::Bytes(value.iter()),
            Self::FString(ExprFString { value, .. }) => {
                AnyStringPartsIter::FString(value.iter(), quoting)
            }
        }
    }

    pub(crate) fn is_multiline(self, source: &str) -> bool {
        match self {
            AnyString::String(_) | AnyString::Bytes(_) => {
                self.parts(Quoting::default())
                    .next()
                    .is_some_and(|part| part.flags().is_triple_quoted())
                    && memchr2(b'\n', b'\r', source[self.range()].as_bytes()).is_some()
            }
            AnyString::FString(fstring) => {
                memchr2(b'\n', b'\r', source[fstring.range].as_bytes()).is_some()
            }
        }
    }
}

impl Ranged for AnyString<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::String(expr) => expr.range(),
            Self::Bytes(expr) => expr.range(),
            Self::FString(expr) => expr.range(),
        }
    }
}

impl<'a> From<&AnyString<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyString<'a>) -> Self {
        match value {
            AnyString::String(expr) => AnyNodeRef::ExprStringLiteral(expr),
            AnyString::Bytes(expr) => AnyNodeRef::ExprBytesLiteral(expr),
            AnyString::FString(expr) => AnyNodeRef::ExprFString(expr),
        }
    }
}

impl<'a> From<AnyString<'a>> for AnyNodeRef<'a> {
    fn from(value: AnyString<'a>) -> Self {
        AnyNodeRef::from(&value)
    }
}

impl<'a> From<&AnyString<'a>> for ExpressionRef<'a> {
    fn from(value: &AnyString<'a>) -> Self {
        match value {
            AnyString::String(expr) => ExpressionRef::StringLiteral(expr),
            AnyString::Bytes(expr) => ExpressionRef::BytesLiteral(expr),
            AnyString::FString(expr) => ExpressionRef::FString(expr),
        }
    }
}

impl<'a> From<&'a ExprBytesLiteral> for AnyString<'a> {
    fn from(value: &'a ExprBytesLiteral) -> Self {
        AnyString::Bytes(value)
    }
}

impl<'a> From<&'a ExprStringLiteral> for AnyString<'a> {
    fn from(value: &'a ExprStringLiteral) -> Self {
        AnyString::String(value)
    }
}

impl<'a> From<&'a ExprFString> for AnyString<'a> {
    fn from(value: &'a ExprFString) -> Self {
        AnyString::FString(value)
    }
}

pub(super) enum AnyStringPartsIter<'a> {
    String(std::slice::Iter<'a, StringLiteral>),
    Bytes(std::slice::Iter<'a, ast::BytesLiteral>),
    FString(std::slice::Iter<'a, ast::FStringPart>, Quoting),
}

impl<'a> Iterator for AnyStringPartsIter<'a> {
    type Item = AnyStringPart<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let part = match self {
            Self::String(inner) => {
                let part = inner.next()?;
                AnyStringPart::String {
                    part,
                    layout: StringLiteralKind::String,
                }
            }
            Self::Bytes(inner) => AnyStringPart::Bytes(inner.next()?),
            Self::FString(inner, quoting) => {
                let part = inner.next()?;
                match part {
                    ast::FStringPart::Literal(string_literal) => AnyStringPart::String {
                        part: string_literal,
                        layout: StringLiteralKind::InImplicitlyConcatenatedFString(*quoting),
                    },
                    ast::FStringPart::FString(f_string) => AnyStringPart::FString {
                        part: f_string,
                        quoting: *quoting,
                    },
                }
            }
        };

        Some(part)
    }
}

impl FusedIterator for AnyStringPartsIter<'_> {}

/// Represents any kind of string which is part of an implicitly concatenated
/// string. This could be either a string, bytes or f-string.
///
/// This is constructed from the [`AnyString::parts`] method on [`AnyString`].
#[derive(Clone, Debug)]
pub(super) enum AnyStringPart<'a> {
    String {
        part: &'a ast::StringLiteral,
        layout: StringLiteralKind,
    },
    Bytes(&'a ast::BytesLiteral),
    FString {
        part: &'a ast::FString,
        quoting: Quoting,
    },
}

impl AnyStringPart<'_> {
    fn flags(&self) -> AnyStringFlags {
        match self {
            Self::String { part, .. } => part.flags.into(),
            Self::Bytes(bytes_literal) => bytes_literal.flags.into(),
            Self::FString { part, .. } => part.flags.into(),
        }
    }
}

impl<'a> From<&AnyStringPart<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyStringPart<'a>) -> Self {
        match value {
            AnyStringPart::String { part, .. } => AnyNodeRef::StringLiteral(part),
            AnyStringPart::Bytes(part) => AnyNodeRef::BytesLiteral(part),
            AnyStringPart::FString { part, .. } => AnyNodeRef::FString(part),
        }
    }
}

impl Ranged for AnyStringPart<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::String { part, .. } => part.range(),
            Self::Bytes(part) => part.range(),
            Self::FString { part, .. } => part.range(),
        }
    }
}

impl Format<PyFormatContext<'_>> for AnyStringPart<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self {
            AnyStringPart::String { part, layout } => {
                FormatStringLiteral::new(part, *layout).fmt(f)
            }
            AnyStringPart::Bytes(bytes_literal) => bytes_literal.format().fmt(f),
            AnyStringPart::FString { part, quoting } => FormatFString::new(part, *quoting).fmt(f),
        }
    }
}
