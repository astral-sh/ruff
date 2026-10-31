use crate::StringFlags;
use itertools::Either;
use ruff_text_size::{Ranged, TextRange};
use std::fmt;
use std::ops::Deref;

use super::{ExprKind, Handle, List, View};
use crate::{CmpOp, Expr, ExprCompare, Identifier, NodeKind, Parameter, ParameterWithDefault};

impl<'a> View<'a, Identifier> {
    #[inline]
    pub fn as_str(self) -> &'a str {
        self.id().as_str()
    }

    pub fn is_valid(self) -> bool {
        !self.as_str().is_empty()
    }
}

impl Deref for View<'_, Identifier> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for View<'_, Identifier> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for View<'_, Identifier> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for View<'_, Identifier> {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// A literal expression and its borrowed value in a parsed tree.
#[derive(Debug, Copy, Clone)]
pub enum LiteralExpression<'a> {
    StringLiteral(View<'a, crate::ExprStringLiteral>),
    BytesLiteral(View<'a, crate::ExprBytesLiteral>),
    NumberLiteral(View<'a, crate::ExprNumberLiteral>),
    BooleanLiteral(View<'a, crate::ExprBooleanLiteral>),
    NoneLiteral(View<'a, crate::ExprNoneLiteral>),
    EllipsisLiteral(View<'a, crate::ExprEllipsisLiteral>),
}

impl<'a> LiteralExpression<'a> {
    pub fn view(self) -> View<'a, Expr> {
        match self {
            Self::StringLiteral(node) => node.into(),
            Self::BytesLiteral(node) => node.into(),
            Self::NumberLiteral(node) => node.into(),
            Self::BooleanLiteral(node) => node.into(),
            Self::NoneLiteral(node) => node.into(),
            Self::EllipsisLiteral(node) => node.into(),
        }
    }
    pub fn is_implicit_concatenated(self) -> bool {
        match self {
            Self::StringLiteral(node) => node.value().is_implicit_concatenated(),
            Self::BytesLiteral(node) => node.value().is_implicit_concatenated(),
            _ => false,
        }
    }
}

impl<'a> View<'a, Expr> {
    pub fn as_literal_expr(self) -> Option<LiteralExpression<'a>> {
        Some(match self.classify() {
            ExprKind::StringLiteral(node) => LiteralExpression::StringLiteral(node),
            ExprKind::BytesLiteral(node) => LiteralExpression::BytesLiteral(node),
            ExprKind::NumberLiteral(node) => LiteralExpression::NumberLiteral(node),
            ExprKind::BooleanLiteral(node) => LiteralExpression::BooleanLiteral(node),
            ExprKind::NoneLiteral(node) => LiteralExpression::NoneLiteral(node),
            ExprKind::EllipsisLiteral(node) => LiteralExpression::EllipsisLiteral(node),
            _ => return None,
        })
    }
}

impl View<'_, Expr> {
    pub fn is_literal_expr(self) -> bool {
        matches!(
            self.node_kind(),
            NodeKind::ExprStringLiteral
                | NodeKind::ExprBytesLiteral
                | NodeKind::ExprNumberLiteral
                | NodeKind::ExprBooleanLiteral
                | NodeKind::ExprNoneLiteral
                | NodeKind::ExprEllipsisLiteral
        )
    }

    #[must_use]
    pub fn expression_value(mut self) -> Self {
        while let ExprKind::Named(named) = self.classify() {
            self = named.value();
        }
        self
    }
}

impl<'a> View<'a, ExprCompare> {
    pub fn first_operand(self) -> View<'a, Expr> {
        self.operands().at(0)
    }

    pub fn second_operand(self) -> View<'a, Expr> {
        self.operands().at(1)
    }

    pub fn first_operator(self) -> CmpOp {
        self.ops().at(0)
    }

    pub fn comparators(self) -> List<'a, Handle<Expr>> {
        self.operands().slice(1..self.operands().len())
    }

