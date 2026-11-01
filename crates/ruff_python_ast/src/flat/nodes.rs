use crate::StringFlags;
use itertools::Either;
use ruff_text_size::{Ranged, TextRange};
use std::fmt;
use std::ops::Deref;

use super::{AstNode, AstNodeId, ExprKind, List};
use crate::{CmpOp, Expr, ExprCompare, Identifier, NodeKind, Parameter, ParameterWithDefault};

impl<'a> Identifier<'a> {
    #[inline]
    pub fn as_str(self) -> &'a str {
        self.id().as_str()
    }

    pub fn is_valid(self) -> bool {
        !self.as_str().is_empty()
    }
}

impl Deref for Identifier<'_> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for Identifier<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for Identifier<'_> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Identifier<'_> {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// A literal expression and its borrowed value in a parsed tree.
#[derive(Debug, Copy, Clone)]
pub enum LiteralExpression<'a> {
    StringLiteral(crate::ExprStringLiteral<'a>),
    BytesLiteral(crate::ExprBytesLiteral<'a>),
    NumberLiteral(crate::ExprNumberLiteral<'a>),
    BooleanLiteral(crate::ExprBooleanLiteral<'a>),
    NoneLiteral(crate::ExprNoneLiteral<'a>),
    EllipsisLiteral(crate::ExprEllipsisLiteral<'a>),
}

impl<'a> LiteralExpression<'a> {
    pub fn view(self) -> Expr<'a> {
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

impl<'a> Expr<'a> {
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

impl Expr<'_> {
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

impl<'a> ExprCompare<'a> {
    pub fn first_operand(self) -> Expr<'a> {
        self.operands().at(0)
    }

    pub fn second_operand(self) -> Expr<'a> {
        self.operands().at(1)
    }

    pub fn first_operator(self) -> CmpOp {
        self.ops().at(0)
    }

    pub fn comparators(self) -> List<'a, AstNodeId<crate::kind::Expr>> {
        self.operands().slice(1..self.operands().len())
    }

    pub fn iter(
        self,
    ) -> impl DoubleEndedIterator<Item = (Expr<'a>, CmpOp, Expr<'a>)> + ExactSizeIterator {
        self.operands()
            .iter()
            .zip(self.ops())
            .zip(self.comparators())
            .map(|((left, op), right)| (left, op, right))
    }
}

impl<'a> ParameterWithDefault<'a> {
    pub fn name(self) -> Identifier<'a> {
        self.parameter().name()
    }

    pub fn annotation(self) -> Option<Expr<'a>> {
        self.parameter().annotation()
    }
}

/// The decoded string and the individual tokens of an implicitly concatenated literal.
#[derive(Clone, Copy)]
pub struct StringValue<'a> {
    parts: List<'a, AstNodeId<crate::kind::StringLiteral>>,
    text: u64,
}

impl<'a> StringValue<'a> {
    pub fn as_slice(self) -> List<'a, AstNodeId<crate::kind::StringLiteral>> {
        self.parts
    }
    pub(super) fn new(parts: List<'a, AstNodeId<crate::kind::StringLiteral>>, text: u64) -> Self {
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
    type Target = List<'a, AstNodeId<crate::kind::StringLiteral>>;
    fn deref(&self) -> &Self::Target {
        &self.parts
    }
}

impl<'a> IntoIterator for StringValue<'a> {
    type Item = crate::StringLiteral<'a>;
    type IntoIter = <List<'a, AstNodeId<crate::kind::StringLiteral>> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.parts.into_iter()
    }
}

impl crate::Arguments<'_> {
    pub fn len(self) -> usize {
        self.args().len() + self.keywords().len()
    }
    pub fn is_empty(self) -> bool {
        self.args().is_empty() && self.keywords().is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AnyParameter<'a> {
    NonVariadic(ParameterWithDefault<'a>),
    Variadic(Parameter<'a>),
}

impl<'a> AnyParameter<'a> {
    pub fn as_parameter(self) -> Parameter<'a> {
        match self {
            Self::Variadic(node) => node,
            Self::NonVariadic(node) => node.parameter(),
        }
    }

    pub fn name(self) -> Identifier<'a> {
        self.as_parameter().name()
    }
    pub fn is_variadic(self) -> bool {
        matches!(self, Self::Variadic(_))
    }
    pub fn annotation(self) -> Option<Expr<'a>> {
        self.as_parameter().annotation()
    }
    pub fn default(self) -> Option<Expr<'a>> {
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

