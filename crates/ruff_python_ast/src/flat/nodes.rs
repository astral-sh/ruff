use crate::StringFlags;
use ruff_text_size::{Ranged, TextRange};
use std::fmt;
use std::ops::Deref;

use super::{AstNode, AstNodeId, ExprKind, List};
use crate::{
    CmpOp, Expr, ExprCompare, Identifier, NodeKind, Parameter, ParameterWithDefault, Pattern,
};

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

impl<'a> ExprKind<'a> {
    /// Returns the elements of a list or tuple expression.
    pub fn as_list_or_tuple_elements(self) -> Option<super::NodeList<'a, crate::kind::Expr>> {
        match self {
            ExprKind::List(list) => Some(list.elts()),
            ExprKind::Tuple(tuple) => Some(tuple.elts()),
            _ => None,
        }
    }
}

impl<'a> Expr<'a> {
    /// Returns the elements of a list or tuple expression.
    pub fn as_list_or_tuple_elements(self) -> Option<super::NodeList<'a, crate::kind::Expr>> {
        self.classify().as_list_or_tuple_elements()
    }

    /// Returns the value of an integer literal.
    #[inline]
    pub fn as_int_literal(self) -> Option<&'a crate::Int> {
        self.as_number_literal_expr()?.value().as_int()
    }

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
        self.operands().slice(1..)
    }

    /// Yields each comparison operator and the operand to its right, in source order.
    #[inline]
    pub fn operators_and_comparators(
        self,
    ) -> impl DoubleEndedIterator<Item = (CmpOp, Expr<'a>)>
    + ExactSizeIterator
    + std::iter::FusedIterator
    + Clone {
        self.ops().iter().zip(self.comparators())
    }

    #[inline]
    pub fn iter(
        self,
    ) -> impl DoubleEndedIterator<Item = (Expr<'a>, CmpOp, Expr<'a>)>
    + ExactSizeIterator
    + std::iter::FusedIterator
    + Clone {
        let operands = self.operands();
        operands
            .iter()
            .zip(self.ops())
            .zip(operands.slice(1..))
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
    fn text_range(self) -> u64 {
        let [start, len] = super::split_words(self.text);
        if len == u32::from(u16::MAX) {
            self.parts.ast.extra_word::<true>(start as usize)
        } else {
            self.text
        }
    }
    #[inline]
    pub fn to_str(self) -> &'a str {
        self.parts.ast.string(self.text_range())
    }
    pub fn len(self) -> usize {
        super::split_words(self.text_range())[1] as usize
    }
    pub fn is_empty(self) -> bool {
        // The out-of-line marker is only used when the text is too long to fit inline.
        super::split_words(self.text)[1] == 0
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

impl<'a> crate::Arguments<'a> {
    /// Returns the index of this keyword in the written argument list.
    pub fn keyword_position(self, keyword: crate::Keyword<'a>) -> Option<usize> {
        self.items().position_of_node(keyword.syntax_node())
    }

    pub fn len(self) -> usize {
        self.items().len()
    }
    pub fn is_empty(self) -> bool {
        self.items().is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AnyParameter<'a> {
    NonVariadic(ParameterWithDefault<'a>),
    Variadic(Parameter<'a>),
}

impl<'a> AnyParameter<'a> {
    #[inline]
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

impl<'a> crate::ParameterItem<ParameterWithDefault<'a>, Parameter<'a>> {
    pub fn into_parameter(self) -> AnyParameter<'a> {
        match self {
            Self::PositionalOnly(p) | Self::PositionalOrKeyword(p) | Self::KeywordOnly(p) => {
                AnyParameter::NonVariadic(p)
            }
            Self::Vararg(p) | Self::Kwarg(p) => AnyParameter::Variadic(p),
        }
    }
}

impl<'a> crate::Parameters<'a> {
    /// Iterates the parameter roles in source order, without looking up the child nodes.
    pub fn roles(
        self,
    ) -> impl DoubleEndedIterator<Item = crate::ParameterRole>
    + ExactSizeIterator
    + std::iter::FusedIterator
    + Clone {
        self.items().parameter_roles()
    }

    pub fn posonlyargs(self) -> impl DoubleEndedIterator<Item = ParameterWithDefault<'a>> + Clone {
        self.items()
            .matching(|role| role == super::ParameterItems::POSITIONAL_ONLY)
    }

    pub fn args(self) -> impl DoubleEndedIterator<Item = ParameterWithDefault<'a>> + Clone {
        self.items()
            .matching(|role| role == super::ParameterItems::POSITIONAL_OR_KEYWORD)
    }

    pub fn kwonlyargs(self) -> impl DoubleEndedIterator<Item = ParameterWithDefault<'a>> + Clone {
        self.items()
            .matching(|role| role == super::ParameterItems::KEYWORD_ONLY)
    }

    pub fn vararg(self) -> Option<crate::Parameter<'a>> {
        self.items()
            .matching(|role| role == super::ParameterItems::VARARG)
            .next()
    }

    pub fn kwarg(self) -> Option<crate::Parameter<'a>> {
        self.items()
            .matching(|role| role == super::ParameterItems::KWARG)
            .next_back()
    }

    /// Positional-only and positional-or-keyword parameters, in argument-binding order.
    pub fn iter_positional_params(
        self,
    ) -> impl DoubleEndedIterator<Item = ParameterWithDefault<'a>> + Clone {
        self.items().matching(|role| {
            matches!(
                role,
                super::ParameterItems::POSITIONAL_ONLY
                    | super::ParameterItems::POSITIONAL_OR_KEYWORD
            )
        })
    }

    /// Positional-or-keyword and keyword-only parameters, in source order.
    pub fn iter_keyword_params(
        self,
    ) -> impl DoubleEndedIterator<Item = ParameterWithDefault<'a>> + Clone {
        self.items().matching(|role| {
            matches!(
                role,
                super::ParameterItems::POSITIONAL_OR_KEYWORD | super::ParameterItems::KEYWORD_ONLY
            )
        })
    }

    /// Parameters which can have defaults, including keyword-only parameters.
    pub fn iter_non_variadic_params(
        self,
    ) -> impl DoubleEndedIterator<Item = ParameterWithDefault<'a>> {
        self.items().matching(|role| {
            matches!(
                role,
                super::ParameterItems::POSITIONAL_ONLY
                    | super::ParameterItems::POSITIONAL_OR_KEYWORD
                    | super::ParameterItems::KEYWORD_ONLY
            )
        })
    }

    /// Visits parameters that can have defaults and returns `*args` and `**kwargs`.
    /// Callers that process variadic parameters last can do so in the same pass.
    pub fn split_variadic(
        self,
        mut non_variadic: impl FnMut(ParameterWithDefault<'a>),
    ) -> (Option<Parameter<'a>>, Option<Parameter<'a>>) {
        let mut vararg = None;
        let mut kwarg = None;
        for item in self.items().iter() {
            match item {
                crate::ParameterItem::PositionalOnly(p)
                | crate::ParameterItem::PositionalOrKeyword(p)
                | crate::ParameterItem::KeywordOnly(p) => non_variadic(p),
                crate::ParameterItem::Vararg(p) => {
                    vararg.get_or_insert(p);
                }
                crate::ParameterItem::Kwarg(p) => kwarg = Some(p),
            }
        }
        (vararg, kwarg)
    }

    pub fn find(self, name: &str) -> Option<ParameterWithDefault<'a>> {
        self.iter_non_variadic_params()
            .find(|p| p.parameter().name().as_str() == name)
    }

    pub fn index(self, name: &str) -> Option<usize> {
        self.iter_non_variadic_params()
            .position(|p| p.parameter().name().as_str() == name)
    }

    /// Iterates parameters in source order, including in recovered syntax.
    pub fn iter_source_order(
        self,
    ) -> impl DoubleEndedIterator<Item = AnyParameter<'a>>
    + ExactSizeIterator
    + std::iter::FusedIterator
    + Clone {
        self.items()
            .iter()
            .map(crate::ParameterItem::into_parameter)
    }

    /// Iterates parameter roles and nodes in argument-binding order, including in recovered syntax.
    pub fn items_in_binding_order(
        self,
    ) -> impl DoubleEndedIterator<Item = crate::ParameterItem<ParameterWithDefault<'a>, Parameter<'a>>>
    {
        if self.items().is_in_binding_order() {
            return itertools::Either::Left(self.items().iter());
        }
        itertools::Either::Right(
            self.posonlyargs()
                .map(crate::ParameterItem::PositionalOnly)
                .chain(self.args().map(crate::ParameterItem::PositionalOrKeyword))
                .chain(self.vararg().map(crate::ParameterItem::Vararg))
                .chain(self.kwonlyargs().map(crate::ParameterItem::KeywordOnly))
                .chain(self.kwarg().map(crate::ParameterItem::Kwarg)),
        )
    }

    /// Returns the first parameter in argument-binding order.
    pub fn first(self) -> Option<AnyParameter<'a>> {
        let first = self.items().iter().next()?;
        if matches!(
            first,
            crate::ParameterItem::PositionalOnly(_) | crate::ParameterItem::PositionalOrKeyword(_)
        ) || self.items().is_in_binding_order()
        {
            Some(first.into_parameter())
        } else {
            self.iter_grouped().next()
        }
    }

    /// Iterates parameters in argument-binding order.
    pub fn iter(self) -> impl DoubleEndedIterator<Item = AnyParameter<'a>> {
        if self.items().is_in_binding_order() {
            itertools::Either::Left(self.iter_source_order())
        } else {
            itertools::Either::Right(self.iter_grouped())
        }
    }

    fn iter_grouped(self) -> impl DoubleEndedIterator<Item = AnyParameter<'a>> {
        self.iter_positional_params()
            .map(AnyParameter::NonVariadic)
            .chain(
                std::iter::once_with(move || self.vararg().map(AnyParameter::Variadic)).flatten(),
            )
            .chain(self.kwonlyargs().map(AnyParameter::NonVariadic))
            .chain(std::iter::once_with(move || self.kwarg().map(AnyParameter::Variadic)).flatten())
    }

    pub fn len(self) -> usize {
        self.items().len()
    }

    pub fn is_empty(self) -> bool {
        self.items().is_empty()
    }

    pub fn includes(self, name: &str) -> bool {
        self.items()
            .iter()
            .any(|item| item.into_parameter().name() == name)
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

impl<'a> super::PatternKind<'a> {
    /// The name bound by this pattern itself, excluding nested patterns.
    pub fn capture_name(self) -> Option<Identifier<'a>> {
        match self {
            super::PatternKind::MatchAs(node) => node.name(),
            super::PatternKind::MatchStar(node) => node.name(),
            super::PatternKind::MatchMapping(node) => node.items().rest(),
            _ => None,
        }
    }
}