    pub fn iter(
        self,
    ) -> impl DoubleEndedIterator<Item = (View<'a, Expr>, CmpOp, View<'a, Expr>)> + ExactSizeIterator
    {
        self.operands()
            .iter()
            .zip(self.ops())
            .zip(self.comparators())
            .map(|((left, op), right)| (left, op, right))
    }
}

impl<'a> View<'a, ParameterWithDefault> {
    pub fn name(self) -> View<'a, Identifier> {
        self.parameter().name()
    }

    pub fn annotation(self) -> Option<View<'a, Expr>> {
        self.parameter().annotation()
    }
}

/// The decoded string and the individual tokens of an implicitly concatenated literal.
#[derive(Clone, Copy)]
pub struct StringValue<'a> {
    parts: List<'a, Handle<crate::StringLiteral>>,
    text: u64,
}

impl<'a> StringValue<'a> {
    pub fn as_slice(self) -> List<'a, Handle<crate::StringLiteral>> {
        self.parts
    }
    pub(super) fn new(parts: List<'a, Handle<crate::StringLiteral>>, text: u64) -> Self {
        Self { parts, text }
    }

    #[inline]
    pub fn to_str(self) -> &'a str {
        let [start, len] = super::split_words(self.text);
        let text = if len == u32::from(u16::MAX) {
            self.parts.ast.extra_word::<true>(start as usize)
        } else {
            self.text
        };
        self.parts.ast.string(text)
    }
    pub fn len(self) -> usize {
        self.to_str().len()
    }
    pub fn is_empty(self) -> bool {
        self.to_str().is_empty()
    }
    pub fn chars(self) -> std::str::Chars<'a> {
        self.to_str().chars()
    }
    pub fn is_implicit_concatenated(self) -> bool {
        self.parts.len() > 1
    }
    pub fn first_literal_flags(self) -> crate::StringLiteralFlags {
        self.parts.at(0).flags()
    }
    pub fn is_unicode(self) -> bool {
        self.parts
            .first()
            .is_some_and(|part| part.flags().prefix().is_unicode())
    }
}

impl PartialEq for StringValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.parts == other.parts
    }
}

impl<'a> Deref for StringValue<'a> {
    type Target = List<'a, Handle<crate::StringLiteral>>;
    fn deref(&self) -> &Self::Target {
        &self.parts
    }
}

impl<'a> IntoIterator for StringValue<'a> {
    type Item = View<'a, crate::StringLiteral>;
    type IntoIter = <List<'a, Handle<crate::StringLiteral>> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.parts.into_iter()
    }
}

impl View<'_, crate::Arguments> {
    pub fn len(self) -> usize {
        self.args().len() + self.keywords().len()
    }
    pub fn is_empty(self) -> bool {
        self.args().is_empty() && self.keywords().is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AnyParameter<'a> {
    NonVariadic(View<'a, ParameterWithDefault>),
    Variadic(View<'a, Parameter>),
}

impl<'a> AnyParameter<'a> {
    pub fn as_parameter(self) -> View<'a, Parameter> {
        match self {
            Self::Variadic(node) => node,
            Self::NonVariadic(node) => node.parameter(),
        }
    }

    pub fn name(self) -> View<'a, Identifier> {
        self.as_parameter().name()
    }
    pub fn is_variadic(self) -> bool {
        matches!(self, Self::Variadic(_))
    }
    pub fn annotation(self) -> Option<View<'a, Expr>> {
        self.as_parameter().annotation()
    }
    pub fn default(self) -> Option<View<'a, Expr>> {
        match self {
            Self::NonVariadic(node) => node.default(),
            Self::Variadic(_) => None,
        }
    }
}

impl Ranged for AnyParameter<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::NonVariadic(node) => node.range(),
            Self::Variadic(node) => node.range(),
        }
    }
}