impl<'a> crate::Parameters<'a> {
    pub fn iter_source_order(self) -> impl Iterator<Item = AnyParameter<'a>> {
        self.ordered_items().iter().map(|node| {
            if let Some(parameter) = node.cast::<crate::Parameter>() {
                AnyParameter::Variadic(parameter)
            } else {
                AnyParameter::NonVariadic(node.retype())
            }
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

impl<'a> crate::TypeParams<'a> {
    pub fn iter(self) -> super::list::Iter<'a, AstNodeId<crate::kind::TypeParam>> {
        self.type_params().iter()
    }
}

impl<'a> crate::TypeParam<'a> {
    pub fn name(self) -> Identifier<'a> {
        match self.classify() {
            super::TypeParamKind::TypeVar(node) => node.name(),
            super::TypeParamKind::ParamSpec(node) => node.name(),
            super::TypeParamKind::TypeVarTuple(node) => node.name(),
        }
    }

    pub fn default(self) -> Option<Expr<'a>> {
        match self.classify() {
            super::TypeParamKind::TypeVar(node) => node.default(),
            super::TypeParamKind::ParamSpec(node) => node.default(),
            super::TypeParamKind::TypeVarTuple(node) => node.default(),
        }
    }
}

impl<'a> crate::Pattern<'a> {
    /// The name bound by this pattern itself, excluding nested patterns.
    pub fn capture_name(self) -> Option<Identifier<'a>> {
        match self.classify() {
            super::PatternKind::MatchAs(node) => node.name(),
            super::PatternKind::MatchStar(node) => node.name(),
            super::PatternKind::MatchMapping(node) => node.items().rest(),
            _ => None,
        }
    }

    pub fn is_irrefutable(self) -> bool {
        match self.classify() {
            super::PatternKind::MatchAs(node) => node
                .pattern()
                .is_none_or(super::super::generated::Pattern::is_irrefutable),
            super::PatternKind::MatchOr(node) => node
                .patterns()
                .iter()
                .any(super::super::generated::Pattern::is_irrefutable),
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
            super::PatternKind::MatchOr(node) => node
                .patterns()
                .iter()
                .find_map(super::super::generated::Pattern::irrefutable_pattern),
            _ => None,
        }
    }
}

impl<'a> crate::PatternMatchMapping<'a> {
    /// The final `**name` capture. Valid mapping patterns contain at most one.
    pub fn rest(self) -> Option<Identifier<'a>> {
        self.items().rest()
    }
}

impl<'a> crate::Arguments<'a> {
    #[inline]
    pub fn find_keyword(self, name: &str) -> Option<crate::Keyword<'a>> {
        self.keywords()
            .iter()
            .find(|kw| kw.arg().is_some_and(|arg| arg == name))
    }

    pub fn find_positional(self, position: usize) -> Option<Expr<'a>> {
        self.args()
            .iter()
            .take_while(|arg| !arg.is_starred_expr())
            .nth(position)
    }

    pub fn find_argument_value(self, name: &str, position: usize) -> Option<Expr<'a>> {
        self.find_keyword(name)
            .map(crate::Keyword::value)
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

