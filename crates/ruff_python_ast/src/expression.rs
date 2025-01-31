use std::iter::FusedIterator;

use ruff_text_size::{Ranged, TextRange};

use crate::{
    self as ast, AnyNodeRef, AnyStringFlags, Expr, ExprBytesLiteral, ExprFString, ExprRef,
    ExprStringLiteral, StringFlags,
};

impl<'a> From<&'a Box<Expr>> for ExprRef<'a> {
    fn from(value: &'a Box<Expr>) -> Self {
        ExprRef::from(value.as_ref())
    }
}

/// Unowned pendant to all the literal variants of [`ast::Expr`] that stores a
/// reference instead of an owned value.
#[derive(Copy, Clone, Debug, PartialEq, is_macro::Is)]
pub enum LiteralExpressionRef<'a> {
    StringLiteral(&'a ast::ExprStringLiteral),
    BytesLiteral(&'a ast::ExprBytesLiteral),
    NumberLiteral(&'a ast::ExprNumberLiteral),
    BooleanLiteral(&'a ast::ExprBooleanLiteral),
    NoneLiteral(&'a ast::ExprNoneLiteral),
    EllipsisLiteral(&'a ast::ExprEllipsisLiteral),
}

impl Ranged for LiteralExpressionRef<'_> {
    fn range(&self) -> TextRange {
        match self {
            LiteralExpressionRef::StringLiteral(expression) => expression.range(),
            LiteralExpressionRef::BytesLiteral(expression) => expression.range(),
            LiteralExpressionRef::NumberLiteral(expression) => expression.range(),
            LiteralExpressionRef::BooleanLiteral(expression) => expression.range(),
            LiteralExpressionRef::NoneLiteral(expression) => expression.range(),
            LiteralExpressionRef::EllipsisLiteral(expression) => expression.range(),
        }
    }
}

impl<'a> From<LiteralExpressionRef<'a>> for AnyNodeRef<'a> {
    fn from(value: LiteralExpressionRef<'a>) -> Self {
        match value {
            LiteralExpressionRef::StringLiteral(expression) => {
                AnyNodeRef::ExprStringLiteral(expression)
            }
            LiteralExpressionRef::BytesLiteral(expression) => {
                AnyNodeRef::ExprBytesLiteral(expression)
            }
            LiteralExpressionRef::NumberLiteral(expression) => {
                AnyNodeRef::ExprNumberLiteral(expression)
            }
            LiteralExpressionRef::BooleanLiteral(expression) => {
                AnyNodeRef::ExprBooleanLiteral(expression)
            }
            LiteralExpressionRef::NoneLiteral(expression) => {
                AnyNodeRef::ExprNoneLiteral(expression)
            }
            LiteralExpressionRef::EllipsisLiteral(expression) => {
                AnyNodeRef::ExprEllipsisLiteral(expression)
            }
        }
    }
}

impl LiteralExpressionRef<'_> {
    /// Returns `true` if the literal is either a string or bytes literal that
    /// is implicitly concatenated.
    pub fn is_implicit_concatenated(&self) -> bool {
        match self {
            LiteralExpressionRef::StringLiteral(expression) => {
                expression.value.is_implicit_concatenated()
            }
            LiteralExpressionRef::BytesLiteral(expression) => {
                expression.value.is_implicit_concatenated()
            }
            _ => false,
        }
    }
}

/// An enum that holds a reference to a string-like expression from the AST. This includes string
/// literals, bytes literals, and f-strings.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StringLike<'a> {
    String(&'a ast::ExprStringLiteral),
    Bytes(&'a ast::ExprBytesLiteral),
    FString(&'a ast::ExprFString),
}

impl<'a> StringLike<'a> {
    pub const fn is_fstring(self) -> bool {
        matches!(self, Self::FString(_))
    }

    /// Returns an iterator over the [`StringLikePart`] contained in this string-like expression.
    pub fn parts(&self) -> StringLikePartIter<'a> {
        match self {
            StringLike::String(expr) => StringLikePartIter::String(expr.value.iter()),
            StringLike::Bytes(expr) => StringLikePartIter::Bytes(expr.value.iter()),
            StringLike::FString(expr) => StringLikePartIter::FString(expr.value.iter()),
        }
    }

    /// Returns `true` if the string is implicitly concatenated.
    pub fn is_implicit_concatenated(self) -> bool {
        match self {
            Self::String(ExprStringLiteral { value, .. }) => value.is_implicit_concatenated(),
            Self::Bytes(ExprBytesLiteral { value, .. }) => value.is_implicit_concatenated(),
            Self::FString(ExprFString { value, .. }) => value.is_implicit_concatenated(),
        }
    }

    pub const fn as_expression_ref(self) -> ExprRef<'a> {
        match self {
            StringLike::String(expr) => ExprRef::StringLiteral(expr),
            StringLike::Bytes(expr) => ExprRef::BytesLiteral(expr),
            StringLike::FString(expr) => ExprRef::FString(expr),
        }
    }
}

impl<'a> From<&'a ast::ExprStringLiteral> for StringLike<'a> {
    fn from(value: &'a ast::ExprStringLiteral) -> Self {
        StringLike::String(value)
    }
}

impl<'a> From<&'a ast::ExprBytesLiteral> for StringLike<'a> {
    fn from(value: &'a ast::ExprBytesLiteral) -> Self {
        StringLike::Bytes(value)
    }
}