impl crate::Pattern<'_> {
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

impl<'a> crate::PatternOrKeyword<Pattern<'a>, crate::PatternKeyword<'a>> {
    pub fn pattern(self) -> Pattern<'a> {
        match self {
            Self::Pattern(pattern) => pattern,
            Self::Keyword(keyword) => keyword.pattern(),
        }
    }
}

impl<'a> crate::PatternArguments<'a> {
    pub fn patterns(self) -> super::FilteredNodes<'a, crate::kind::Pattern> {
        super::FilteredNodes::new(self.items())
    }

    pub fn keywords(self) -> super::FilteredNodes<'a, crate::kind::PatternKeyword> {
        super::FilteredNodes::new(self.items())
    }
}

impl<'a> crate::Arguments<'a> {
    #[inline]
    pub fn find_keyword(self, name: &str) -> Option<crate::Keyword<'a>> {
        self.keywords()
            .iter()
            .find(|kw| kw.arg().is_some_and(|arg| arg == name))
    }

    /// Look up the first explicit keyword for each requested name.
    pub fn find_keywords<const N: usize>(
        self,
        names: [&str; N],
    ) -> [Option<crate::Keyword<'a>>; N] {
        let mut values = [None; N];
        if N == 0 {
            return values;
        }
        let mut remaining = N;
        for keyword in self.keywords() {
            let Some(name) = keyword.arg() else {
                continue;
            };
            let name = name.as_str();
            for (requested, slot) in names.iter().zip(&mut values) {
                if slot.is_none() && *requested == name {
                    *slot = Some(keyword);
                    remaining -= 1;
                    if remaining == 0 {
                        return values;
                    }
                }
            }
        }
        values
    }

    pub fn find_positional(self, position: usize) -> Option<Expr<'a>> {
        self.args()
            .iter()
            .take_while(|arg| !arg.is_starred_expr())
            .nth(position)
    }

    pub fn find_argument_value(self, name: &str, position: usize) -> Option<Expr<'a>> {
        self.find_argument_as(
            name,
            position,
            crate::Keyword::value,
            std::convert::identity,
        )
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
        self.iter()
            .all(|part| part.byte_range().indices().is_empty())
    }
    pub fn len(self) -> usize {
        self.iter()
            .map(|part| part.byte_range().indices().len())
            .sum()
    }
}

