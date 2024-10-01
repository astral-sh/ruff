use std::iter::FusedIterator;

use ruff_text_size::{Ranged, TextRange};

use crate::{self as ast, AnyNodeRef, AnyStringFlags, Expr};

/// Unowned pendant to [`ast::Expr`] that stores a reference instead of a owned value.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ExpressionRef<'a> {
    BoolOp(&'a ast::ExprBoolOp),
    Named(&'a ast::ExprNamed),
    BinOp(&'a ast::ExprBinOp),
    UnaryOp(&'a ast::ExprUnaryOp),
    Lambda(&'a ast::ExprLambda),
    If(&'a ast::ExprIf),
    Dict(&'a ast::ExprDict),
    Set(&'a ast::ExprSet),
    ListComp(&'a ast::ExprListComp),
    SetComp(&'a ast::ExprSetComp),
    DictComp(&'a ast::ExprDictComp),
    Generator(&'a ast::ExprGenerator),
    Await(&'a ast::ExprAwait),
    Yield(&'a ast::ExprYield),
    YieldFrom(&'a ast::ExprYieldFrom),
    Compare(&'a ast::ExprCompare),
    Call(&'a ast::ExprCall),
    FString(&'a ast::ExprFString),
    StringLiteral(&'a ast::ExprStringLiteral),
    BytesLiteral(&'a ast::ExprBytesLiteral),
    NumberLiteral(&'a ast::ExprNumberLiteral),
    BooleanLiteral(&'a ast::ExprBooleanLiteral),
    NoneLiteral(&'a ast::ExprNoneLiteral),
    EllipsisLiteral(&'a ast::ExprEllipsisLiteral),
    Attribute(&'a ast::ExprAttribute),
    Subscript(&'a ast::ExprSubscript),
    Starred(&'a ast::ExprStarred),
    Name(&'a ast::ExprName),
    List(&'a ast::ExprList),
    Tuple(&'a ast::ExprTuple),
    Slice(&'a ast::ExprSlice),
    IpyEscapeCommand(&'a ast::ExprIpyEscapeCommand),
}

impl<'a> From<&'a Box<Expr>> for ExpressionRef<'a> {
    fn from(value: &'a Box<Expr>) -> Self {
        ExpressionRef::from(value.as_ref())
    }
}

impl<'a> From<&'a Expr> for ExpressionRef<'a> {
    fn from(value: &'a Expr) -> Self {
        match value {
            Expr::BoolOp(value) => ExpressionRef::BoolOp(value),
            Expr::Named(value) => ExpressionRef::Named(value),
            Expr::BinOp(value) => ExpressionRef::BinOp(value),
            Expr::UnaryOp(value) => ExpressionRef::UnaryOp(value),
            Expr::Lambda(value) => ExpressionRef::Lambda(value),
            Expr::If(value) => ExpressionRef::If(value),
            Expr::Dict(value) => ExpressionRef::Dict(value),
            Expr::Set(value) => ExpressionRef::Set(value),
            Expr::ListComp(value) => ExpressionRef::ListComp(value),
            Expr::SetComp(value) => ExpressionRef::SetComp(value),
            Expr::DictComp(value) => ExpressionRef::DictComp(value),
            Expr::Generator(value) => ExpressionRef::Generator(value),
            Expr::Await(value) => ExpressionRef::Await(value),
            Expr::Yield(value) => ExpressionRef::Yield(value),
            Expr::YieldFrom(value) => ExpressionRef::YieldFrom(value),
            Expr::Compare(value) => ExpressionRef::Compare(value),
            Expr::Call(value) => ExpressionRef::Call(value),
            Expr::FString(value) => ExpressionRef::FString(value),
            Expr::StringLiteral(value) => ExpressionRef::StringLiteral(value),
            Expr::BytesLiteral(value) => ExpressionRef::BytesLiteral(value),
            Expr::NumberLiteral(value) => ExpressionRef::NumberLiteral(value),
            Expr::BooleanLiteral(value) => ExpressionRef::BooleanLiteral(value),
            Expr::NoneLiteral(value) => ExpressionRef::NoneLiteral(value),
            Expr::EllipsisLiteral(value) => ExpressionRef::EllipsisLiteral(value),
            Expr::Attribute(value) => ExpressionRef::Attribute(value),
            Expr::Subscript(value) => ExpressionRef::Subscript(value),
            Expr::Starred(value) => ExpressionRef::Starred(value),
            Expr::Name(value) => ExpressionRef::Name(value),
            Expr::List(value) => ExpressionRef::List(value),
            Expr::Tuple(value) => ExpressionRef::Tuple(value),
            Expr::Slice(value) => ExpressionRef::Slice(value),
            Expr::IpyEscapeCommand(value) => ExpressionRef::IpyEscapeCommand(value),
        }
    }
}

impl<'a> From<&'a ast::ExprBoolOp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprBoolOp) -> Self {
        Self::BoolOp(value)
    }
}
impl<'a> From<&'a ast::ExprNamed> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprNamed) -> Self {
        Self::Named(value)
    }
}
impl<'a> From<&'a ast::ExprBinOp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprBinOp) -> Self {
        Self::BinOp(value)
    }
}
impl<'a> From<&'a ast::ExprUnaryOp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprUnaryOp) -> Self {
        Self::UnaryOp(value)
    }
}
impl<'a> From<&'a ast::ExprLambda> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprLambda) -> Self {
        Self::Lambda(value)
    }
}
impl<'a> From<&'a ast::ExprIf> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprIf) -> Self {
        Self::If(value)
    }
}
impl<'a> From<&'a ast::ExprDict> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprDict) -> Self {
        Self::Dict(value)
    }
}
impl<'a> From<&'a ast::ExprSet> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprSet) -> Self {
        Self::Set(value)
    }
}
impl<'a> From<&'a ast::ExprListComp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprListComp) -> Self {
        Self::ListComp(value)
    }
}
impl<'a> From<&'a ast::ExprSetComp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprSetComp) -> Self {
        Self::SetComp(value)
    }
}
impl<'a> From<&'a ast::ExprDictComp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprDictComp) -> Self {
        Self::DictComp(value)
    }
}
impl<'a> From<&'a ast::ExprGenerator> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprGenerator) -> Self {
        Self::Generator(value)
    }
}
impl<'a> From<&'a ast::ExprAwait> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprAwait) -> Self {
        Self::Await(value)
    }
}
impl<'a> From<&'a ast::ExprYield> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprYield) -> Self {
        Self::Yield(value)
    }
}
impl<'a> From<&'a ast::ExprYieldFrom> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprYieldFrom) -> Self {
        Self::YieldFrom(value)
    }
}
impl<'a> From<&'a ast::ExprCompare> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprCompare) -> Self {
        Self::Compare(value)
    }
}
impl<'a> From<&'a ast::ExprCall> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprCall) -> Self {
        Self::Call(value)
    }
}
impl<'a> From<&'a ast::ExprFString> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprFString) -> Self {
        Self::FString(value)
    }
}
impl<'a> From<&'a ast::ExprStringLiteral> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprStringLiteral) -> Self {
        Self::StringLiteral(value)
    }
}
impl<'a> From<&'a ast::ExprBytesLiteral> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprBytesLiteral) -> Self {
        Self::BytesLiteral(value)
    }
}
impl<'a> From<&'a ast::ExprNumberLiteral> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprNumberLiteral) -> Self {
        Self::NumberLiteral(value)
    }
}
impl<'a> From<&'a ast::ExprBooleanLiteral> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprBooleanLiteral) -> Self {
        Self::BooleanLiteral(value)
    }
}
impl<'a> From<&'a ast::ExprNoneLiteral> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprNoneLiteral) -> Self {
        Self::NoneLiteral(value)
    }
}
impl<'a> From<&'a ast::ExprEllipsisLiteral> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprEllipsisLiteral) -> Self {
        Self::EllipsisLiteral(value)
    }
}
impl<'a> From<&'a ast::ExprAttribute> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprAttribute) -> Self {
        Self::Attribute(value)
    }
}
impl<'a> From<&'a ast::ExprSubscript> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprSubscript) -> Self {
        Self::Subscript(value)
    }
}
impl<'a> From<&'a ast::ExprStarred> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprStarred) -> Self {
        Self::Starred(value)
    }
}
impl<'a> From<&'a ast::ExprName> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprName) -> Self {
        Self::Name(value)
    }
}
impl<'a> From<&'a ast::ExprList> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprList) -> Self {
        Self::List(value)
    }
}
impl<'a> From<&'a ast::ExprTuple> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprTuple) -> Self {
        Self::Tuple(value)
    }
}
impl<'a> From<&'a ast::ExprSlice> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprSlice) -> Self {
        Self::Slice(value)
    }
}
impl<'a> From<&'a ast::ExprIpyEscapeCommand> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprIpyEscapeCommand) -> Self {
        Self::IpyEscapeCommand(value)
    }
}

