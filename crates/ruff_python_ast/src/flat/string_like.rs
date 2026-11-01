use super::{AstNode, FStringPartView, SyntaxNode};
use crate::{self as ast, AnyStringFlags, Expr, NodeKind, StringFlags};
use ruff_text_size::{Ranged, TextRange};
use std::iter::FusedIterator;

/// A borrowed string literal, bytes literal, f-string, or t-string expression.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StringLike<'a>(Expr<'a>);

/// The concrete syntax of a string-like expression.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StringLikeKind<'a> {
    String(ast::ExprStringLiteral<'a>),
    Bytes(ast::ExprBytesLiteral<'a>),
    FString(ast::ExprFString<'a>),
    TString(ast::ExprTString<'a>),
}

impl<'a> StringLike<'a> {
    #[inline]
    pub fn classify(self) -> StringLikeKind<'a> {
        match self.0.node_kind() {
            NodeKind::ExprStringLiteral => StringLikeKind::String(self.0.syntax_node().retype()),
            NodeKind::ExprBytesLiteral => StringLikeKind::Bytes(self.0.syntax_node().retype()),
            NodeKind::ExprFString => StringLikeKind::FString(self.0.syntax_node().retype()),
            NodeKind::ExprTString => StringLikeKind::TString(self.0.syntax_node().retype()),
            _ => unreachable!("StringLike contains one of the four string expression kinds"),
        }
    }

    pub fn is_interpolated_string(self) -> bool {
        matches!(
            self.0.node_kind(),
            NodeKind::ExprTString | NodeKind::ExprFString
        )
    }

    /// Returns an iterator over the [`StringLikePart`] contained in this string-like expression.
    #[inline]
    pub fn parts(
        self,
    ) -> impl DoubleEndedIterator<Item = StringLikePart<'a>> + ExactSizeIterator + FusedIterator + Clone
    {
        self.part_nodes().iter().map(StringLikePart)
    }

    /// Returns `true` if the string is implicitly concatenated.
    pub fn is_implicit_concatenated(self) -> bool {
        self.part_nodes().len() > 1
    }
}

impl<'a> From<ast::ExprStringLiteral<'a>> for StringLike<'a> {
    fn from(value: ast::ExprStringLiteral<'a>) -> Self {
        Self(value.into())
    }
}

impl<'a> From<ast::ExprBytesLiteral<'a>> for StringLike<'a> {
    fn from(value: ast::ExprBytesLiteral<'a>) -> Self {
        Self(value.into())
    }
}

impl<'a> From<ast::ExprFString<'a>> for StringLike<'a> {
    fn from(value: ast::ExprFString<'a>) -> Self {
        Self(value.into())
    }
}

impl<'a> From<ast::ExprTString<'a>> for StringLike<'a> {
    fn from(value: ast::ExprTString<'a>) -> Self {
        Self(value.into())
    }
}

impl<'a> From<StringLike<'a>> for Expr<'a> {
    fn from(value: StringLike<'a>) -> Self {
        value.0
    }
}

impl<'a> From<StringLike<'a>> for SyntaxNode<'a> {
    fn from(value: StringLike<'a>) -> Self {
        value.0.syntax_node()
    }
}

impl<'a> From<&StringLike<'a>> for SyntaxNode<'a> {
    fn from(value: &StringLike<'a>) -> Self {
        value.0.syntax_node()
    }
}

impl<'a> TryFrom<Expr<'a>> for StringLike<'a> {
    type Error = ();

    fn try_from(value: Expr<'a>) -> Result<Self, Self::Error> {
        Self::try_from(value.syntax_node())
    }
}

impl<'a> TryFrom<SyntaxNode<'a>> for StringLike<'a> {
    type Error = ();

    fn try_from(value: SyntaxNode<'a>) -> Result<Self, Self::Error> {
        match value.node_kind() {
            NodeKind::ExprStringLiteral
            | NodeKind::ExprBytesLiteral
            | NodeKind::ExprFString
            | NodeKind::ExprTString => Ok(Self(value.retype())),
            _ => Err(()),
        }
    }
}

impl Ranged for StringLike<'_> {
    fn range(&self) -> TextRange {
        self.0.range()
    }
}

/// One literal part of a string-like expression.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StringLikePart<'a>(SyntaxNode<'a>);

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StringLikePartKind<'a> {
    String(ast::StringLiteral<'a>),
    Bytes(ast::BytesLiteral<'a>),
    FString(ast::FString<'a>),
    TString(ast::TString<'a>),
}

impl<'a> StringLikePart<'a> {
    #[inline]
    pub fn classify(self) -> StringLikePartKind<'a> {
        match self.0.node_kind() {
            NodeKind::StringLiteral => StringLikePartKind::String(self.0.syntax_node().retype()),
            NodeKind::BytesLiteral => StringLikePartKind::Bytes(self.0.syntax_node().retype()),
            NodeKind::FString => StringLikePartKind::FString(self.0.syntax_node().retype()),
            NodeKind::TString => StringLikePartKind::TString(self.0.syntax_node().retype()),
            _ => unreachable!("StringLikePart contains one of the four literal part kinds"),
        }
    }

    /// Returns the [`AnyStringFlags`] for the current string-like part.
    #[inline]
    pub fn flags(&self) -> AnyStringFlags {
        match self.classify() {
            StringLikePartKind::String(string) => AnyStringFlags::from(string.flags()),
            StringLikePartKind::Bytes(bytes) => AnyStringFlags::from(bytes.flags()),
            StringLikePartKind::FString(f_string) => AnyStringFlags::from(f_string.flags()),
            StringLikePartKind::TString(t_string) => AnyStringFlags::from(t_string.flags()),
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

    pub fn is_string_literal(self) -> bool {
        self.0.node_kind() == NodeKind::StringLiteral
    }

    pub fn is_interpolated_string(self) -> bool {
        matches!(self.0.node_kind(), NodeKind::FString | NodeKind::TString)
    }
}

impl<'a> From<ast::StringLiteral<'a>> for StringLikePart<'a> {
    fn from(value: ast::StringLiteral<'a>) -> Self {
        Self(value.syntax_node())
    }
}

impl<'a> From<ast::BytesLiteral<'a>> for StringLikePart<'a> {
    fn from(value: ast::BytesLiteral<'a>) -> Self {
        Self(value.syntax_node())
    }
}

impl<'a> From<ast::FString<'a>> for StringLikePart<'a> {
    fn from(value: ast::FString<'a>) -> Self {
        Self(value.syntax_node())
    }
}

impl<'a> From<ast::TString<'a>> for StringLikePart<'a> {
    fn from(value: ast::TString<'a>) -> Self {
        Self(value.syntax_node())
    }
}

impl<'a> From<FStringPartView<'a>> for StringLikePart<'a> {
    fn from(value: FStringPartView<'a>) -> Self {
        match value {
            FStringPartView::Literal(literal) => literal.into(),
            FStringPartView::FString(fstring) => fstring.into(),
        }
    }
}

impl<'a> From<&StringLikePart<'a>> for SyntaxNode<'a> {
    fn from(value: &StringLikePart<'a>) -> Self {
        value.0
    }
}

impl<'a> From<StringLikePart<'a>> for SyntaxNode<'a> {
    fn from(value: StringLikePart<'a>) -> Self {
        value.0
    }
}

impl Ranged for StringLikePart<'_> {
    fn range(&self) -> TextRange {
        self.0.range()
    }
}