impl<'a> From<&'a ast::ExprFString> for StringLike<'a> {
    fn from(value: &'a ast::ExprFString) -> Self {
        StringLike::FString(value)
    }
}

impl<'a> From<&StringLike<'a>> for ExprRef<'a> {
    fn from(value: &StringLike<'a>) -> Self {
        match value {
            StringLike::String(expr) => ExprRef::StringLiteral(expr),
            StringLike::Bytes(expr) => ExprRef::BytesLiteral(expr),
            StringLike::FString(expr) => ExprRef::FString(expr),
        }
    }
}

impl<'a> From<StringLike<'a>> for AnyNodeRef<'a> {
    fn from(value: StringLike<'a>) -> Self {
        AnyNodeRef::from(&value)
    }
}

impl<'a> From<&StringLike<'a>> for AnyNodeRef<'a> {
    fn from(value: &StringLike<'a>) -> Self {
        match value {
            StringLike::String(expr) => AnyNodeRef::ExprStringLiteral(expr),
            StringLike::Bytes(expr) => AnyNodeRef::ExprBytesLiteral(expr),
            StringLike::FString(expr) => AnyNodeRef::ExprFString(expr),
        }
    }
}

impl<'a> TryFrom<&'a Expr> for StringLike<'a> {
    type Error = ();

    fn try_from(value: &'a Expr) -> Result<Self, Self::Error> {
        match value {
            Expr::StringLiteral(value) => Ok(Self::String(value)),
            Expr::BytesLiteral(value) => Ok(Self::Bytes(value)),
            Expr::FString(value) => Ok(Self::FString(value)),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<AnyNodeRef<'a>> for StringLike<'a> {
    type Error = ();

    fn try_from(value: AnyNodeRef<'a>) -> Result<Self, Self::Error> {
        match value {
            AnyNodeRef::ExprStringLiteral(value) => Ok(Self::String(value)),
            AnyNodeRef::ExprBytesLiteral(value) => Ok(Self::Bytes(value)),
            AnyNodeRef::ExprFString(value) => Ok(Self::FString(value)),
            _ => Err(()),
        }
    }
}

impl Ranged for StringLike<'_> {
    fn range(&self) -> TextRange {
        match self {
            StringLike::String(literal) => literal.range(),
            StringLike::Bytes(literal) => literal.range(),
            StringLike::FString(literal) => literal.range(),
        }
    }
}

/// An enum that holds a reference to an individual part of a string-like expression.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StringLikePart<'a> {
    String(&'a ast::StringLiteral),
    Bytes(&'a ast::BytesLiteral),
    FString(&'a ast::FString),
}

impl<'a> StringLikePart<'a> {
    /// Returns the [`AnyStringFlags`] for the current string-like part.
    pub fn flags(&self) -> AnyStringFlags {
        match self {
            StringLikePart::String(string) => AnyStringFlags::from(string.flags),
            StringLikePart::Bytes(bytes) => AnyStringFlags::from(bytes.flags),
            StringLikePart::FString(f_string) => AnyStringFlags::from(f_string.flags),
        }
    }

    /// Returns the range of the string's content in the source (minus prefix and quotes).
    pub fn content_range(self) -> TextRange {
        let kind = self.flags();
        TextRange::new(
            self.start() + kind.opener_len(),
            self.end() - kind.closer_len(),
        )
    }

    pub const fn is_string_literal(self) -> bool {
        matches!(self, Self::String(_))
    }

    pub const fn as_string_literal(self) -> Option<&'a ast::StringLiteral> {
        match self {
            StringLikePart::String(value) => Some(value),
            _ => None,
        }
    }

    pub const fn is_fstring(self) -> bool {
        matches!(self, Self::FString(_))
    }
}

impl<'a> From<&'a ast::StringLiteral> for StringLikePart<'a> {
    fn from(value: &'a ast::StringLiteral) -> Self {
        StringLikePart::String(value)
    }
}

impl<'a> From<&'a ast::BytesLiteral> for StringLikePart<'a> {
    fn from(value: &'a ast::BytesLiteral) -> Self {
        StringLikePart::Bytes(value)
    }
}