impl<'a> View<'a, crate::Parameters> {
    pub fn iter_source_order(self) -> impl Iterator<Item = AnyParameter<'a>> {
        let mut ordinary = self
            .posonlyargs()
            .iter()
            .chain(self.args())
            .chain(self.kwonlyargs())
            .peekable();
        let mut variadics = [self.vararg(), self.kwarg()];
        variadics.sort_by_key(|param| param.map_or(u32::MAX, |node| u32::from(node.start())));
        let mut variadics = variadics.into_iter().flatten().peekable();
        std::iter::from_fn(move || match (ordinary.peek(), variadics.peek()) {
            (Some(first), Some(variadic)) if variadic.start() < first.start() => {
                variadics.next().map(AnyParameter::Variadic)
            }
            (Some(_), _) => ordinary.next().map(AnyParameter::NonVariadic),
            (None, _) => variadics.next().map(AnyParameter::Variadic),
        })
    }

    pub fn index(self, name: &str) -> Option<usize> {
        self.posonlyargs()
            .iter()
            .chain(self.args())
            .chain(self.kwonlyargs())
            .position(|parameter| parameter.name() == name)
    }

    pub fn includes(self, name: &str) -> bool {
        self.iter().any(|param| param.name() == name)
    }
    #[inline]
    pub fn iter(self) -> impl DoubleEndedIterator<Item = AnyParameter<'a>> {
        if self.has_only_regular_args() {
            return Either::Left(self.args().iter().map(AnyParameter::NonVariadic));
        }
        Either::Right(
            self.posonlyargs()
                .iter()
                .chain(self.args())
                .map(AnyParameter::NonVariadic)
                .chain(self.vararg().map(AnyParameter::Variadic))
                .chain(self.kwonlyargs().iter().map(AnyParameter::NonVariadic))
                .chain(self.kwarg().map(AnyParameter::Variadic)),
        )
    }

    pub fn len(self) -> usize {
        if self.has_only_regular_args() {
            return self.args().len();
        }
        self.posonlyargs().len()
            + self.args().len()
            + self.kwonlyargs().len()
            + usize::from(self.vararg().is_some())
            + usize::from(self.kwarg().is_some())
    }

    pub fn is_empty(self) -> bool {
        self.args().is_empty()
            && (self.has_only_regular_args()
                || (self.posonlyargs().is_empty()
                    && self.kwonlyargs().is_empty()
                    && self.vararg().is_none()
                    && self.kwarg().is_none()))
    }
}

impl<'a> View<'a, crate::TypeParams> {
    pub fn iter(self) -> super::list::Iter<'a, Handle<crate::TypeParam>> {
        self.type_params().iter()
    }
}

impl<'a> View<'a, crate::TypeParam> {
    pub fn name(self) -> View<'a, Identifier> {
        match self.classify() {
            super::TypeParamKind::TypeVar(node) => node.name(),
            super::TypeParamKind::ParamSpec(node) => node.name(),
            super::TypeParamKind::TypeVarTuple(node) => node.name(),
        }
    }

    pub fn default(self) -> Option<View<'a, Expr>> {
        match self.classify() {
            super::TypeParamKind::TypeVar(node) => node.default(),
            super::TypeParamKind::ParamSpec(node) => node.default(),
            super::TypeParamKind::TypeVarTuple(node) => node.default(),
        }
    }
}

impl View<'_, crate::Pattern> {
    pub fn is_irrefutable(self) -> bool {
        match self.classify() {
            super::PatternKind::MatchAs(node) => node.pattern().is_none_or(View::is_irrefutable),
            super::PatternKind::MatchOr(node) => node.patterns().iter().any(View::is_irrefutable),
            _ => false,
        }
    }

    pub fn irrefutable_pattern(self) -> Option<crate::IrrefutablePattern> {
        match self.classify() {
            super::PatternKind::MatchAs(node) => {
                if let Some(pattern) = node.pattern() {
                    return pattern.irrefutable_pattern();
                }
                let kind = node
                    .name()
                    .map_or(crate::IrrefutablePatternKind::Wildcard, |name| {
                        crate::IrrefutablePatternKind::Name(name.id().clone())
                    });
                Some(crate::IrrefutablePattern {
                    kind,
                    range: node.range(),
                    node_index: node.node_index(),
                })
            }
            super::PatternKind::MatchOr(node) => {
                node.patterns().iter().find_map(View::irrefutable_pattern)
            }
            _ => None,
        }
    }
}