literal_value!(BytesValue, AstNodeId<crate::kind::BytesLiteral>);
literal_value!(FStringValue, crate::FStringPart);
literal_value!(TStringValue, AstNodeId<crate::kind::TString>);

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
    pub fn f_strings(self) -> impl Iterator<Item = crate::FString<'a>> {
        self.iter().filter_map(|part| match part {
            super::FStringPartView::FString(part) => Some(part),
            super::FStringPartView::Literal(_) => None,
        })
    }
    pub fn literals(self) -> impl Iterator<Item = crate::StringLiteral<'a>> {
        self.iter().filter_map(|part| match part {
            super::FStringPartView::Literal(part) => Some(part),
            super::FStringPartView::FString(_) => None,
        })
    }
    pub fn elements(self) -> impl Iterator<Item = crate::InterpolatedStringElement<'a>> {
        self.f_strings().flat_map(crate::FString::elements)
    }
}
impl<'a> TStringValue<'a> {
    pub fn elements(self) -> impl Iterator<Item = crate::InterpolatedStringElement<'a>> {
        self.iter().flat_map(crate::TString::elements)
    }
}

impl crate::Pattern<'_> {
    pub fn is_wildcard(self) -> bool {
        match self.classify() {
            super::PatternKind::MatchAs(node) => node
                .pattern()
                .is_none_or(super::super::generated::Pattern::is_wildcard),
            super::PatternKind::MatchOr(node) => node
                .patterns()
                .iter()
                .all(super::super::generated::Pattern::is_wildcard),
            _ => false,
        }
    }
}

impl<'a> crate::StmtClassDef<'a> {
    pub fn bases(self) -> List<'a, AstNodeId<crate::kind::Expr>> {
        self.arguments()
            .map_or_else(|| List::new(self.syntax.ast, 0), crate::Arguments::args)
    }
    pub fn keywords(self) -> List<'a, AstNodeId<crate::kind::Keyword>> {
        self.arguments()
            .map_or_else(|| List::new(self.syntax.ast, 0), crate::Arguments::keywords)
    }
}
impl<'a> crate::ExprStringLiteral<'a> {
    pub fn as_single_part_string(self) -> Option<crate::StringLiteral<'a>> {
        self.value().as_array().map(|[part]| part)
    }
}

impl<'a> crate::ExprBytesLiteral<'a> {
    pub fn as_single_part_bytestring(self) -> Option<crate::BytesLiteral<'a>> {
        self.value().as_array().map(|[part]| part)
    }
}
impl<'a> crate::ExprTString<'a> {
    pub fn as_single_part_tstring(self) -> Option<crate::TString<'a>> {
        self.value().as_array().map(|[part]| part)
    }
}
impl<'a> crate::ExprFString<'a> {
    pub fn as_single_part_fstring(self) -> Option<crate::FString<'a>> {
        match self.value().as_array()? {
            [super::FStringPartView::FString(part)] => Some(part),
            [super::FStringPartView::Literal(_)] => None,
        }
    }
}

impl<'a> crate::ExprTuple<'a> {
    pub fn iter(self) -> super::list::Iter<'a, AstNodeId<crate::kind::Expr>> {
        self.elts().iter()
    }
}
#[derive(Clone, Copy, Debug)]
pub enum ArgOrKeyword<'a> {
    Arg(Expr<'a>),
    Keyword(crate::Keyword<'a>),
}
impl<'a> ArgOrKeyword<'a> {
    pub fn value(self) -> Expr<'a> {
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
    pub fn as_variadic(self) -> Option<crate::Keyword<'a>> {
        self.as_keyword().filter(|keyword| keyword.arg().is_none())
    }
    pub fn as_keyword(self) -> Option<crate::Keyword<'a>> {
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
impl<'a> crate::Arguments<'a> {
    pub fn iter_source_order(self) -> ArgumentsSourceOrder<'a> {
        ArgumentsSourceOrder(self.ordered_items().iter())
    }
    pub fn find_argument(self, name: &str, position: usize) -> Option<ArgOrKeyword<'a>> {
        self.find_keyword(name)
            .map(ArgOrKeyword::Keyword)
            .or_else(|| self.find_positional(position).map(ArgOrKeyword::Arg))
    }
}

