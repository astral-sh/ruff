use std::iter::FusedIterator;

use ruff_text_size::{Ranged, TextRange};

use crate::{self as ast, AnyNodeRef, AnyStringFlags, Expr};

/// Unowned pendant to [`ast::Expr`] that stores a reference instead of a owned value.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ExpressionRef<'a, 'ast> {
    BoolOp(&'a ast::ExprBoolOp<'ast>),
    Named(&'a ast::ExprNamed<'ast>),
    BinOp(&'a ast::ExprBinOp<'ast>),
    UnaryOp(&'a ast::ExprUnaryOp<'ast>),
    Lambda(&'a ast::ExprLambda<'ast>),
    If(&'a ast::ExprIf<'ast>),
    Dict(&'a ast::ExprDict<'ast>),
    Set(&'a ast::ExprSet<'ast>),
    ListComp(&'a ast::ExprListComp<'ast>),
    SetComp(&'a ast::ExprSetComp<'ast>),
    DictComp(&'a ast::ExprDictComp<'ast>),
    Generator(&'a ast::ExprGenerator<'ast>),
    Await(&'a ast::ExprAwait<'ast>),
    Yield(&'a ast::ExprYield<'ast>),
    YieldFrom(&'a ast::ExprYieldFrom<'ast>),
    Compare(&'a ast::ExprCompare<'ast>),
    Call(&'a ast::ExprCall<'ast>),
    FString(&'a ast::ExprFString<'ast>),
    StringLiteral(&'a ast::ExprStringLiteral<'ast>),
    BytesLiteral(&'a ast::ExprBytesLiteral<'ast>),
    NumberLiteral(&'a ast::ExprNumberLiteral<'ast>),
    BooleanLiteral(&'a ast::ExprBooleanLiteral),
    NoneLiteral(&'a ast::ExprNoneLiteral),
    EllipsisLiteral(&'a ast::ExprEllipsisLiteral),
    Attribute(&'a ast::ExprAttribute<'ast>),
    Subscript(&'a ast::ExprSubscript<'ast>),
    Starred(&'a ast::ExprStarred<'ast>),
    Name(&'a ast::ExprName<'ast>),
    List(&'a ast::ExprList<'ast>),
    Tuple(&'a ast::ExprTuple<'ast>),
    Slice(&'a ast::ExprSlice<'ast>),
    IpyEscapeCommand(&'a ast::ExprIpyEscapeCommand<'ast>),
}

impl<'a, 'ast> From<&'a Box<Expr<'ast>>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a Box<Expr<'ast>>) -> Self {
        ExpressionRef::from(value.as_ref())
    }
}