impl<'a> View<'a, crate::Arguments> {
    #[inline]
    pub fn find_keyword(self, name: &str) -> Option<View<'a, crate::Keyword>> {
        self.keywords()
            .iter()
            .find(|kw| kw.arg().is_some_and(|arg| arg == name))
    }

    pub fn find_positional(self, position: usize) -> Option<View<'a, Expr>> {
        self.args()
            .iter()
            .take_while(|arg| !arg.is_starred_expr())
            .nth(position)
    }

    pub fn find_argument_value(self, name: &str, position: usize) -> Option<View<'a, Expr>> {
        self.find_keyword(name)
            .map(super::View::<crate::Keyword>::value)
            .or_else(|| self.find_positional(position))
    }
}

macro_rules! literal_value {
    ($name:ident, $element:ty) => {
        #[derive(Clone, Copy, PartialEq)]
        pub struct $name<'a>(List<'a, $element>);

        impl<'a> $name<'a> {
            pub(super) fn new(parts: List<'a, $element>) -> Self {
                Self(parts)
            }
            pub fn as_slice(self) -> List<'a, $element> {
                self.0
            }
            pub fn is_implicit_concatenated(self) -> bool {
                self.0.len() > 1
            }
        }

        impl<'a> Deref for $name<'a> {
            type Target = List<'a, $element>;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<'a> IntoIterator for $name<'a> {
            type Item = <List<'a, $element> as IntoIterator>::Item;
            type IntoIter = <List<'a, $element> as IntoIterator>::IntoIter;
            fn into_iter(self) -> Self::IntoIter {
                self.0.into_iter()
            }
        }
    };
}

literal_value!(BytesValue, Handle<crate::BytesLiteral>);
literal_value!(FStringValue, crate::FStringPart);
literal_value!(TStringValue, Handle<crate::TString>);

impl BytesValue<'_> {
    pub fn is_empty(self) -> bool {
        self.iter().all(|part| part.value().is_empty())
    }
    pub fn len(self) -> usize {
        self.iter().map(|part| part.value().len()).sum()
    }
}

impl<'a> From<BytesValue<'a>> for std::borrow::Cow<'a, [u8]> {
    fn from(value: BytesValue<'a>) -> Self {
        if let Some([part]) = value.0.as_array() {
            Self::Borrowed(part.value())
        } else {
            Self::Owned(value.bytes().collect())
        }
    }
}

impl<'a> FStringValue<'a> {
    pub fn f_strings(self) -> impl Iterator<Item = View<'a, crate::FString>> {
        self.iter().filter_map(|part| match part {
            super::FStringPartView::FString(part) => Some(part),
            super::FStringPartView::Literal(_) => None,
        })
    }
    pub fn literals(self) -> impl Iterator<Item = View<'a, crate::StringLiteral>> {
        self.iter().filter_map(|part| match part {
            super::FStringPartView::Literal(part) => Some(part),
            super::FStringPartView::FString(_) => None,
        })
    }
    pub fn elements(self) -> impl Iterator<Item = View<'a, crate::InterpolatedStringElement>> {
        self.f_strings()
            .flat_map(super::View::<crate::FString>::elements)
    }
}
impl<'a> TStringValue<'a> {
    pub fn elements(self) -> impl Iterator<Item = View<'a, crate::InterpolatedStringElement>> {
        self.iter()
            .flat_map(super::View::<crate::TString>::elements)
    }
}

impl View<'_, crate::Pattern> {
    pub fn is_wildcard(self) -> bool {
        match self.classify() {
            super::PatternKind::MatchAs(node) => node.pattern().is_none_or(View::is_wildcard),
            super::PatternKind::MatchOr(node) => node.patterns().iter().all(View::is_wildcard),
            _ => false,
        }
    }
}