impl<'a> IntoIterator for crate::ExprTuple<'a> {
    type Item = Expr<'a>;
    type IntoIter = super::list::Iter<'a, AstNodeId<crate::kind::Expr>>;
    fn into_iter(self) -> Self::IntoIter {
        self.elts().iter()
    }
}
impl<'a> IntoIterator for crate::TypeParams<'a> {
    type Item = crate::TypeParam<'a>;
    type IntoIter = super::list::Iter<'a, AstNodeId<crate::kind::TypeParam>>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl crate::StringLiteral<'_> {
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

/// Call arguments in their order in the source.
#[derive(Clone)]
pub struct ArgumentsSourceOrder<'a>(super::list::Iter<'a, AstNodeId<crate::kind::Any>>);

impl<'a> Iterator for ArgumentsSourceOrder<'a> {
    type Item = ArgOrKeyword<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|node| {
            if let Some(keyword) = node.cast::<crate::Keyword>() {
                ArgOrKeyword::Keyword(keyword)
            } else {
                ArgOrKeyword::Arg(node.retype())
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for ArgumentsSourceOrder<'_> {}
impl std::iter::FusedIterator for ArgumentsSourceOrder<'_> {}

impl crate::ExprName<'_> {
    pub fn is_invalid(self) -> bool {
        self.ctx() == crate::ExprContext::Invalid
    }
}

impl<'a> crate::StringLiteral<'a> {
    pub fn as_str(self) -> &'a str {
        self.value()
    }
}

impl fmt::Display for StringValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl ParameterWithDefault<'_> {
    pub fn uses_pep_484_positional_only_convention(self) -> bool {
        let name = self.name();
        name.starts_with("__") && !name.ends_with("__")
    }
}

impl<'a> IntoIterator for crate::ExprDict<'a> {
    type Item = crate::DictItem<'a>;
    type IntoIter = super::list::Iter<'a, crate::AstNodeId<crate::kind::DictItem>>;
    fn into_iter(self) -> Self::IntoIter {
        self.items().iter()
    }
}

impl<'a> crate::ExprDict<'a> {
    pub fn iter_keys(
        self,
    ) -> impl DoubleEndedIterator<Item = Option<Expr<'a>>> + ExactSizeIterator {
        self.items().iter().map(crate::DictItem::key)
    }

    pub fn iter_values(self) -> impl DoubleEndedIterator<Item = Expr<'a>> + ExactSizeIterator {
        self.items().iter().map(crate::DictItem::value)
    }
}