impl<'a> From<ExpressionRef<'a>> for AnyNodeRef<'a> {
    fn from(value: ExpressionRef<'a>) -> Self {
        match value {
            ExpressionRef::BoolOp(expression) => AnyNodeRef::ExprBoolOp(expression),
            ExpressionRef::Named(expression) => AnyNodeRef::ExprNamed(expression),
            ExpressionRef::BinOp(expression) => AnyNodeRef::ExprBinOp(expression),
            ExpressionRef::UnaryOp(expression) => AnyNodeRef::ExprUnaryOp(expression),
            ExpressionRef::Lambda(expression) => AnyNodeRef::ExprLambda(expression),
            ExpressionRef::If(expression) => AnyNodeRef::ExprIf(expression),
            ExpressionRef::Dict(expression) => AnyNodeRef::ExprDict(expression),
            ExpressionRef::Set(expression) => AnyNodeRef::ExprSet(expression),
            ExpressionRef::ListComp(expression) => AnyNodeRef::ExprListComp(expression),
            ExpressionRef::SetComp(expression) => AnyNodeRef::ExprSetComp(expression),
            ExpressionRef::DictComp(expression) => AnyNodeRef::ExprDictComp(expression),
            ExpressionRef::Generator(expression) => AnyNodeRef::ExprGenerator(expression),
            ExpressionRef::Await(expression) => AnyNodeRef::ExprAwait(expression),
            ExpressionRef::Yield(expression) => AnyNodeRef::ExprYield(expression),
            ExpressionRef::YieldFrom(expression) => AnyNodeRef::ExprYieldFrom(expression),
            ExpressionRef::Compare(expression) => AnyNodeRef::ExprCompare(expression),
            ExpressionRef::Call(expression) => AnyNodeRef::ExprCall(expression),
            ExpressionRef::FString(expression) => AnyNodeRef::ExprFString(expression),
            ExpressionRef::StringLiteral(expression) => AnyNodeRef::ExprStringLiteral(expression),
            ExpressionRef::BytesLiteral(expression) => AnyNodeRef::ExprBytesLiteral(expression),
            ExpressionRef::NumberLiteral(expression) => AnyNodeRef::ExprNumberLiteral(expression),
            ExpressionRef::BooleanLiteral(expression) => AnyNodeRef::ExprBooleanLiteral(expression),
            ExpressionRef::NoneLiteral(expression) => AnyNodeRef::ExprNoneLiteral(expression),
            ExpressionRef::EllipsisLiteral(expression) => {
                AnyNodeRef::ExprEllipsisLiteral(expression)
            }
            ExpressionRef::Attribute(expression) => AnyNodeRef::ExprAttribute(expression),
            ExpressionRef::Subscript(expression) => AnyNodeRef::ExprSubscript(expression),
            ExpressionRef::Starred(expression) => AnyNodeRef::ExprStarred(expression),
            ExpressionRef::Name(expression) => AnyNodeRef::ExprName(expression),
            ExpressionRef::List(expression) => AnyNodeRef::ExprList(expression),
            ExpressionRef::Tuple(expression) => AnyNodeRef::ExprTuple(expression),
            ExpressionRef::Slice(expression) => AnyNodeRef::ExprSlice(expression),
            ExpressionRef::IpyEscapeCommand(expression) => {
                AnyNodeRef::ExprIpyEscapeCommand(expression)
            }
        }
    }
}