impl<'a> View<'a, crate::StmtClassDef> {
    pub fn bases(self) -> List<'a, Handle<Expr>> {
        self.arguments().map_or_else(
            || List::new(self.ast, 0),
            super::View::<crate::Arguments>::args,
        )
    }
    pub fn keywords(self) -> List<'a, Handle<crate::Keyword>> {
        self.arguments().map_or_else(
            || List::new(self.ast, 0),
            super::View::<crate::Arguments>::keywords,
        )
    }
}
impl<'a> View<'a, crate::ExprStringLiteral> {
    pub fn as_single_part_string(self) -> Option<View<'a, crate::StringLiteral>> {
        self.value().as_array().map(|[part]| part)
    }
}

impl<'a> View<'a, crate::ExprBytesLiteral> {
    pub fn as_single_part_bytestring(self) -> Option<View<'a, crate::BytesLiteral>> {
        self.value().as_array().map(|[part]| part)
    }
}
impl<'a> View<'a, crate::ExprTString> {
    pub fn as_single_part_tstring(self) -> Option<View<'a, crate::TString>> {
        self.value().as_array().map(|[part]| part)
    }
}
impl<'a> View<'a, crate::ExprFString> {
    pub fn as_single_part_fstring(self) -> Option<View<'a, crate::FString>> {
        match self.value().as_array()? {
            [super::FStringPartView::FString(part)] => Some(part),
            [super::FStringPartView::Literal(_)] => None,
        }
    }
}

impl<'a> View<'a, crate::ExprTuple> {
    pub fn iter(self) -> super::list::Iter<'a, Handle<Expr>> {
        self.elts().iter()
    }
}
#[derive(Clone, Copy, Debug)]
pub enum ArgOrKeyword<'a> {
    Arg(View<'a, Expr>),
    Keyword(View<'a, crate::Keyword>),
}
impl<'a> ArgOrKeyword<'a> {
    pub fn value(self) -> View<'a, Expr> {
        match self {
            Self::Arg(expr) => expr,
            Self::Keyword(keyword) => keyword.value(),
        }
    }
    pub fn is_variadic(self) -> bool {
        match self {
            Self::Arg(expr) => expr.is_starred_expr(),
            Self::Keyword(keyword) => keyword.arg().is_none(),
        }
    }
    pub fn as_variadic(self) -> Option<View<'a, crate::Keyword>> {
        self.as_keyword().filter(|keyword| keyword.arg().is_none())
    }
    pub fn as_keyword(self) -> Option<View<'a, crate::Keyword>> {
        match self {
            Self::Arg(_) => None,
            Self::Keyword(keyword) => Some(keyword),
        }
    }
}
impl Ranged for ArgOrKeyword<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Arg(expr) => expr.range(),
            Self::Keyword(keyword) => keyword.range(),
        }
    }
}
impl<'a> View<'a, crate::Arguments> {
    pub fn iter_source_order(self) -> ArgumentsSourceOrder<'a> {
        ArgumentsSourceOrder {
            args: self.args(),
            keywords: self.keywords(),
        }
    }
    pub fn find_argument(self, name: &str, position: usize) -> Option<ArgOrKeyword<'a>> {
        self.find_keyword(name)
            .map(ArgOrKeyword::Keyword)
            .or_else(|| self.find_positional(position).map(ArgOrKeyword::Arg))
    }
}