impl crate::SyntaxNode<'_> {
    /// The last child of the last branch of a compound statement or clause.
    pub fn last_child_in_body(self) -> Option<Self> {
        match self.classify() {
            super::AnyKind::StmtFunctionDef(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyKind::StmtClassDef(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyKind::StmtWith(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyKind::MatchCase(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyKind::ExceptHandlerExceptHandler(node) => {
                node.body().last().map(AstNode::syntax_node)
            }
            super::AnyKind::ElifElseClause(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyKind::StmtIf(node) => node
                .elif_else_clauses()
                .last()
                .map_or_else(|| node.body(), crate::ElifElseClause::body)
                .last()
                .map(AstNode::syntax_node),
            super::AnyKind::StmtFor(node) => node
                .orelse()
                .last()
                .or_else(|| node.body().last())
                .map(AstNode::syntax_node),
            super::AnyKind::StmtWhile(node) => node
                .orelse()
                .last()
                .or_else(|| node.body().last())
                .map(AstNode::syntax_node),
            super::AnyKind::StmtMatch(node) => node.cases().last().map(AstNode::syntax_node),
            super::AnyKind::StmtTry(node) => node
                .finalbody()
                .last()
                .map(AstNode::syntax_node)
                .or_else(|| node.orelse().last().map(AstNode::syntax_node))
                .or_else(|| node.handlers().last().map(AstNode::syntax_node))
                .or_else(|| node.body().last().map(AstNode::syntax_node)),
            _ => None,
        }
    }
}

impl<'a, N: AstNode<'a>> From<&N> for crate::SyntaxNode<'a> {
    fn from(value: &N) -> Self {
        value.syntax_node()
    }
}

impl crate::Expr<'_> {
    pub fn precedence(self) -> crate::OperatorPrecedence {
        self.into()
    }
}

impl crate::SyntaxNode<'_> {
    pub fn is_alternative_branch_with_node(self) -> bool {
        matches!(
            self.node_kind(),
            NodeKind::ExceptHandlerExceptHandler | NodeKind::ElifElseClause
        )
    }

    pub fn is_first_statement_in_body(self, body: Self) -> bool {
        match body.fields() {
            crate::AnyFields::StmtFor(crate::StmtForFields { body, orelse, .. })
            | crate::AnyFields::StmtWhile(crate::StmtWhileFields { body, orelse, .. }) => {
                body.first().is_some_and(|node| self.is_same_node(node))
                    || orelse.first().is_some_and(|node| self.is_same_node(node))
            }

            crate::AnyFields::StmtTry(crate::StmtTryFields {
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

            crate::AnyFields::StmtIf(crate::StmtIfFields { body, .. })
            | crate::AnyFields::ElifElseClause(crate::ElifElseClauseFields { body, .. })
            | crate::AnyFields::StmtWith(crate::StmtWithFields { body, .. })
            | crate::AnyFields::ExceptHandlerExceptHandler(
                crate::ExceptHandlerExceptHandlerFields { body, .. },
            )
            | crate::AnyFields::MatchCase(crate::MatchCaseFields { body, .. })
            | crate::AnyFields::StmtFunctionDef(crate::StmtFunctionDefFields { body, .. })
            | crate::AnyFields::StmtClassDef(crate::StmtClassDefFields { body, .. }) => {
                body.first().is_some_and(|node| self.is_same_node(node))
            }

            crate::AnyFields::StmtMatch(crate::StmtMatchFields { cases, .. }) => {
                cases.first().is_some_and(|node| self.is_same_node(node))
            }

            _ => false,
        }
    }

    /// Returns `true` if `statement` is the first statement in an alternate `body` (e.g. the else of an if statement)
    pub fn is_first_statement_in_alternate_body(self, body: Self) -> bool {
        match body.fields() {
            crate::AnyFields::StmtFor(crate::StmtForFields { orelse, .. })
            | crate::AnyFields::StmtWhile(crate::StmtWhileFields { orelse, .. }) => {
                orelse.first().is_some_and(|node| self.is_same_node(node))
            }

            crate::AnyFields::StmtTry(crate::StmtTryFields {
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

            crate::AnyFields::StmtIf(crate::StmtIfFields {
                elif_else_clauses, ..
            }) => elif_else_clauses
                .first()
                .is_some_and(|node| self.is_same_node(node)),
            _ => false,
        }
    }
}

impl<'a> List<'a, AstNodeId<crate::kind::InterpolatedStringElement>> {
    pub fn literals(
        self,
    ) -> impl DoubleEndedIterator<Item = crate::InterpolatedStringLiteralElement<'a>> {
        self.iter()
            .filter_map(super::super::generated::InterpolatedStringElement::as_literal)
    }

    pub fn interpolations(self) -> impl DoubleEndedIterator<Item = crate::InterpolatedElement<'a>> {
        self.iter()
            .filter_map(super::super::generated::InterpolatedStringElement::as_interpolation)
    }
}

impl crate::BytesLiteral<'_> {
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

impl<'a> crate::Parameters<'a> {
    pub fn find(self, name: &str) -> Option<ParameterWithDefault<'a>> {
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

impl crate::TString<'_> {
    pub fn quote_style(self) -> crate::str::Quote {
        self.flags().quote_style()
    }
}

impl TStringValue<'_> {
    pub fn is_empty_iterable(self) -> bool {
        self.iter().all(|node| node.elements().is_empty())
    }
}

impl Deref for crate::StringLiteral<'_> {
    type Target = str;
    fn deref(&self) -> &str {
        self.value()
    }
}

impl Deref for crate::InterpolatedStringLiteralElement<'_> {
    type Target = str;
    fn deref(&self) -> &str {
        self.value()
    }
}

impl crate::Arguments<'_> {
    pub fn l_paren_range(self) -> TextRange {
        TextRange::at(self.start(), 1.into())
    }
    pub fn r_paren_range(self) -> TextRange {
        TextRange::at(self.end() - ruff_text_size::TextSize::new(1), 1.into())
    }
}