impl Ranged for ExpressionRef<'_> {
    fn range(&self) -> TextRange {
        match self {
            ExpressionRef::BoolOp(expression) => expression.range(),
            ExpressionRef::Named(expression) => expression.range(),
            ExpressionRef::BinOp(expression) => expression.range(),
            ExpressionRef::UnaryOp(expression) => expression.range(),
            ExpressionRef::Lambda(expression) => expression.range(),
            ExpressionRef::If(expression) => expression.range(),
            ExpressionRef::Dict(expression) => expression.range(),
            ExpressionRef::Set(expression) => expression.range(),
            ExpressionRef::ListComp(expression) => expression.range(),
            ExpressionRef::SetComp(expression) => expression.range(),
            ExpressionRef::DictComp(expression) => expression.range(),
            ExpressionRef::Generator(expression) => expression.range(),
            ExpressionRef::Await(expression) => expression.range(),
            ExpressionRef::Yield(expression) => expression.range(),
            ExpressionRef::YieldFrom(expression) => expression.range(),
            ExpressionRef::Compare(expression) => expression.range(),
            ExpressionRef::Call(expression) => expression.range(),
            ExpressionRef::FString(expression) => expression.range(),
            ExpressionRef::StringLiteral(expression) => expression.range(),
            ExpressionRef::BytesLiteral(expression) => expression.range(),
            ExpressionRef::NumberLiteral(expression) => expression.range(),
            ExpressionRef::BooleanLiteral(expression) => expression.range(),
            ExpressionRef::NoneLiteral(expression) => expression.range(),
            ExpressionRef::EllipsisLiteral(expression) => expression.range(),
            ExpressionRef::Attribute(expression) => expression.range(),
            ExpressionRef::Subscript(expression) => expression.range(),
            ExpressionRef::Starred(expression) => expression.range(),
            ExpressionRef::Name(expression) => expression.range(),
            ExpressionRef::List(expression) => expression.range(),
            ExpressionRef::Tuple(expression) => expression.range(),
            ExpressionRef::Slice(expression) => expression.range(),
            ExpressionRef::IpyEscapeCommand(expression) => expression.range(),
        }
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
    /// Returns an iterator over the [`StringLikePart`] contained in this string-like expression.
    pub fn parts(&self) -> StringLikePartIter<'_> {
        match self {
            StringLike::String(expr) => StringLikePartIter::String(expr.value.iter()),
            StringLike::Bytes(expr) => StringLikePartIter::Bytes(expr.value.iter()),
            StringLike::FString(expr) => StringLikePartIter::FString(expr.value.iter()),
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

impl StringLikePart<'_> {
    /// Returns the [`AnyStringFlags`] for the current string-like part.
    pub fn flags(&self) -> AnyStringFlags {
        match self {
            StringLikePart::String(string) => AnyStringFlags::from(string.flags),
            StringLikePart::Bytes(bytes) => AnyStringFlags::from(bytes.flags),
            StringLikePart::FString(f_string) => AnyStringFlags::from(f_string.flags),
        }
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

impl FusedIterator for StringLikePartIter<'_> {}
impl ExactSizeIterator for StringLikePartIter<'_> {}