impl<'a> IntoIterator for View<'a, crate::ExprTuple> {
    type Item = View<'a, Expr>;
    type IntoIter = super::list::Iter<'a, Handle<Expr>>;
    fn into_iter(self) -> Self::IntoIter {
        self.elts().iter()
    }
}
impl<'a> IntoIterator for View<'a, crate::TypeParams> {
    type Item = View<'a, crate::TypeParam>;
    type IntoIter = super::list::Iter<'a, Handle<crate::TypeParam>>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl View<'_, crate::StringLiteral> {
    pub fn content_range(self) -> TextRange {
        self.range()
            .add_start(self.flags().opener_len())
            .sub_end(self.flags().closer_len())
    }
}

impl fmt::Debug for StringValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = fmt::from_fn(|f| match self.parts.as_array() {
            Some([part]) => f.debug_tuple("Single").field(&part).finish(),
            None => f
                .debug_tuple("Concatenated")
                .field(&fmt::from_fn(|f| {
                    f.debug_struct("ConcatenatedStringLiteral")
                        .field("strings", &self.parts)
                        .field("value", &self.to_str())
                        .finish()
                }))
                .finish(),
        });
        f.debug_struct("StringLiteralValue")
            .field("inner", &inner)
            .finish()
    }
}

impl fmt::Debug for FStringValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = fmt::from_fn(|f| match self.0.as_array() {
            Some([super::FStringPartView::FString(part)]) => {
                f.debug_tuple("Single").field(&part).finish()
            }
            _ => f.debug_tuple("Concatenated").field(&self.0).finish(),
        });
        f.debug_struct("FStringValue")
            .field("inner", &inner)
            .finish()
    }
}

impl fmt::Debug for TStringValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = fmt::from_fn(|f| match self.0.as_array() {
            Some([part]) => f.debug_tuple("Single").field(&part).finish(),
            _ => f.debug_tuple("Concatenated").field(&self.0).finish(),
        });
        f.debug_struct("TStringValue")
            .field("inner", &inner)
            .finish()
    }
}

impl fmt::Debug for BytesValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = fmt::from_fn(|f| match self.0.as_array() {
            Some([part]) => f.debug_tuple("Single").field(&part).finish(),
            _ => f.debug_tuple("Concatenated").field(&self.0).finish(),
        });
        f.debug_struct("BytesLiteralValue")
            .field("inner", &inner)
            .finish()
    }
}

/// Merges positional and keyword arguments by their location in the source.
#[derive(Clone)]
pub struct ArgumentsSourceOrder<'a> {
    args: List<'a, Handle<Expr>>,
    keywords: List<'a, Handle<crate::Keyword>>,
}

impl<'a> Iterator for ArgumentsSourceOrder<'a> {
    type Item = ArgOrKeyword<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((arg, rest)) = self.args.split_first()
            && self
                .keywords
                .first()
                .is_none_or(|keyword| arg.start() <= keyword.start())
        {
            self.args = rest;
            Some(ArgOrKeyword::Arg(arg))
        } else {
            let (keyword, rest) = self.keywords.split_first()?;
            self.keywords = rest;
            Some(ArgOrKeyword::Keyword(keyword))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.args.len() + self.keywords.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for ArgumentsSourceOrder<'_> {}
impl std::iter::FusedIterator for ArgumentsSourceOrder<'_> {}

impl View<'_, crate::ExprName> {
    pub fn is_invalid(self) -> bool {
        self.ctx() == crate::ExprContext::Invalid
    }
}

impl<'a> View<'a, crate::StringLiteral> {
    pub fn as_str(self) -> &'a str {
        self.value()
    }
}

impl fmt::Display for StringValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl View<'_, ParameterWithDefault> {
    pub fn uses_pep_484_positional_only_convention(self) -> bool {
        let name = self.name();
        name.starts_with("__") && !name.ends_with("__")
    }
}

impl<'a> IntoIterator for View<'a, crate::ExprDict> {
    type Item = super::DictItemView<'a>;
    type IntoIter = super::list::Iter<'a, crate::DictItem>;
    fn into_iter(self) -> Self::IntoIter {
        self.items().iter()
    }
}

impl<'a> View<'a, crate::ExprDict> {
    pub fn iter_keys(
        self,
    ) -> impl DoubleEndedIterator<Item = Option<View<'a, Expr>>> + ExactSizeIterator {
        self.items().iter().map(|item| item.key)
    }

    pub fn iter_values(
        self,
    ) -> impl DoubleEndedIterator<Item = View<'a, Expr>> + ExactSizeIterator {
        self.items().iter().map(|item| item.value)
    }

    pub fn value(self, index: usize) -> View<'a, Expr> {
        self.items().at(index).value
    }
}