impl<'a, 'ast> From<&'a Expr<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a Expr<'ast>) -> Self {
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

impl<'a, 'ast> From<&'a ast::ExprBoolOp<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprBoolOp<'ast>) -> Self {
        Self::BoolOp(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprNamed<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprNamed<'ast>) -> Self {
        Self::Named(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprBinOp<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprBinOp<'ast>) -> Self {
        Self::BinOp(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprUnaryOp<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprUnaryOp<'ast>) -> Self {
        Self::UnaryOp(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprLambda<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprLambda<'ast>) -> Self {
        Self::Lambda(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprIf<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprIf<'ast>) -> Self {
        Self::If(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprDict<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprDict<'ast>) -> Self {
        Self::Dict(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprSet<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprSet<'ast>) -> Self {
        Self::Set(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprListComp<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprListComp<'ast>) -> Self {
        Self::ListComp(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprSetComp<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprSetComp<'ast>) -> Self {
        Self::SetComp(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprDictComp<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprDictComp<'ast>) -> Self {
        Self::DictComp(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprGenerator<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprGenerator<'ast>) -> Self {
        Self::Generator(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprAwait<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprAwait<'ast>) -> Self {
        Self::Await(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprYield<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprYield<'ast>) -> Self {
        Self::Yield(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprYieldFrom<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprYieldFrom<'ast>) -> Self {
        Self::YieldFrom(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprCompare<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprCompare<'ast>) -> Self {
        Self::Compare(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprCall<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprCall<'ast>) -> Self {
        Self::Call(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprFString<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprFString<'ast>) -> Self {
        Self::FString(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprStringLiteral<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprStringLiteral<'ast>) -> Self {
        Self::StringLiteral(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprBytesLiteral<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprBytesLiteral<'ast>) -> Self {
        Self::BytesLiteral(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprNumberLiteral<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprNumberLiteral<'ast>) -> Self {
        Self::NumberLiteral(value)
    }
}
impl<'a> From<&'a ast::ExprBooleanLiteral> for ExpressionRef<'a, '_> {
    fn from(value: &'a ast::ExprBooleanLiteral) -> Self {
        Self::BooleanLiteral(value)
    }
}
impl<'a> From<&'a ast::ExprNoneLiteral> for ExpressionRef<'a, '_> {
    fn from(value: &'a ast::ExprNoneLiteral) -> Self {
        Self::NoneLiteral(value)
    }
}
impl<'a> From<&'a ast::ExprEllipsisLiteral> for ExpressionRef<'a, '_> {
    fn from(value: &'a ast::ExprEllipsisLiteral) -> Self {
        Self::EllipsisLiteral(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprAttribute<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprAttribute<'ast>) -> Self {
        Self::Attribute(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprSubscript<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprSubscript<'ast>) -> Self {
        Self::Subscript(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprStarred<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprStarred<'ast>) -> Self {
        Self::Starred(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprName<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprName<'ast>) -> Self {
        Self::Name(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprList<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprList<'ast>) -> Self {
        Self::List(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprTuple<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprTuple<'ast>) -> Self {
        Self::Tuple(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprSlice<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprSlice<'ast>) -> Self {
        Self::Slice(value)
    }
}
impl<'a, 'ast> From<&'a ast::ExprIpyEscapeCommand<'ast>> for ExpressionRef<'a, 'ast> {
    fn from(value: &'a ast::ExprIpyEscapeCommand<'ast>) -> Self {
        Self::IpyEscapeCommand(value)
    }
}

impl<'a, 'ast> From<ExpressionRef<'a, 'ast>> for AnyNodeRef<'a, 'ast> {
    fn from(value: ExpressionRef<'a, 'ast>) -> Self {
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

impl Ranged for ExpressionRef<'_, '_> {
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
pub enum LiteralExpressionRef<'a, 'ast> {
    StringLiteral(&'a ast::ExprStringLiteral<'ast>),
    BytesLiteral(&'a ast::ExprBytesLiteral<'ast>),
    NumberLiteral(&'a ast::ExprNumberLiteral<'ast>),
    BooleanLiteral(&'a ast::ExprBooleanLiteral),
    NoneLiteral(&'a ast::ExprNoneLiteral),
    EllipsisLiteral(&'a ast::ExprEllipsisLiteral),
}

impl Ranged for LiteralExpressionRef<'_, '_> {
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

impl<'a, 'ast> From<LiteralExpressionRef<'a, 'ast>> for AnyNodeRef<'a, 'ast> {
    fn from(value: LiteralExpressionRef<'a, 'ast>) -> Self {
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

impl LiteralExpressionRef<'_, '_> {
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
pub enum StringLike<'a, 'ast> {
    String(&'a ast::ExprStringLiteral<'ast>),
    Bytes(&'a ast::ExprBytesLiteral<'ast>),
    FString(&'a ast::ExprFString<'ast>),
}

impl<'a, 'ast> StringLike<'a, 'ast> {
    /// Returns an iterator over the [`StringLikePart`] contained in this string-like expression.
    pub fn parts(&self) -> StringLikePartIter<'a, 'ast> {
        match self {
            StringLike::String(expr) => StringLikePartIter::String(expr.value.iter()),
            StringLike::Bytes(expr) => StringLikePartIter::Bytes(expr.value.iter()),
            StringLike::FString(expr) => StringLikePartIter::FString(expr.value.iter()),
        }
    }
}

impl<'a, 'ast> From<&'a ast::ExprStringLiteral<'ast>> for StringLike<'a, 'ast> {
    fn from(value: &'a ast::ExprStringLiteral<'ast>) -> Self {
        StringLike::String(value)
    }
}

impl<'a, 'ast> From<&'a ast::ExprBytesLiteral<'ast>> for StringLike<'a, 'ast> {
    fn from(value: &'a ast::ExprBytesLiteral<'ast>) -> Self {
        StringLike::Bytes(value)
    }
}

impl<'a, 'ast> From<&'a ast::ExprFString<'ast>> for StringLike<'a, 'ast> {
    fn from(value: &'a ast::ExprFString<'ast>) -> Self {
        StringLike::FString(value)
    }
}

impl Ranged for StringLike<'_, '_> {
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
pub enum StringLikePart<'a, 'ast> {
    String(&'a ast::StringLiteral<'ast>),
    Bytes(&'a ast::BytesLiteral<'ast>),
    FString(&'a ast::FString<'ast>),
}

impl StringLikePart<'_, '_> {
    /// Returns the [`AnyStringFlags`] for the current string-like part.
    pub fn flags(&self) -> AnyStringFlags {
        match self {
            StringLikePart::String(string) => AnyStringFlags::from(string.flags),
            StringLikePart::Bytes(bytes) => AnyStringFlags::from(bytes.flags),
            StringLikePart::FString(f_string) => AnyStringFlags::from(f_string.flags),
        }
    }
}

impl<'a, 'ast> From<&'a ast::StringLiteral<'ast>> for StringLikePart<'a, 'ast> {
    fn from(value: &'a ast::StringLiteral<'ast>) -> Self {
        StringLikePart::String(value)
    }
}

impl<'a, 'ast> From<&'a ast::BytesLiteral<'ast>> for StringLikePart<'a, 'ast> {
    fn from(value: &'a ast::BytesLiteral<'ast>) -> Self {
        StringLikePart::Bytes(value)
    }
}

impl<'a, 'ast> From<&'a ast::FString<'ast>> for StringLikePart<'a, 'ast> {
    fn from(value: &'a ast::FString<'ast>) -> Self {
        StringLikePart::FString(value)
    }
}

impl Ranged for StringLikePart<'_, '_> {
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
pub enum StringLikePartIter<'a, 'ast> {
    String(std::slice::Iter<'a, ast::StringLiteral<'ast>>),
    Bytes(std::slice::Iter<'a, ast::BytesLiteral<'ast>>),
    FString(std::slice::Iter<'a, ast::FStringPart<'ast>>),
}

impl<'a, 'ast> Iterator for StringLikePartIter<'a, 'ast> {
    type Item = StringLikePart<'a, 'ast>;

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

impl FusedIterator for StringLikePartIter<'_, '_> {}
impl ExactSizeIterator for StringLikePartIter<'_, '_> {}