impl<'a> From<BytesValue<'a>> for std::borrow::Cow<'a, [u8]> {
    fn from(value: BytesValue<'a>) -> Self {
        let mut parts = value.iter();
        let Some(first) = parts.next() else {
            return Self::Borrowed(&[]);
        };
        if parts.len() == 0 {
            return Self::Borrowed(first.value());
        }

        // Parsed parts are consecutive in the byte buffer. A builder can reuse parts
        // from anywhere in that buffer or put them in a different order.
        let mut range = first.byte_range().indices();
        for part in parts {
            let next = part.byte_range().indices();
            if range.end != next.start {
                let mut joined = Vec::with_capacity(value.len());
                for part in value {
                    joined.extend_from_slice(part.value());
                }
                return Self::Owned(joined);
            }
            range.end = next.end;
        }
        Self::Borrowed(&value.0.ast.bytes[range])
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
    pub fn bases(self) -> super::FilteredNodes<'a, crate::kind::Expr> {
        self.arguments()
            .map_or_else(Default::default, crate::Arguments::args)
    }
    pub fn keywords(self) -> super::FilteredNodes<'a, crate::kind::Keyword> {
        self.arguments()
            .map_or_else(Default::default, crate::Arguments::keywords)
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
impl crate::Keyword<'_> {
    /// Return whether this keyword is a mapping unpacking (`**expression`).
    pub fn is_variadic(self) -> bool {
        self.syntax.row.keyword_arg() == 0
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ArgOrKeyword<'a> {
    Arg(Expr<'a>),
    Keyword(crate::Keyword<'a>),
}
impl std::fmt::Debug for ArgOrKeyword<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arg(expr) => f.debug_tuple("Positional").field(expr).finish(),
            Self::Keyword(kw) => f.debug_tuple("Keyword").field(kw).finish(),
        }
    }
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
            Self::Keyword(keyword) => keyword.is_variadic(),
        }
    }
    pub fn as_variadic(self) -> Option<crate::Keyword<'a>> {
        self.as_keyword().filter(|keyword| keyword.is_variadic())
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
    /// Visits positional arguments (including starred arguments) before keywords.
    pub fn for_each_in_evaluation_order(self, mut visit: impl FnMut(ArgOrKeyword<'a>)) {
        let mut items = self.iter_source_order();
        // In f(a, key=b, *c), only the suffix after key=b can contain positional
        // arguments that must move ahead of it.
        let first_keyword = loop {
            match items.next() {
                Some(arg @ ArgOrKeyword::Arg(_)) => visit(arg),
                Some(keyword @ ArgOrKeyword::Keyword(_)) => break keyword,
                None => return,
            }
        };
        for arg in items
            .clone()
            .filter(|item| matches!(item, ArgOrKeyword::Arg(_)))
        {
            visit(arg);
        }
        visit(first_keyword);
        for keyword in items.filter(|item| matches!(item, ArgOrKeyword::Keyword(_))) {
            visit(keyword);
        }
    }

    pub fn iter_source_order(self) -> ArgumentsSourceOrder<'a> {
        self.items().iter()
    }

    /// Return the `N` positional arguments if the call has exactly that shape.
    /// A starred argument counts as positional.
    pub fn as_positional_array<const N: usize>(self) -> Option<[Expr<'a>; N]> {
        let items = self.items();
        if items.len() != N {
            return None;
        }
        itertools::Itertools::collect_array(items.iter().map_while(|item| match item {
            ArgOrKeyword::Arg(arg) => Some(arg),
            ArgOrKeyword::Keyword(_) => None,
        }))
    }

    /// Return the argument if the call contains exactly one positional argument and no keywords.
    pub fn as_single_positional(self) -> Option<Expr<'a>> {
        let [ArgOrKeyword::Arg(arg)] = self.items().as_array()? else {
            return None;
        };
        Some(arg)
    }

    pub fn args(self) -> super::FilteredNodes<'a, crate::kind::Expr> {
        super::FilteredNodes::new(self.items())
    }

    pub fn keywords(self) -> super::FilteredNodes<'a, crate::kind::Keyword> {
        super::FilteredNodes::new(self.items())
    }
    /// Look up the first `N` parameters by position or keyword. An explicit
    /// keyword takes precedence over a positional value, just like `find_argument`.
    pub fn find_argument_values<const N: usize>(self, names: [&str; N]) -> [Option<Expr<'a>>; N] {
        let mut values = [None; N];
        let mut from_keyword = [false; N];
        let mut position = 0;
        for item in self.iter_source_order() {
            match item {
                ArgOrKeyword::Arg(arg) if position < N => {
                    if arg.is_starred_expr() {
                        position = N;
                    } else {
                        if !from_keyword[position] {
                            values[position] = Some(arg);
                        }
                        position += 1;
                    }
                }
                ArgOrKeyword::Keyword(keyword) if let Some(name) = keyword.arg() => {
                    for (index, &requested) in names.iter().enumerate() {
                        if !from_keyword[index] && name.as_str() == requested {
                            values[index] = Some(keyword.value());
                            from_keyword[index] = true;
                        }
                    }
                }
                _ => {}
            }
        }
        values
    }

    pub fn find_argument(self, name: &str, position: usize) -> Option<ArgOrKeyword<'a>> {
        self.find_argument_as(name, position, ArgOrKeyword::Keyword, ArgOrKeyword::Arg)
    }

    fn find_argument_as<T>(
        self,
        name: &str,
        position: usize,
        from_keyword: impl FnOnce(crate::Keyword<'a>) -> T,
        from_positional: impl FnOnce(Expr<'a>) -> T,
    ) -> Option<T> {
        let mut position = Some(position);
        let mut positional = None;
        for item in self.iter_source_order() {
            match item {
                ArgOrKeyword::Keyword(keyword) if keyword.arg().is_some_and(|arg| arg == name) => {
                    // An explicit keyword takes precedence over a positional argument.
                    return Some(from_keyword(keyword));
                }
                ArgOrKeyword::Arg(arg) if let Some(index) = position => {
                    if arg.is_starred_expr() {
                        position = None;
                    } else if index == 0 {
                        positional = Some(arg);
                        position = None;
                    } else {
                        position = Some(index - 1);
                    }
                }
                _ => {}
            }
        }
        positional.map(from_positional)
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
pub type ArgumentsSourceOrder<'a> = super::ListIter<'a, super::ArgumentId>;

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

impl crate::DictItem<'_> {
    /// Whether this item unpacks a mapping (`**mapping`).
    pub fn is_unpacking(self) -> bool {
        self.syntax.row.dict_item_key() == 0
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
            super::AnyNode::StmtFunctionDef(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyNode::StmtClassDef(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyNode::StmtWith(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyNode::MatchCase(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyNode::ExceptHandlerExceptHandler(node) => {
                node.body().last().map(AstNode::syntax_node)
            }
            super::AnyNode::ElifElseClause(node) => node.body().last().map(AstNode::syntax_node),
            super::AnyNode::StmtIf(node) => node
                .elif_else_clauses()
                .last()
                .map_or_else(|| node.body(), crate::ElifElseClause::body)
                .last()
                .map(AstNode::syntax_node),
            super::AnyNode::StmtFor(node) => node
                .orelse()
                .last()
                .or_else(|| node.body().last())
                .map(AstNode::syntax_node),
            super::AnyNode::StmtWhile(node) => node
                .orelse()
                .last()
                .or_else(|| node.body().last())
                .map(AstNode::syntax_node),
            super::AnyNode::StmtMatch(node) => node.cases().last().map(AstNode::syntax_node),
            super::AnyNode::StmtTry(node) => node
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
        match body.classify() {
            super::AnyNode::StmtFor(node) => {
                node.body().starts_with_node(self) || node.orelse().starts_with_node(self)
            }
            super::AnyNode::StmtWhile(node) => {
                node.body().starts_with_node(self) || node.orelse().starts_with_node(self)
            }
            super::AnyNode::StmtTry(node) => {
                node.body().starts_with_node(self)
                    || node.orelse().starts_with_node(self)
                    || node.finalbody().starts_with_node(self)
            }
            super::AnyNode::StmtIf(node) => node.body().starts_with_node(self),
            super::AnyNode::ElifElseClause(node) => node.body().starts_with_node(self),
            super::AnyNode::StmtWith(node) => node.body().starts_with_node(self),
            super::AnyNode::ExceptHandlerExceptHandler(node) => node.body().starts_with_node(self),
            super::AnyNode::MatchCase(node) => node.body().starts_with_node(self),
            super::AnyNode::StmtFunctionDef(node) => node.body().starts_with_node(self),
            super::AnyNode::StmtClassDef(node) => node.body().starts_with_node(self),
            super::AnyNode::StmtMatch(node) => node.cases().starts_with_node(self),
            _ => false,
        }
    }

    /// Returns `true` if this node is first in an alternate body (for example, the `else` of a loop).
    pub fn is_first_statement_in_alternate_body(self, body: Self) -> bool {
        match body.classify() {
            super::AnyNode::StmtFor(node) => node.orelse().starts_with_node(self),
            super::AnyNode::StmtWhile(node) => node.orelse().starts_with_node(self),
            super::AnyNode::StmtTry(node) => {
                node.handlers().starts_with_node(self)
                    || node.orelse().starts_with_node(self)
                    || node.finalbody().starts_with_node(self)
            }
            super::AnyNode::StmtIf(node) => node.elif_else_clauses().starts_with_node(self),
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