impl View<'_, super::Any> {
    /// The last child of the last branch of a compound statement or clause.
    pub fn last_child_in_body(self) -> Option<Self> {
        match self.classify() {
            super::AnyKind::StmtFunctionDef(node) => node.body().last().map(View::erase),
            super::AnyKind::StmtClassDef(node) => node.body().last().map(View::erase),
            super::AnyKind::StmtWith(node) => node.body().last().map(View::erase),
            super::AnyKind::MatchCase(node) => node.body().last().map(View::erase),
            super::AnyKind::ExceptHandlerExceptHandler(node) => node.body().last().map(View::erase),
            super::AnyKind::ElifElseClause(node) => node.body().last().map(View::erase),
            super::AnyKind::StmtIf(node) => node
                .elif_else_clauses()
                .last()
                .map_or_else(|| node.body(), super::View::<crate::ElifElseClause>::body)
                .last()
                .map(View::erase),
            super::AnyKind::StmtFor(node) => node
                .orelse()
                .last()
                .or_else(|| node.body().last())
                .map(View::erase),
            super::AnyKind::StmtWhile(node) => node
                .orelse()
                .last()
                .or_else(|| node.body().last())
                .map(View::erase),
            super::AnyKind::StmtMatch(node) => node.cases().last().map(View::erase),
            super::AnyKind::StmtTry(node) => node
                .finalbody()
                .last()
                .map(View::erase)
                .or_else(|| node.orelse().last().map(View::erase))
                .or_else(|| node.handlers().last().map(View::erase))
                .or_else(|| node.body().last().map(View::erase)),
            _ => None,
        }
    }
}

impl<'a, T> From<&View<'a, T>> for View<'a, super::Any> {
    fn from(value: &View<'a, T>) -> Self {
        value.erase()
    }
}

impl View<'_, crate::Expr> {
    pub fn precedence(self) -> crate::OperatorPrecedence {
        self.into()
    }
}

impl View<'_, super::Any> {
    pub fn is_alternative_branch_with_node(self) -> bool {
        matches!(
            self.node_kind(),
            NodeKind::ExceptHandlerExceptHandler | NodeKind::ElifElseClause
        )
    }

    pub fn is_first_statement_in_body(self, body: Self) -> bool {
        match body.fields() {
            super::fields::AnyFields::StmtFor(super::fields::StmtFor { body, orelse, .. })
            | super::fields::AnyFields::StmtWhile(super::fields::StmtWhile {
                body, orelse, ..
            }) => {
                body.first().is_some_and(|node| self.is_same_node(node))
                    || orelse.first().is_some_and(|node| self.is_same_node(node))
            }

            super::fields::AnyFields::StmtTry(super::fields::StmtTry {
                body,
                orelse,
                finalbody,
                ..
            }) => {
                body.first().is_some_and(|node| self.is_same_node(node))
                    || orelse.first().is_some_and(|node| self.is_same_node(node))
                    || finalbody
                        .first()
                        .is_some_and(|node| self.is_same_node(node))
            }

            super::fields::AnyFields::StmtIf(super::fields::StmtIf { body, .. })
            | super::fields::AnyFields::ElifElseClause(super::fields::ElifElseClause {
                body,
                ..
            })
            | super::fields::AnyFields::StmtWith(super::fields::StmtWith { body, .. })
            | super::fields::AnyFields::ExceptHandlerExceptHandler(
                super::fields::ExceptHandlerExceptHandler { body, .. },
            )
            | super::fields::AnyFields::MatchCase(super::fields::MatchCase { body, .. })
            | super::fields::AnyFields::StmtFunctionDef(super::fields::StmtFunctionDef {
                body,
                ..
            })
            | super::fields::AnyFields::StmtClassDef(super::fields::StmtClassDef {
                body, ..
            }) => body.first().is_some_and(|node| self.is_same_node(node)),

            super::fields::AnyFields::StmtMatch(super::fields::StmtMatch { cases, .. }) => {
                cases.first().is_some_and(|node| self.is_same_node(node))
            }

            _ => false,
        }
    }

    /// Returns `true` if `statement` is the first statement in an alternate `body` (e.g. the else of an if statement)
    pub fn is_first_statement_in_alternate_body(self, body: Self) -> bool {
        match body.fields() {
            super::fields::AnyFields::StmtFor(super::fields::StmtFor { orelse, .. })
            | super::fields::AnyFields::StmtWhile(super::fields::StmtWhile { orelse, .. }) => {
                orelse.first().is_some_and(|node| self.is_same_node(node))
            }

            super::fields::AnyFields::StmtTry(super::fields::StmtTry {
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                handlers.first().is_some_and(|node| self.is_same_node(node))
                    || orelse.first().is_some_and(|node| self.is_same_node(node))
                    || finalbody
                        .first()
                        .is_some_and(|node| self.is_same_node(node))
            }

            super::fields::AnyFields::StmtIf(super::fields::StmtIf {
                elif_else_clauses, ..
            }) => elif_else_clauses
                .first()
                .is_some_and(|node| self.is_same_node(node)),
            _ => false,
        }
    }
}