impl<'a> From<&'a ast::FString> for StringLikePart<'a> {
    fn from(value: &'a ast::FString) -> Self {
        StringLikePart::FString(value)
    }
}

impl<'a> From<&StringLikePart<'a>> for AnyNodeRef<'a> {
    fn from(value: &StringLikePart<'a>) -> Self {
        AnyNodeRef::from(*value)
    }
}

impl<'a> From<StringLikePart<'a>> for AnyNodeRef<'a> {
    fn from(value: StringLikePart<'a>) -> Self {
        match value {
            StringLikePart::String(part) => AnyNodeRef::StringLiteral(part),
            StringLikePart::Bytes(part) => AnyNodeRef::BytesLiteral(part),
            StringLikePart::FString(part) => AnyNodeRef::FString(part),
        }
    }
}

impl Ranged for StringLikePart<'_> {
    fn range(&self) -> TextRange {
        match self {
            StringLikePart::String(part) => part.range(),
            StringLikePart::Bytes(part) => part.range(),
            StringLikePart::FString(part) => part.range(),
        }
    }
}

/// An iterator over all the [`StringLikePart`] of a string-like expression.
///
/// This is created by the [`StringLike::parts`] method.
#[derive(Clone)]
pub enum StringLikePartIter<'a> {
    String(std::slice::Iter<'a, ast::StringLiteral>),
    Bytes(std::slice::Iter<'a, ast::BytesLiteral>),
    FString(std::slice::Iter<'a, ast::FStringPart>),
}

impl<'a> Iterator for StringLikePartIter<'a> {
    type Item = StringLikePart<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let part = match self {
            StringLikePartIter::String(inner) => StringLikePart::String(inner.next()?),
            StringLikePartIter::Bytes(inner) => StringLikePart::Bytes(inner.next()?),
            StringLikePartIter::FString(inner) => {
                let part = inner.next()?;
                match part {
                    ast::FStringPart::Literal(string_literal) => {
                        StringLikePart::String(string_literal)
                    }
                    ast::FStringPart::FString(f_string) => StringLikePart::FString(f_string),
                }
            }
        };

        Some(part)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            StringLikePartIter::String(inner) => inner.size_hint(),
            StringLikePartIter::Bytes(inner) => inner.size_hint(),
            StringLikePartIter::FString(inner) => inner.size_hint(),
        }
    }
}

impl DoubleEndedIterator for StringLikePartIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let part = match self {
            StringLikePartIter::String(inner) => StringLikePart::String(inner.next_back()?),
            StringLikePartIter::Bytes(inner) => StringLikePart::Bytes(inner.next_back()?),
            StringLikePartIter::FString(inner) => {
                let part = inner.next_back()?;
                match part {
                    ast::FStringPart::Literal(string_literal) => {
                        StringLikePart::String(string_literal)
                    }
                    ast::FStringPart::FString(f_string) => StringLikePart::FString(f_string),
                }
            }
        };

        Some(part)
    }
}

impl FusedIterator for StringLikePartIter<'_> {}
impl ExactSizeIterator for StringLikePartIter<'_> {}