impl<'a> List<'a, Handle<crate::InterpolatedStringElement>> {
    pub fn literals(
        self,
    ) -> impl DoubleEndedIterator<Item = View<'a, crate::InterpolatedStringLiteralElement>> {
        self.iter().filter_map(super::View::as_literal)
    }

    pub fn interpolations(
        self,
    ) -> impl DoubleEndedIterator<Item = View<'a, crate::InterpolatedElement>> {
        self.iter().filter_map(super::View::as_interpolation)
    }
}

impl View<'_, crate::BytesLiteral> {
    pub fn content_range(self) -> TextRange {
        self.range()
            .add_start(self.flags().opener_len())
            .sub_end(self.flags().closer_len())
    }
}

impl PartialEq<str> for StringValue<'_> {
    fn eq(&self, rhs: &str) -> bool {
        self.to_str() == rhs
    }
}
impl PartialEq<&str> for StringValue<'_> {
    fn eq(&self, rhs: &&str) -> bool {
        self.to_str() == *rhs
    }
}

impl<'a> View<'a, crate::Parameters> {
    pub fn find(self, name: &str) -> Option<View<'a, ParameterWithDefault>> {
        self.iter_non_variadic_params()
            .find(|parameter| parameter.parameter().name().as_str() == name)
    }
}

impl super::FStringPartView<'_> {
    pub fn quote_style(self) -> crate::str::Quote {
        match self {
            Self::Literal(node) => node.flags().quote_style(),
            Self::FString(node) => node.flags().quote_style(),
        }
    }
}

impl Ranged for super::FStringPartView<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Literal(node) => node.range(),
            Self::FString(node) => node.range(),
        }
    }
}

impl View<'_, crate::TString> {
    pub fn quote_style(self) -> crate::str::Quote {
        self.flags().quote_style()
    }
}

impl TStringValue<'_> {
    pub fn is_empty_iterable(self) -> bool {
        self.iter().all(|node| node.elements().is_empty())
    }
}

impl Deref for View<'_, crate::StringLiteral> {
    type Target = str;
    fn deref(&self) -> &str {
        self.value()
    }
}

impl Deref for View<'_, crate::InterpolatedStringLiteralElement> {
    type Target = str;
    fn deref(&self) -> &str {
        self.value()
    }
}

impl View<'_, crate::Arguments> {
    pub fn l_paren_range(self) -> TextRange {
        TextRange::at(self.start(), 1.into())
    }
    pub fn r_paren_range(self) -> TextRange {
        TextRange::at(self.end() - ruff_text_size::TextSize::new(1), 1.into())
    }
}
