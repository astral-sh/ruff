//! Structural equality and hashing for borrowed Python AST nodes.
//!
//! Two [`ComparableExpr`]s are considered equal if the underlying AST nodes have the
//! same shape, ignoring trivia (e.g., parentheses, comments, and whitespace), the
//! location in the source code, and other contextual information (e.g., whether they
//! represent reads or writes, which is typically encoded in the Python AST).
//!
//! For example, in `[(a, b) for a, b in c]`, the `(a, b)` and `a, b` expressions are
//! considered equal, despite the former being parenthesized, and despite the former
//! being a write ([`ast::ExprContext::Store`]) and the latter being a read
//! ([`ast::ExprContext::Load`]).
//!
//! Similarly, `"a" "b"` and `"ab"` would be considered equal, despite the former being
//! an implicit concatenation of string literals, as these expressions are considered to
//! have the same shape in that they evaluate to the same value.

use crate as ast;
use crate::AstNode;
use crate::{Expr, Number};
use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// Use `compare_enum!(Wrapper(AstEnum) { ... })` to compare borrowed AST nodes
/// by their structural fields. Listing the fields once keeps the wrapper's
/// `PartialEq`, `Hash`, and `Debug` implementations in sync. Equality checks
/// the variants before reading either node's fields.
///
/// List every variant of `AstEnum`. For example, `ComparableStmt` uses both
/// forms of entry.
///
/// - `Return => stmt_return_fields` delegates to a function taking
///   `&ast::StmtReturn` and returning `Option<ComparableExpr<'_>>`. Only the
///   return value participates in comparison.
/// - `Pass(ast::StmtPassFields { range: _, node: _ })` compares just the
///   variant. Any two `pass` statements are structurally equal.
///
/// Destructure the node exhaustively in the helper or the inline pattern so
/// that adding an AST field forces a decision about whether to compare it.
macro_rules! hash_comparable_kind {
    ($ast:ident, $node:expr $(, $classify:ident)?; $state:ident) => {{
        let kind = $node$(.$classify())?;
        std::mem::discriminant(&kind).hash($state);
        kind
    }};
}

macro_rules! compare_enum {
    ($key:ident($ast:ident $(, $classify:ident)? $(; $guard:ident)?) {
        $($variant:ident $(($ignored:pat))? $(=> $fields:ident)?,)*
    }) => {
        impl PartialEq for $key<'_> {
            fn eq(&self, other: &Self) -> bool {
                $(if self.0.$guard() != other.0.$guard() { return false; })?
                let kind = self.0$(.$classify())?;
                let other = other.0$(.$classify())?;
                match kind {
                    $(
                        $(ast::$ast::$variant(left) => match other {
                            ast::$ast::$variant(right) => $fields(left) == $fields(right),
                            _ => false,
                        },)?
                        $(ast::$ast::$variant(node) => { let $ignored = node.fields(); matches!(other, ast::$ast::$variant(_)) },)?
                    )*
                }
            }
        }

        impl Eq for $key<'_> {}

        impl Hash for $key<'_> {
            fn hash<H: Hasher>(&self, state: &mut H) {
                let kind = hash_comparable_kind!($ast, self.0 $(, $classify)?; state);
                match kind {
                    $(
                        $(ast::$ast::$variant(node) => $fields(node).hash(state),)?
                        $(ast::$ast::$variant(node) => { let $ignored = node.fields(); },)?
                    )*
                }
            }
        }

        impl fmt::Debug for $key<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.0$(.$classify())? {
                    $(
                        $(ast::$ast::$variant(node) => f
                            .debug_tuple(stringify!($variant))
                            .field(&$fields(node))
                            .finish(),)?
                        $(ast::$ast::$variant(node) => { let $ignored = node.fields(); f.write_str(stringify!($variant)) },)?
                    )*
                }
            }
        }
    };
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableAlias<'a> {
    name: &'a str,
    asname: Option<&'a str>,
}

impl<'a> From<ast::Alias<'a>> for ComparableAlias<'a> {
    fn from(alias: ast::Alias<'a>) -> Self {
        let ast::AliasFields {
            name,
            asname,
            range: _,
            node: _,
        } = alias.fields();
        Self {
            name: name.as_str(),
            asname: asname.map(ast::Identifier::as_str),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableWithItem<'a> {
    context_expr: ComparableExpr<'a>,
    optional_vars: Option<ComparableExpr<'a>>,
}

impl<'a> From<ast::WithItem<'a>> for ComparableWithItem<'a> {
    fn from(with_item: ast::WithItem<'a>) -> Self {
        let ast::WithItemFields {
            context_expr,
            optional_vars,
            range: _,
            node: _,
        } = with_item.fields();
        Self {
            context_expr: context_expr.into(),
            optional_vars: optional_vars.map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparablePatternKeyword<'a> {
    attr: &'a str,
    pattern: ComparablePattern<'a>,
}

impl<'a> From<ast::PatternKeyword<'a>> for ComparablePatternKeyword<'a> {
    fn from(keyword: ast::PatternKeyword<'a>) -> Self {
        let ast::PatternKeywordFields {
            attr,
            pattern,
            range: _,
            node: _,
        } = keyword.fields();
        Self {
            attr: attr.as_str(),
            pattern: pattern.into(),
        }
    }
}

/// A borrowed structural comparison key for a match pattern.
#[derive(Clone, Copy)]
pub struct ComparablePattern<'a>(ast::Pattern<'a>);

impl<'a> From<ast::Pattern<'a>> for ComparablePattern<'a> {
    fn from(node: ast::Pattern<'a>) -> Self {
        Self(node)
    }
}

compare_enum!(ComparablePattern(PatternKind, classify) {
    MatchValue => match_value_fields,
    MatchSingleton => match_singleton_fields,
    MatchSequence => match_sequence_fields,
    MatchMapping => match_mapping_fields,
    MatchClass => match_class_fields,
    MatchStar => match_star_fields,
    MatchAs => match_as_fields,
    MatchOr => match_or_fields,
});

fn match_value_fields(node: ast::PatternMatchValue<'_>) -> ComparableExpr<'_> {
    let ast::PatternMatchValueFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    value.into()
}

fn match_singleton_fields(node: ast::PatternMatchSingleton<'_>) -> ast::Singleton {
    let ast::PatternMatchSingletonFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    value
}

fn match_sequence_fields(node: ast::PatternMatchSequence<'_>) -> ComparablePatternSlice<'_> {
    let ast::PatternMatchSequenceFields {
        patterns,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(patterns)
}

struct MatchMappingFields<'a>(ast::MappingPatternItems<'a>);

impl fmt::Debug for MatchMappingFields<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.entries()).finish()
    }
}

impl MatchMappingFields<'_> {
    fn entries(
        &self,
    ) -> impl Iterator<Item = ast::MappingPatternItem<ComparableExpr<'_>, ComparablePattern<'_>, &str>>
    {
        self.0.into_iter().map(|item| match item {
            ast::MappingPatternItem::Entry { key, pattern } => ast::MappingPatternItem::Entry {
                key: key.into(),
                pattern: pattern.into(),
            },
            ast::MappingPatternItem::Rest(name) => ast::MappingPatternItem::Rest(name.as_str()),
        })
    }
}
impl PartialEq for MatchMappingFields<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.entries().eq(other.entries())
    }
}
impl Eq for MatchMappingFields<'_> {}
impl Hash for MatchMappingFields<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.into_iter().count().hash(state);
        for entry in self.entries() {
            entry.hash(state);
        }
    }
}

fn match_mapping_fields(node: ast::PatternMatchMapping<'_>) -> MatchMappingFields<'_> {
    MatchMappingFields(node.items())
}

type ComparablePatternOrKeyword<'a> =
    ast::PatternOrKeyword<ComparablePattern<'a>, ComparablePatternKeyword<'a>>;

impl<'a> From<&'a ast::PatternOrKeyword> for ComparablePatternOrKeyword<'a> {
    fn from(item: &'a ast::PatternOrKeyword) -> Self {
        match item {
            ast::PatternOrKeyword::Pattern(pattern) => Self::Pattern(pattern.into()),
            ast::PatternOrKeyword::Keyword(keyword) => Self::Keyword(keyword.into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct MatchClassFields<'a> {
    cls: ComparableExpr<'a>,
    patterns: ComparablePatternSlice<'a>,
    keywords: ComparableSlice<'a, ast::kind::PatternKeyword, ComparablePatternKeyword<'a>>,
}

fn match_class_fields(node: ast::PatternMatchClass<'_>) -> MatchClassFields<'_> {
    let ast::PatternMatchClassFields { cls, arguments, .. } = node.fields();
    let ast::PatternArgumentsFields {
        patterns, keywords, ..
    } = arguments.fields();
    MatchClassFields {
        cls: cls.into(),
        items: ComparableSlice::new(items),
    }
}

fn match_star_fields(node: ast::PatternMatchStar<'_>) -> Option<&str> {
    let ast::PatternMatchStarFields {
        name,
        range: _,
        node: _,
    } = node.fields();
    name.map(ast::Identifier::as_str)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct MatchAsFields<'a> {
    pattern: Option<ComparablePattern<'a>>,
    name: Option<&'a str>,
}

fn match_as_fields(node: ast::PatternMatchAs<'_>) -> MatchAsFields<'_> {
    let ast::PatternMatchAsFields {
        pattern,
        name,
        range: _,
        node: _,
    } = node.fields();
    MatchAsFields {
        pattern: pattern.map(Into::into),
        name: name.map(ast::Identifier::as_str),
    }
}

fn match_or_fields(node: ast::PatternMatchOr<'_>) -> ComparablePatternSlice<'_> {
    let ast::PatternMatchOrFields {
        patterns,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(patterns)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableMatchCase<'a> {
    pattern: ComparablePattern<'a>,
    guard: Option<ComparableExpr<'a>>,
    body: ComparableStmtSlice<'a>,
}

impl<'a> From<ast::MatchCase<'a>> for ComparableMatchCase<'a> {
    fn from(match_case: ast::MatchCase<'a>) -> Self {
        let ast::MatchCaseFields {
            pattern,
            guard,
            body,
            range: _,
            node: _,
        } = match_case.fields();
        Self {
            pattern: pattern.into(),
            guard: guard.map(Into::into),
            body: ComparableSlice::new(body),
        }
    }
}

impl<'a> From<ast::Decorator<'a>> for ComparableExpr<'a> {
    fn from(decorator: ast::Decorator<'a>) -> Self {
        let ast::DecoratorFields {
            expression,
            range: _,
            node: _,
        } = decorator.fields();
        expression.into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableText<'a>(&'a str);
impl<'a> From<ast::Identifier<'a>> for ComparableText<'a> {
    fn from(identifier: ast::Identifier<'a>) -> Self {
        let ast::IdentifierFields {
            id,
            range: _,
            node: _,
        } = identifier.fields();
        Self(id.as_str())
    }
}

struct ComparableNumber<'a>(&'a ast::Number);

impl fmt::Debug for ComparableNumber<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for ComparableNumber<'_> {
    fn eq(&self, other: &Self) -> bool {
        match self.0 {
            ast::Number::Int(left) => matches!(other.0, ast::Number::Int(right) if left == right),
            ast::Number::Float(left) => {
                matches!(other.0, ast::Number::Float(right) if left.to_bits() == right.to_bits())
            }
            ast::Number::Complex { real, imag } => match other.0 {
                ast::Number::Complex {
                    real: other_real,
                    imag: other_imag,
                } => {
                    real.to_bits() == other_real.to_bits() && imag.to_bits() == other_imag.to_bits()
                }
                _ => false,
            },
        }
    }
}

impl Eq for ComparableNumber<'_> {}

impl Hash for ComparableNumber<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self.0).hash(state);
        match self.0 {
            ast::Number::Int(value) => value.hash(state),
            ast::Number::Float(value) => value.to_bits().hash(state),
            ast::Number::Complex { real, imag } => (real.to_bits(), imag.to_bits()).hash(state),
        }
    }
}

/// Borrows a sequence and converts each element as it is visited.
struct ComparableSlice<'a, T: ast::NodeType, C> {
    items: ast::NodeList<'a, T>,
    comparable: PhantomData<fn() -> C>,
}

impl<'a, T: ast::NodeType, C> ComparableSlice<'a, T, C> {
    fn new(items: ast::NodeList<'a, T>) -> Self {
        Self {
            items,
            comparable: PhantomData,
        }
    }

    fn iter(&self) -> impl Iterator<Item = C>
    where
        C: From<T::Borrowed<'a>>,
    {
        self.items.iter().map(C::from)
    }
}

impl<'a, T: ast::NodeType, C: From<T::Borrowed<'a>> + fmt::Debug> fmt::Debug
    for ComparableSlice<'a, T, C>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T: ast::NodeType, C> Default for ComparableSlice<'_, T, C> {
    fn default() -> Self {
        Self::new(ast::NodeList::default())
    }
}

impl<'a, T: ast::NodeType, C: From<T::Borrowed<'a>> + PartialEq> PartialEq
    for ComparableSlice<'a, T, C>
{
    fn eq(&self, other: &Self) -> bool {
        self.items.len() == other.items.len() && self.iter().eq(other.iter())
    }
}

impl<'a, T: ast::NodeType, C: From<T::Borrowed<'a>> + Eq> Eq for ComparableSlice<'a, T, C> {}

impl<'a, T: ast::NodeType, C: From<T::Borrowed<'a>> + Hash> Hash for ComparableSlice<'a, T, C> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.items.len().hash(state);
        for item in self.iter() {
            item.hash(state);
        }
    }
}

type ComparableStmtSlice<'a> = ComparableSlice<'a, ast::kind::Stmt, ComparableStmt<'a>>;
type ComparablePatternSlice<'a> = ComparableSlice<'a, ast::kind::Pattern, ComparablePattern<'a>>;
type ComparableComprehensionSlice<'a> =
    ComparableSlice<'a, ast::kind::Comprehension, ComparableComprehension<'a>>;
type ComparableExprSlice<'a> = ComparableSlice<'a, ast::kind::Expr, ComparableExpr<'a>>;

#[derive(Debug, Default, PartialEq, Eq, Hash)]
struct ComparableArguments<'a> {
    args: ComparableExprSlice<'a>,
    keywords: ComparableSlice<'a, ast::kind::Keyword, ComparableKeyword<'a>>,
}

impl<'a> From<ast::Arguments<'a>> for ComparableArguments<'a> {
    fn from(arguments: ast::Arguments<'a>) -> Self {
        let ast::ArgumentsFields {
            args,
            keywords,
            range: _,
            node: _,
        } = arguments.fields();
        Self {
            args: ComparableSlice::new(args),
            keywords: ComparableSlice::new(keywords),
        }
    }
}

#[derive(Clone, Copy)]
struct ComparableParameters<'a>(ast::Parameters<'a>);

impl fmt::Debug for ComparableParameters<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        ComparableParametersFields::from(self.0).fmt(f)
    }
}

impl PartialEq for ComparableParameters<'_> {
    fn eq(&self, other: &Self) -> bool {
        ComparableParametersFields::from(self.0) == ComparableParametersFields::from(other.0)
    }
}

impl Eq for ComparableParameters<'_> {}

impl Hash for ComparableParameters<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ComparableParametersFields::from(self.0).hash(state);
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableParametersFields<'a> {
    posonlyargs:
        ComparableSlice<'a, ast::kind::ParameterWithDefault, ComparableParameterWithDefault<'a>>,
    args: ComparableSlice<'a, ast::kind::ParameterWithDefault, ComparableParameterWithDefault<'a>>,
    vararg: Option<ComparableParameter<'a>>,
    kwonlyargs:
        ComparableSlice<'a, ast::kind::ParameterWithDefault, ComparableParameterWithDefault<'a>>,
    kwarg: Option<ComparableParameter<'a>>,
}

impl<'a> From<ast::Parameters<'a>> for ComparableParametersFields<'a> {
    fn from(parameters: ast::Parameters<'a>) -> Self {
        let ast::ParametersFields {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
            range: _,
            node: _,
        } = parameters.fields();
        Self {
            posonlyargs: ComparableSlice::new(posonlyargs),
            args: ComparableSlice::new(args),
            vararg: vararg.map(Into::into),
            kwonlyargs: ComparableSlice::new(kwonlyargs),
            kwarg: kwarg.map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableParameter<'a> {
    arg: &'a str,
    annotation: Option<ComparableExpr<'a>>,
}

impl<'a> From<ast::Parameter<'a>> for ComparableParameter<'a> {
    fn from(arg: ast::Parameter<'a>) -> Self {
        let ast::ParameterFields {
            name,
            annotation,
            range: _,
            node: _,
        } = arg.fields();
        Self {
            arg: name.as_str(),
            annotation: annotation.map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableParameterWithDefault<'a> {
    def: ComparableParameter<'a>,
    default: Option<ComparableExpr<'a>>,
}

impl<'a> From<ast::ParameterWithDefault<'a>> for ComparableParameterWithDefault<'a> {
    fn from(arg: ast::ParameterWithDefault<'a>) -> Self {
        let ast::ParameterWithDefaultFields {
            parameter,
            default,
            range: _,
            node: _,
        } = arg.fields();
        Self {
            def: parameter.into(),
            default: default.map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableKeyword<'a> {
    arg: Option<&'a str>,
    value: ComparableExpr<'a>,
}

impl<'a> From<ast::Keyword<'a>> for ComparableKeyword<'a> {
    fn from(keyword: ast::Keyword<'a>) -> Self {
        let ast::KeywordFields {
            arg,
            value,
            range: _,
            node: _,
        } = keyword.fields();
        Self {
            arg: arg.map(ast::Identifier::as_str),
            value: value.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableComprehension<'a> {
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    ifs: ComparableExprSlice<'a>,
    is_async: bool,
}

impl<'a> From<ast::Comprehension<'a>> for ComparableComprehension<'a> {
    fn from(comprehension: ast::Comprehension<'a>) -> Self {
        let ast::ComprehensionFields {
            target,
            iter,
            ifs,
            is_async,
            range: _,
            node: _,
        } = comprehension.fields();
        Self {
            target: target.into(),
            iter: iter.into(),
            ifs: ComparableSlice::new(ifs),
            is_async,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableExceptHandler<'a> {
    type_: Option<ComparableExpr<'a>>,
    name: Option<&'a str>,
    body: ComparableStmtSlice<'a>,
}

impl<'a> From<ast::ExceptHandler<'a>> for ComparableExceptHandler<'a> {
    fn from(except_handler: ast::ExceptHandler<'a>) -> Self {
        let ast::ExceptHandlerFields::ExceptHandler(ast::ExceptHandlerExceptHandlerFields {
            type_,
            name,
            body,
            range: _,
            node: _,
        }) = except_handler.fields();
        Self {
            type_: type_.map(Into::into),
            name: name.map(ast::Identifier::as_str),
            body: ComparableSlice::new(body),
        }
    }
}

/// Compares normalized elements. The enclosing expression distinguishes f-strings from t-strings.
#[derive(Clone, Copy)]
enum ComparableInterpolatedString<'a> {
    FString(ast::FStringValue<'a>),
    TString(ast::TStringValue<'a>),
}

impl<'a> ComparableInterpolatedString<'a> {
    /// Coalesce adjacent literals so that part boundaries do not affect comparison.
    /// For example, `f"{foo!r} one" " and two " f" and three {bar!s}"` decomposes into
    ///
    /// - An f-string part with the interpolation `{foo!r}` and the literal `" one"`.
    /// - A plain string literal part containing `" and two "`.
    /// - An f-string part with the literal `" and three "` and the interpolation `{bar!s}`.
    ///
    /// All three pieces of literal text form one comparable element between the
    /// two interpolations. A single piece can stay borrowed.
    ///
    /// [CPython's `string.templatelib.Template`] has one more string than interpolation.
    /// Interleaving the two sequences, starting with a string, recovers the reading
    /// order. For example, `t"{foo}bar"` has string slots `("", "bar")`, while
    /// `t"bar{foo}"` has `("bar", "")`. Here the interpolations already delimit
    /// the slots, so we can omit empty t-string text. For f-strings, an explicit
    /// empty literal part still participates in comparison, as in `f"{foo}" ""`.
    ///
    /// [CPython's `string.templatelib.Template`]: https://github.com/python/cpython/blob/c91ad5da9d92eac4718e4da8d53689c3cc24535e/Python/codegen.c#L4052-L4103
    fn normalized_elements(self) -> impl Iterator<Item = ComparableStringElement<'a>> {
        let mut raw = self.raw_elements().peekable();
        std::iter::from_fn(move || {
            loop {
                match raw.next()? {
                    element @ ComparableStringElement::Interpolation(_) => {
                        return Some(element);
                    }
                    ComparableStringElement::Literal(mut text) => {
                        while let Some(ComparableStringElement::Literal(next)) =
                            raw.next_if(|next| matches!(next, ComparableStringElement::Literal(_)))
                        {
                            text += next;
                        }
                        if !text.is_empty() || matches!(self, Self::FString(_)) {
                            return Some(ComparableStringElement::Literal(text));
                        }
                    }
                }
            }
        })
    }

    fn raw_elements(self) -> impl Iterator<Item = ComparableStringElement<'a>> {
        enum Parts<'a> {
            FString(ast::ListIter<'a, ast::FStringPart>),
            TString(ast::ListIter<'a, ast::AstNodeId<ast::kind::TString>>),
        }
        let mut parts = match self {
            Self::FString(value) => Parts::FString(value.iter()),
            Self::TString(value) => Parts::TString(value.iter()),
        };
        let mut elements = ast::InterpolatedStringElements::default().iter();
        std::iter::from_fn(move || {
            loop {
                if let Some(element) = elements.next() {
                    return Some(ComparableStringElement::from(element));
                }
                elements = match &mut parts {
                    Parts::FString(parts) => match parts.next()? {
                        ast::FStringPartView::FString(part) => part.elements().iter(),
                        ast::FStringPartView::Literal(part) => {
                            return Some(ComparableStringElement::Literal(Cow::Borrowed(
                                part.value(),
                            )));
                        }
                    },
                    Parts::TString(parts) => parts.next()?.elements().iter(),
                };
            }
        })
    }
}

impl PartialEq for ComparableInterpolatedString<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
            && self.normalized_elements().eq(other.normalized_elements())
    }
}

impl fmt::Debug for ComparableInterpolatedString<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.normalized_elements()).finish()
    }
}

impl Eq for ComparableInterpolatedString<'_> {}

impl Hash for ComparableInterpolatedString<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for element in self.normalized_elements() {
            Some(element).hash(state);
        }
        Option::<ComparableStringElement>::None.hash(state);
    }
}

enum ComparableStringElement<'a> {
    Literal(Cow<'a, str>),
    Interpolation(ast::InterpolatedElement<'a>),
}

impl<'a> From<ast::InterpolatedStringElement<'a>> for ComparableStringElement<'a> {
    fn from(element: ast::InterpolatedStringElement<'a>) -> Self {
        match element.classify() {
            ast::InterpolatedStringElementKind::Literal(literal) => {
                Self::Literal(Cow::Borrowed(literal.value()))
            }
            ast::InterpolatedStringElementKind::Interpolation(interpolation) => {
                Self::Interpolation(interpolation)
            }
        }
    }
}

impl PartialEq for ComparableStringElement<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(left), Self::Literal(right)) => left == right,
            (Self::Interpolation(left), Self::Interpolation(right)) => {
                interpolation_fields(*left) == interpolation_fields(*right)
            }
            (Self::Literal(_), Self::Interpolation(_))
            | (Self::Interpolation(_), Self::Literal(_)) => false,
        }
    }
}

impl fmt::Debug for ComparableStringElement<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(text) => text.fmt(f),
            Self::Interpolation(element) => interpolation_fields(*element).fmt(f),
        }
    }
}

impl Eq for ComparableStringElement<'_> {}

impl Hash for ComparableStringElement<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Literal(text) => text.hash(state),
            Self::Interpolation(interpolation) => interpolation_fields(*interpolation).hash(state),
        }
    }
}

/// The full debug text, including leading and trailing whitespace, appears in
/// the runtime value. If `x` is 1, `f"{x =}"` produces `"x =1"` and `f"{x=}"`
/// produces `"x=1"`. Python normalizes CR and CRLF to LF at compile time, so
/// these newline spellings produce identical text.
fn comparable_debug_text(text: &ast::DebugText) -> Cow<'_, str> {
    normalize_newlines(text.as_str())
}

fn normalize_newlines(contents: &str) -> Cow<'_, str> {
    if contents.contains('\r') {
        Cow::Owned(contents.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(contents)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct InterpolationFields<'a> {
    expression: ComparableExpr<'a>,
    debug_text: Option<Cow<'a, str>>,
    conversion: ast::ConversionFlag,
    format_spec: Option<
        ComparableSlice<'a, ast::kind::InterpolatedStringElement, ComparableStringElement<'a>>,
    >,
}

fn interpolation_fields(node: ast::InterpolatedElement<'_>) -> InterpolationFields<'_> {
    let ast::InterpolatedElementFields {
        expression,
        debug_text,
        conversion,
        format_spec,
        range: _,
        node: _,
    } = node.fields();

    InterpolationFields {
        expression: expression.into(),
        debug_text: debug_text.map(comparable_debug_text),
        conversion,
        format_spec: format_spec.map(|spec| ComparableSlice::new(spec.elements())),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableElifElseClause<'a> {
    test: Option<ComparableExpr<'a>>,
    body: ComparableStmtSlice<'a>,
}

impl<'a> From<ast::ElifElseClause<'a>> for ComparableElifElseClause<'a> {
    fn from(elif_else_clause: ast::ElifElseClause<'a>) -> Self {
        let ast::ElifElseClauseFields {
            range: _,
            node: _,
            test,
            body,
        } = elif_else_clause.fields();
        Self {
            test: test.map(Into::into),
            body: ComparableSlice::new(body),
        }
    }
}

/// A structural key for literal expressions that compares each string or bytes part separately.
#[derive(Clone, Copy)]
pub struct ComparableLiteral<'a>(ast::LiteralExpression<'a>);

impl<'a> From<ast::LiteralExpression<'a>> for ComparableLiteral<'a> {
    fn from(literal: ast::LiteralExpression<'a>) -> Self {
        Self(literal)
    }
}

compare_enum!(ComparableLiteral(LiteralExpression) {
    StringLiteral => string_literal_parts,
    BytesLiteral => bytes_literal_parts,
    NumberLiteral => number_literal_fields,
    BooleanLiteral => boolean_literal_fields,
    NoneLiteral(ast::ExprNoneLiteralFields { range: _, node: _ }),
    EllipsisLiteral(ast::ExprEllipsisLiteralFields { range: _, node: _ }),
});

fn string_literal_parts(
    node: ast::ExprStringLiteral<'_>,
) -> ComparableSlice<'_, ast::kind::StringLiteral, ComparableText<'_>> {
    let ast::ExprStringLiteralFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(value.as_slice())
}

fn bytes_literal_parts(
    node: ast::ExprBytesLiteral<'_>,
) -> ComparableSlice<'_, ast::kind::BytesLiteral, ComparableBytesLiteral<'_>> {
    let ast::ExprBytesLiteralFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(value.as_slice())
}

impl<'a> From<ast::StringLiteral<'a>> for ComparableText<'a> {
    fn from(literal: ast::StringLiteral<'a>) -> Self {
        let ast::StringLiteralFields {
            value,
            range: _,
            node: _,
            flags: _,
        } = literal.fields();
        Self(value)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableBytesLiteral<'a>(&'a [u8]);
impl<'a> From<ast::BytesLiteral<'a>> for ComparableBytesLiteral<'a> {
    fn from(literal: ast::BytesLiteral<'a>) -> Self {
        let ast::BytesLiteralFields {
            value,
            range: _,
            node: _,
            flags: _,
        } = literal.fields();
        Self(value)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableDictItem<'a> {
    key: Option<ComparableExpr<'a>>,
    value: ComparableExpr<'a>,
}

impl<'a> From<ast::DictItem<'a>> for ComparableDictItem<'a> {
    fn from(item: ast::DictItem<'a>) -> Self {
        let ast::DictItemFields { key, value, .. } = item.fields();
        Self {
            key: key.map(ComparableExpr::from),
            value: value.into(),
        }
    }
}

/// A borrowed expression key that ignores source locations and expression context.
#[derive(Clone, Copy)]
pub struct ComparableExpr<'a>(ast::Expr<'a>);

compare_enum!(ComparableExpr(ExprKind, classify; node_kind) {
    BoolOp => bool_op_fields,
    Named => named_fields,
    BinOp => bin_op_fields,
    UnaryOp => unary_op_fields,
    Lambda => lambda_fields,
    If => if_fields,
    Dict => dict_fields,
    Set => set_fields,
    ListComp => list_comp_fields,
    SetComp => set_comp_fields,
    DictComp => dict_comp_fields,
    Generator => generator_fields,
    Await => await_fields,
    Yield => yield_fields,
    YieldFrom => yield_from_fields,
    Compare => compare_fields,
    Call => call_fields,
    FString => f_string_fields,
    TString => t_string_fields,
    StringLiteral => string_literal_fields,
    BytesLiteral => bytes_literal_fields,
    NumberLiteral => number_literal_fields,
    BooleanLiteral => boolean_literal_fields,
    NoneLiteral(ast::ExprNoneLiteralFields { range: _, node: _ }),
    EllipsisLiteral(ast::ExprEllipsisLiteralFields { range: _, node: _ }),
    Attribute => attribute_fields,
    Subscript => subscript_fields,
    Starred => starred_fields,
    Name => name_fields,
    List => list_fields,
    Tuple => tuple_fields,
    Slice => slice_fields,
    IpyEscapeCommand => ipy_escape_command_fields,
});

impl<'a> From<ast::Expr<'a>> for ComparableExpr<'a> {
    fn from(expr: ast::Expr<'a>) -> Self {
        Self(expr)
    }
}

impl<'a> From<ast::ExprName<'a>> for ComparableExpr<'a> {
    fn from(name: ast::ExprName<'a>) -> Self {
        Self(name.into())
    }
}

impl<'a> From<&ast::Expr<'a>> for ComparableExpr<'a> {
    fn from(expr: &ast::Expr<'a>) -> Self {
        Self::from(*expr)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct BoolOpFields<'a> {
    op: ast::BoolOp,
    values: ComparableExprSlice<'a>,
}

fn bool_op_fields(node: ast::ExprBoolOp<'_>) -> BoolOpFields<'_> {
    let ast::ExprBoolOpFields {
        op,
        values,
        range: _,
        node: _,
    } = node.fields();
    BoolOpFields {
        op,
        values: ComparableSlice::new(values),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NamedFields<'a> {
    target: ComparableExpr<'a>,
    value: ComparableExpr<'a>,
}

fn named_fields(node: ast::ExprNamed<'_>) -> NamedFields<'_> {
    let ast::ExprNamedFields {
        target,
        value,
        range: _,
        node: _,
    } = node.fields();
    NamedFields {
        target: target.into(),
        value: value.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct BinOpFields<'a> {
    left: ComparableExpr<'a>,
    op: ast::Operator,
    right: ComparableExpr<'a>,
}

fn bin_op_fields(node: ast::ExprBinOp<'_>) -> BinOpFields<'_> {
    let ast::ExprBinOpFields {
        left,
        op,
        right,
        range: _,
        node: _,
    } = node.fields();
    BinOpFields {
        left: left.into(),
        op,
        right: right.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct UnaryOpFields<'a> {
    op: ast::UnaryOp,
    operand: ComparableExpr<'a>,
}

fn unary_op_fields(node: ast::ExprUnaryOp<'_>) -> UnaryOpFields<'_> {
    let ast::ExprUnaryOpFields {
        op,
        operand,
        range: _,
        node: _,
    } = node.fields();
    UnaryOpFields {
        op,
        operand: operand.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct LambdaFields<'a> {
    parameters: Option<ComparableParameters<'a>>,
    body: ComparableExpr<'a>,
}

fn lambda_fields(node: ast::ExprLambda<'_>) -> LambdaFields<'_> {
    let ast::ExprLambdaFields {
        parameters,
        body,
        range: _,
        node: _,
    } = node.fields();
    LambdaFields {
        parameters: parameters.map(ComparableParameters),
        body: body.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct IfFields<'a> {
    test: ComparableExpr<'a>,
    body: ComparableExpr<'a>,
    orelse: ComparableExpr<'a>,
}

fn if_fields(node: ast::ExprIf<'_>) -> IfFields<'_> {
    let ast::ExprIfFields {
        test,
        body,
        orelse,
        range: _,
        node: _,
    } = node.fields();
    IfFields {
        test: test.into(),
        body: body.into(),
        orelse: orelse.into(),
    }
}

fn dict_fields(
    node: ast::ExprDict<'_>,
) -> ComparableSlice<'_, ast::kind::DictItem, ComparableDictItem<'_>> {
    let ast::ExprDictFields {
        items,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(items)
}

fn set_fields(node: ast::ExprSet<'_>) -> ComparableExprSlice<'_> {
    let ast::ExprSetFields {
        elts,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(elts)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComprehensionExprFields<'a> {
    elt: ComparableExpr<'a>,
    generators: ComparableComprehensionSlice<'a>,
}

impl<'a> ComprehensionExprFields<'a> {
    fn new(elt: Expr<'a>, generators: ast::NodeList<'a, ast::kind::Comprehension>) -> Self {
        Self {
            elt: elt.into(),
            generators: ComparableSlice::new(generators),
        }
    }
}

fn list_comp_fields(node: ast::ExprListComp<'_>) -> ComprehensionExprFields<'_> {
    let ast::ExprListCompFields {
        elt,
        generators,
        range: _,
        node: _,
    } = node.fields();
    ComprehensionExprFields::new(elt, generators)
}

fn set_comp_fields(node: ast::ExprSetComp<'_>) -> ComprehensionExprFields<'_> {
    let ast::ExprSetCompFields {
        elt,
        generators,
        range: _,
        node: _,
    } = node.fields();
    ComprehensionExprFields::new(elt, generators)
}

fn dict_comp_fields(
    node: ast::ExprDictComp<'_>,
) -> (
    Option<ComparableExpr<'_>>,
    ComparableExpr<'_>,
    ComparableComprehensionSlice<'_>,
) {
    let ast::ExprDictCompFields {
        key,
        value,
        generators,
        range: _,
        node: _,
    } = node.fields();
    (
        key.map(Into::into),
        value.into(),
        ComparableSlice::new(generators),
    )
}

fn generator_fields(node: ast::ExprGenerator<'_>) -> ComprehensionExprFields<'_> {
    let ast::ExprGeneratorFields {
        elt,
        generators,
        range: _,
        node: _,
        parenthesized: _,
    } = node.fields();
    ComprehensionExprFields::new(elt, generators)
}

fn await_fields(node: ast::ExprAwait<'_>) -> ComparableExpr<'_> {
    let ast::ExprAwaitFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    value.into()
}

fn yield_fields(node: ast::ExprYield<'_>) -> Option<ComparableExpr<'_>> {
    let ast::ExprYieldFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    value.map(Into::into)
}

fn yield_from_fields(node: ast::ExprYieldFrom<'_>) -> ComparableExpr<'_> {
    let ast::ExprYieldFromFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    value.into()
}

struct ComparableOps<'a>(ast::List<'a, ast::CmpOp>);
impl fmt::Debug for ComparableOps<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0).finish()
    }
}

impl PartialEq for ComparableOps<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.iter().eq(other.0.iter())
    }
}
impl Eq for ComparableOps<'_> {}
impl Hash for ComparableOps<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.len().hash(state);
        for operator in self.0 {
            operator.hash(state);
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct CompareFields<'a> {
    ops: ComparableOps<'a>,
    operands: ComparableExprSlice<'a>,
}

fn compare_fields(node: ast::ExprCompare<'_>) -> CompareFields<'_> {
    let ast::ExprCompareFields {
        ops,
        operands,
        range: _,
        node: _,
    } = node.fields();
    CompareFields {
        ops: ComparableOps(ops),
        operands: ComparableSlice::new(operands),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct CallFields<'a> {
    func: ComparableExpr<'a>,
    arguments: ComparableArguments<'a>,
}

fn call_fields(node: ast::ExprCall<'_>) -> CallFields<'_> {
    let ast::ExprCallFields {
        func,
        arguments,
        range: _,
        node: _,
    } = node.fields();
    CallFields {
        func: func.into(),
        arguments: arguments.into(),
    }
}

fn f_string_fields(node: ast::ExprFString<'_>) -> ComparableInterpolatedString<'_> {
    let ast::ExprFStringFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    ComparableInterpolatedString::FString(value)
}

fn t_string_fields(node: ast::ExprTString<'_>) -> ComparableInterpolatedString<'_> {
    let ast::ExprTStringFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    ComparableInterpolatedString::TString(value)
}

fn string_literal_fields(node: ast::ExprStringLiteral<'_>) -> &str {
    let ast::ExprStringLiteralFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    // `"a" "b"` and `"ab"` both yield "ab". The AST caches the joined text
    // for the first spelling and lends the single literal's text for the second.
    value.to_str()
}

fn bytes_literal_fields(node: ast::ExprBytesLiteral<'_>) -> ComparableBytes<'_> {
    let ast::ExprBytesLiteralFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    ComparableBytes(value)
}

fn number_literal_fields(node: ast::ExprNumberLiteral<'_>) -> ComparableNumber<'_> {
    let ast::ExprNumberLiteralFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    ComparableNumber(value)
}

fn boolean_literal_fields(node: ast::ExprBooleanLiteral<'_>) -> bool {
    let ast::ExprBooleanLiteralFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    value
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct AttributeFields<'a> {
    value: ComparableExpr<'a>,
    attr: &'a str,
}

fn attribute_fields(node: ast::ExprAttribute<'_>) -> AttributeFields<'_> {
    let ast::ExprAttributeFields {
        value,
        attr,
        ctx: _,
        range: _,
        node: _,
    } = node.fields();
    AttributeFields {
        value: value.into(),
        attr: attr.as_str(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct SubscriptFields<'a> {
    value: ComparableExpr<'a>,
    slice: ComparableExpr<'a>,
}

fn subscript_fields(node: ast::ExprSubscript<'_>) -> SubscriptFields<'_> {
    let ast::ExprSubscriptFields {
        value,
        slice,
        ctx: _,
        range: _,
        node: _,
    } = node.fields();
    SubscriptFields {
        value: value.into(),
        slice: slice.into(),
    }
}

fn starred_fields(node: ast::ExprStarred<'_>) -> ComparableExpr<'_> {
    let ast::ExprStarredFields {
        value,
        ctx: _,
        range: _,
        node: _,
    } = node.fields();
    value.into()
}

fn name_fields(node: ast::ExprName<'_>) -> &str {
    let ast::ExprNameFields {
        id,
        ctx: _,
        range: _,
        node: _,
    } = node.fields();
    id.as_str()
}

fn list_fields(node: ast::ExprList<'_>) -> ComparableExprSlice<'_> {
    let ast::ExprListFields {
        elts,
        ctx: _,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(elts)
}

fn tuple_fields(node: ast::ExprTuple<'_>) -> ComparableExprSlice<'_> {
    let ast::ExprTupleFields {
        elts,
        ctx: _,
        range: _,
        node: _,
        parenthesized: _,
    } = node.fields();
    ComparableSlice::new(elts)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct SliceFields<'a> {
    lower: Option<ComparableExpr<'a>>,
    upper: Option<ComparableExpr<'a>>,
    step: Option<ComparableExpr<'a>>,
}

fn slice_fields(node: ast::ExprSlice<'_>) -> SliceFields<'_> {
    let ast::ExprSliceFields {
        lower,
        upper,
        step,
        range: _,
        node: _,
    } = node.fields();
    SliceFields {
        lower: lower.map(Into::into),
        upper: upper.map(Into::into),
        step: step.map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct IpyEscapeCommandFields<'a> {
    kind: ast::IpyEscapeKind,
    value: &'a str,
}

fn ipy_escape_command_fields(node: ast::ExprIpyEscapeCommand<'_>) -> IpyEscapeCommandFields<'_> {
    let ast::ExprIpyEscapeCommandFields {
        kind,
        value,
        range: _,
        node: _,
    } = node.fields();
    IpyEscapeCommandFields { kind, value }
}

struct ComparableBytes<'a>(ast::BytesValue<'a>);

impl PartialEq for ComparableBytes<'_> {
    fn eq(&self, other: &Self) -> bool {
        if let (Some([left]), Some([right])) =
            (self.0.as_slice().as_array(), other.0.as_slice().as_array())
        {
            return left.value() == right.value();
        }

        // Implicit concatenation makes `b'ab' b'c'` equal to `b'a' b'bc'`.
        // Compare slices up to the next part boundary on either side so we can
        // compare the full byte sequences without allocating a joined buffer.
        // In this example, matching `a` exhausts the first right part. Matching
        // `b` then exhausts the first left part, leaving `c` in `right` for
        // the next left part.
        let mut right_parts = other
            .0
            .iter()
            .map(ast::BytesLiteral::value)
            .filter(|part| !part.is_empty());
        let mut right: &[u8] = &[];
        for part in self.0 {
            let mut left = part.value();
            while !left.is_empty() {
                if right.is_empty() {
                    let Some(next) = right_parts.next() else {
                        return false;
                    };
                    right = next;
                }
                let len = left.len().min(right.len());
                if left[..len] != right[..len] {
                    return false;
                }
                left = &left[len..];
                right = &right[len..];
            }
        }
        // All left bytes matched. Extra right bytes can be in the current
        // slice or in a later part. The iterator skips empty parts.
        right.is_empty() && right_parts.next().is_none()
    }
}

impl fmt::Debug for ComparableBytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for part in self.0.as_slice() {
            list.entries(part.value());
        }
        list.finish()
    }
}

impl Eq for ComparableBytes<'_> {}

impl Hash for ComparableBytes<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // `b'a' b'b'` and `b'ab'` compare equal, so hash both as the slice `b"ab"`.
        // A hasher can give two one-byte writes a different result from one two-byte write.
        Cow::from(self.0).hash(state);
    }
}

type ComparableTypeParams<'a> = ComparableSlice<'a, ast::kind::TypeParam, ComparableTypeParam<'a>>;
impl<'a> From<ast::TypeParams<'a>> for ComparableTypeParams<'a> {
    fn from(params: ast::TypeParams<'a>) -> Self {
        let ast::TypeParamsFields {
            type_params,
            range: _,
            node: _,
        } = params.fields();
        Self::new(type_params)
    }
}

struct ComparableTypeParam<'a>(ast::TypeParam<'a>);

impl<'a> From<ast::TypeParam<'a>> for ComparableTypeParam<'a> {
    fn from(node: ast::TypeParam<'a>) -> Self {
        Self(node)
    }
}

compare_enum!(ComparableTypeParam(TypeParamKind, classify) {
    TypeVar => type_var_fields,
    TypeVarTuple => type_var_tuple_fields,
    ParamSpec => param_spec_fields,
});

#[derive(Debug, PartialEq, Eq, Hash)]
struct TypeVarFields<'a> {
    name: &'a str,
    bound: Option<ComparableExpr<'a>>,
    default: Option<ComparableExpr<'a>>,
}

fn type_var_fields(node: ast::TypeParamTypeVar<'_>) -> TypeVarFields<'_> {
    let ast::TypeParamTypeVarFields {
        name,
        bound,
        default,
        range: _,
        node: _,
    } = node.fields();
    TypeVarFields {
        name: name.as_str(),
        bound: bound.map(Into::into),
        default: default.map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct VariadicTypeParamFields<'a> {
    name: &'a str,
    default: Option<ComparableExpr<'a>>,
}

fn type_var_tuple_fields(node: ast::TypeParamTypeVarTuple<'_>) -> VariadicTypeParamFields<'_> {
    let ast::TypeParamTypeVarTupleFields {
        name,
        default,
        range: _,
        node: _,
    } = node.fields();
    VariadicTypeParamFields {
        name: name.as_str(),
        default: default.map(Into::into),
    }
}

fn param_spec_fields(node: ast::TypeParamParamSpec<'_>) -> VariadicTypeParamFields<'_> {
    let ast::TypeParamParamSpecFields {
        name,
        default,
        range: _,
        node: _,
    } = node.fields();
    VariadicTypeParamFields {
        name: name.as_str(),
        default: default.map(Into::into),
    }
}

/// A borrowed structural comparison key for a statement.
#[derive(Clone, Copy)]
pub struct ComparableStmt<'a>(ast::Stmt<'a>);

impl<'a> From<ast::Stmt<'a>> for ComparableStmt<'a> {
    fn from(node: ast::Stmt<'a>) -> Self {
        Self(node)
    }
}

compare_enum!(ComparableStmt(StmtKind, classify) {
    FunctionDef => stmt_function_def_fields,
    ClassDef => stmt_class_def_fields,
    Return => stmt_return_fields,
    Delete => stmt_delete_fields,
    TypeAlias => stmt_type_alias_fields,
    Assign => stmt_assign_fields,
    AugAssign => stmt_aug_assign_fields,
    AnnAssign => stmt_ann_assign_fields,
    For => stmt_for_fields,
    While => stmt_while_fields,
    If => stmt_if_fields,
    With => stmt_with_fields,
    Match => stmt_match_fields,
    Raise => stmt_raise_fields,
    Try => stmt_try_fields,
    Assert => stmt_assert_fields,
    Import => stmt_import_fields,
    ImportFrom => stmt_import_from_fields,
    Global => stmt_global_fields,
    Nonlocal => stmt_nonlocal_fields,
    IpyEscapeCommand => stmt_ipy_escape_command_fields,
    Expr => stmt_expr_fields,
    Pass(ast::StmtPassFields { range: _, node: _ }),
    Break(ast::StmtBreakFields { range: _, node: _ }),
    Continue(ast::StmtContinueFields { range: _, node: _ }),
});

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtFunctionDefFields<'a> {
    is_async: bool,
    decorator_list: ComparableSlice<'a, ast::kind::Decorator, ComparableExpr<'a>>,
    name: &'a str,
    type_params: Option<ComparableTypeParams<'a>>,
    parameters: ComparableParameters<'a>,
    returns: Option<ComparableExpr<'a>>,
    body: ComparableStmtSlice<'a>,
}

fn stmt_function_def_fields(node: ast::StmtFunctionDef<'_>) -> StmtFunctionDefFields<'_> {
    let ast::StmtFunctionDefFields {
        is_async,
        name,
        parameters,
        body,
        decorator_list,
        returns,
        type_params,
        range: _,
        node: _,
    } = node.fields();
    StmtFunctionDefFields {
        is_async,
        name: name.as_str(),
        parameters: ComparableParameters(parameters),
        body: ComparableSlice::new(body),
        decorator_list: ComparableSlice::new(decorator_list),
        returns: returns.map(Into::into),
        type_params: type_params.map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtClassDefFields<'a> {
    decorator_list: ComparableSlice<'a, ast::kind::Decorator, ComparableExpr<'a>>,
    name: &'a str,
    type_params: Option<ComparableTypeParams<'a>>,
    arguments: ComparableArguments<'a>,
    body: ComparableStmtSlice<'a>,
}

fn stmt_class_def_fields(node: ast::StmtClassDef<'_>) -> StmtClassDefFields<'_> {
    let ast::StmtClassDefFields {
        name,
        arguments,
        body,
        decorator_list,
        type_params,
        range: _,
        node: _,
    } = node.fields();
    StmtClassDefFields {
        name: name.as_str(),
        arguments: arguments.map(Into::into).unwrap_or_default(),
        body: ComparableSlice::new(body),
        decorator_list: ComparableSlice::new(decorator_list),
        type_params: type_params.map(Into::into),
    }
}

fn stmt_return_fields(node: ast::StmtReturn<'_>) -> Option<ComparableExpr<'_>> {
    let ast::StmtReturnFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    value.map(Into::into)
}

fn stmt_delete_fields(node: ast::StmtDelete<'_>) -> ComparableExprSlice<'_> {
    let ast::StmtDeleteFields {
        targets,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(targets)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtTypeAliasFields<'a> {
    name: ComparableExpr<'a>,
    type_params: Option<ComparableTypeParams<'a>>,
    value: ComparableExpr<'a>,
}

fn stmt_type_alias_fields(node: ast::StmtTypeAlias<'_>) -> StmtTypeAliasFields<'_> {
    let ast::StmtTypeAliasFields {
        range: _,
        node: _,
        name,
        type_params,
        value,
    } = node.fields();
    StmtTypeAliasFields {
        name: name.into(),
        type_params: type_params.map(Into::into),
        value: value.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAssignFields<'a> {
    targets: ComparableExprSlice<'a>,
    value: ComparableExpr<'a>,
}

fn stmt_assign_fields(node: ast::StmtAssign<'_>) -> StmtAssignFields<'_> {
    let ast::StmtAssignFields {
        targets,
        value,
        range: _,
        node: _,
    } = node.fields();
    StmtAssignFields {
        targets: ComparableSlice::new(targets),
        value: value.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAugAssignFields<'a> {
    target: ComparableExpr<'a>,
    op: ast::Operator,
    value: ComparableExpr<'a>,
}

fn stmt_aug_assign_fields(node: ast::StmtAugAssign<'_>) -> StmtAugAssignFields<'_> {
    let ast::StmtAugAssignFields {
        target,
        op,
        value,
        range: _,
        node: _,
    } = node.fields();
    StmtAugAssignFields {
        target: target.into(),
        op,
        value: value.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAnnAssignFields<'a> {
    target: ComparableExpr<'a>,
    annotation: ComparableExpr<'a>,
    value: Option<ComparableExpr<'a>>,
    simple: bool,
}

fn stmt_ann_assign_fields(node: ast::StmtAnnAssign<'_>) -> StmtAnnAssignFields<'_> {
    let ast::StmtAnnAssignFields {
        target,
        annotation,
        value,
        simple,
        range: _,
        node: _,
    } = node.fields();
    StmtAnnAssignFields {
        target: target.into(),
        annotation: annotation.into(),
        value: value.map(Into::into),
        simple,
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtForFields<'a> {
    is_async: bool,
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    body: ComparableStmtSlice<'a>,
    orelse: ComparableStmtSlice<'a>,
}

fn stmt_for_fields(node: ast::StmtFor<'_>) -> StmtForFields<'_> {
    let ast::StmtForFields {
        is_async,
        target,
        iter,
        body,
        orelse,
        range: _,
        node: _,
    } = node.fields();
    StmtForFields {
        is_async,
        target: target.into(),
        iter: iter.into(),
        body: ComparableSlice::new(body),
        orelse: ComparableSlice::new(orelse),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtWhileFields<'a> {
    test: ComparableExpr<'a>,
    body: ComparableStmtSlice<'a>,
    orelse: ComparableStmtSlice<'a>,
}

fn stmt_while_fields(node: ast::StmtWhile<'_>) -> StmtWhileFields<'_> {
    let ast::StmtWhileFields {
        test,
        body,
        orelse,
        range: _,
        node: _,
    } = node.fields();
    StmtWhileFields {
        test: test.into(),
        body: ComparableSlice::new(body),
        orelse: ComparableSlice::new(orelse),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtIfFields<'a> {
    test: ComparableExpr<'a>,
    body: ComparableStmtSlice<'a>,
    elif_else_clauses: ComparableSlice<'a, ast::kind::ElifElseClause, ComparableElifElseClause<'a>>,
}

fn stmt_if_fields(node: ast::StmtIf<'_>) -> StmtIfFields<'_> {
    let ast::StmtIfFields {
        test,
        body,
        elif_else_clauses,
        range: _,
        node: _,
    } = node.fields();
    StmtIfFields {
        test: test.into(),
        body: ComparableSlice::new(body),
        elif_else_clauses: ComparableSlice::new(elif_else_clauses),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtWithFields<'a> {
    is_async: bool,
    items: ComparableSlice<'a, ast::kind::WithItem, ComparableWithItem<'a>>,
    body: ComparableStmtSlice<'a>,
}

fn stmt_with_fields(node: ast::StmtWith<'_>) -> StmtWithFields<'_> {
    let ast::StmtWithFields {
        is_async,
        items,
        body,
        range: _,
        node: _,
    } = node.fields();
    StmtWithFields {
        is_async,
        items: ComparableSlice::new(items),
        body: ComparableSlice::new(body),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtMatchFields<'a> {
    subject: ComparableExpr<'a>,
    cases: ComparableSlice<'a, ast::kind::MatchCase, ComparableMatchCase<'a>>,
}

fn stmt_match_fields(node: ast::StmtMatch<'_>) -> StmtMatchFields<'_> {
    let ast::StmtMatchFields {
        subject,
        cases,
        range: _,
        node: _,
    } = node.fields();
    StmtMatchFields {
        subject: subject.into(),
        cases: ComparableSlice::new(cases),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtRaiseFields<'a> {
    exc: Option<ComparableExpr<'a>>,
    cause: Option<ComparableExpr<'a>>,
}

fn stmt_raise_fields(node: ast::StmtRaise<'_>) -> StmtRaiseFields<'_> {
    let ast::StmtRaiseFields {
        exc,
        cause,
        range: _,
        node: _,
    } = node.fields();
    StmtRaiseFields {
        exc: exc.map(Into::into),
        cause: cause.map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtTryFields<'a> {
    body: ComparableStmtSlice<'a>,
    handlers: ComparableSlice<'a, ast::kind::ExceptHandler, ComparableExceptHandler<'a>>,
    orelse: ComparableStmtSlice<'a>,
    finalbody: ComparableStmtSlice<'a>,
    is_star: bool,
}

fn stmt_try_fields(node: ast::StmtTry<'_>) -> StmtTryFields<'_> {
    let ast::StmtTryFields {
        body,
        handlers,
        orelse,
        finalbody,
        is_star,
        range: _,
        node: _,
    } = node.fields();
    StmtTryFields {
        body: ComparableSlice::new(body),
        handlers: ComparableSlice::new(handlers),
        orelse: ComparableSlice::new(orelse),
        finalbody: ComparableSlice::new(finalbody),
        is_star,
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAssertFields<'a> {
    test: ComparableExpr<'a>,
    msg: Option<ComparableExpr<'a>>,
}

fn stmt_assert_fields(node: ast::StmtAssert<'_>) -> StmtAssertFields<'_> {
    let ast::StmtAssertFields {
        test,
        msg,
        range: _,
        node: _,
    } = node.fields();
    StmtAssertFields {
        test: test.into(),
        msg: msg.map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtImportFields<'a> {
    names: ComparableSlice<'a, ast::kind::Alias, ComparableAlias<'a>>,
    is_lazy: bool,
}

fn stmt_import_fields(node: ast::StmtImport<'_>) -> StmtImportFields<'_> {
    let ast::StmtImportFields {
        names,
        is_lazy,
        range: _,
        node: _,
    } = node.fields();
    StmtImportFields {
        names: ComparableSlice::new(names),
        is_lazy,
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtImportFromFields<'a> {
    module: Option<&'a str>,
    names: ComparableSlice<'a, ast::kind::Alias, ComparableAlias<'a>>,
    level: u32,
    is_lazy: bool,
}

fn stmt_import_from_fields(node: ast::StmtImportFrom<'_>) -> StmtImportFromFields<'_> {
    let ast::StmtImportFromFields {
        module,
        names,
        level,
        is_lazy,
        range: _,
        node: _,
    } = node.fields();
    StmtImportFromFields {
        module: module.map(ast::Identifier::as_str),
        names: ComparableSlice::new(names),
        level,
        is_lazy,
    }
}

fn stmt_global_fields(
    node: ast::StmtGlobal<'_>,
) -> ComparableSlice<'_, ast::kind::Identifier, ComparableText<'_>> {
    let ast::StmtGlobalFields {
        names,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(names)
}

fn stmt_nonlocal_fields(
    node: ast::StmtNonlocal<'_>,
) -> ComparableSlice<'_, ast::kind::Identifier, ComparableText<'_>> {
    let ast::StmtNonlocalFields {
        names,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(names)
}

fn stmt_ipy_escape_command_fields(
    node: ast::StmtIpyEscapeCommand<'_>,
) -> IpyEscapeCommandFields<'_> {
    let ast::StmtIpyEscapeCommandFields {
        kind,
        value,
        range: _,
        node: _,
    } = node.fields();
    IpyEscapeCommandFields { kind, value }
}

fn stmt_expr_fields(node: ast::StmtExpr<'_>) -> ComparableExpr<'_> {
    let ast::StmtExprFields {
        value,
        range: _,
        node: _,
    } = node.fields();
    value.into()
}

#[derive(Clone, Copy)]
pub struct ComparableMod<'a>(ast::Mod<'a>);

impl<'a> From<ast::ModModule<'a>> for ComparableMod<'a> {
    fn from(node: ast::ModModule<'a>) -> Self {
        Self(node.into())
    }
}

impl<'a> From<ast::Mod<'a>> for ComparableMod<'a> {
    fn from(node: ast::Mod<'a>) -> Self {
        Self(node)
    }
}

compare_enum!(ComparableMod(ModKind, classify) {
    Module => mod_module_fields,
    Expression => mod_expression_fields,
});

fn mod_module_fields(node: ast::ModModule<'_>) -> ComparableStmtSlice<'_> {
    let ast::ModModuleFields {
        body,
        range: _,
        node: _,
    } = node.fields();
    ComparableSlice::new(body)
}

fn mod_expression_fields(node: ast::ModExpression<'_>) -> ComparableExpr<'_> {
    let ast::ModExpressionFields {
        body,
        range: _,
        node: _,
    } = node.fields();
    body.into()
}

/// Wrapper around [`Expr`] that implements [`Hash`] and [`PartialEq`] according to Python
/// semantics:
///
/// > Values that compare equal (such as 1, 1.0, and True) can be used interchangeably to index the
/// > same dictionary entry.
///
/// For example, considers `True`, `1`, and `1.0` to be equal, as they hash to the same value
/// in Python, along with `False`, `0`, and `0.0`.
///
/// See: <https://docs.python.org/3/library/stdtypes.html#mapping-types-dict>
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct HashableExpr<'a>(HashableExprKind<'a>);

#[derive(Debug, PartialEq, Eq, Hash)]
enum HashableExprKind<'a> {
    Comparable(ComparableExpr<'a>),
    Number(HashableNumber),
    NamedExpr {
        target: ComparableExpr<'a>,
        value: Box<HashableExprKind<'a>>,
    },
    Tuple(Vec<HashableExprKind<'a>>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct HashableNumber {
    real: HashableReal,
    imag: HashableReal,
}

impl HashableNumber {
    fn real(real: HashableReal) -> Self {
        Self {
            real,
            imag: HashableReal::Integer(0),
        }
    }

    fn complex(real: HashableReal, imag: HashableReal) -> Self {
        Self { real, imag }
    }

    fn negate(mut self) -> Self {
        self.real.negate();
        self.imag.negate();
        self
    }

    fn into_real(self) -> Option<HashableReal> {
        self.imag.is_zero().then_some(self.real)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum HashableReal {
    Integer(i128),
    Float(u64),
}

impl HashableReal {
    fn from_int(value: &ast::Int) -> Option<Self> {
        value.as_u64().map(i128::from).map(Self::Integer)
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::float_cmp,
        reason = "the round-trip check guarantees that the float is exactly representable as an integer"
    )]
    fn from_float(value: f64) -> Self {
        if value.is_finite() && value.abs() < U64_EXCLUSIVE_UPPER_BOUND {
            let integer = value as i128;
            if integer as f64 == value {
                return Self::Integer(integer);
            }
        }
        Self::Float(value.to_bits())
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "Python converts real components to floats before complex arithmetic"
    )]
    fn into_float(self) -> Self {
        match self {
            Self::Integer(integer) => Self::from_float(integer as f64),
            Self::Float(_) => self,
        }
    }

    fn is_zero(&self) -> bool {
        matches!(self, Self::Integer(0))
    }

    fn negate(&mut self) {
        match self {
            Self::Integer(integer) => *integer = -*integer,
            Self::Float(bits) => *bits ^= 1 << 63,
        }
    }
}

// `2^64`, the exclusive upper bound for values representable as a `u64`.
const U64_EXCLUSIVE_UPPER_BOUND: f64 = 18_446_744_073_709_551_616.0;

impl<'a> From<Expr<'a>> for HashableExpr<'a> {
    fn from(expr: Expr<'a>) -> Self {
        /// Returns a version of the given expression that can be hashed and compared according to
        /// Python  semantics.
        fn as_hashable(expr: Expr<'_>) -> HashableExprKind<'_> {
            if let Some(constant) = as_hashable_constant(expr) {
                return constant;
            }

            match expr.classify() {
                ast::ExprKind::Named(named) => HashableExprKind::NamedExpr {
                    target: ComparableExpr::from(named.target()),
                    value: Box::new(as_hashable(named.value())),
                },
                _ => HashableExprKind::Comparable(ComparableExpr::from(expr)),
            }
        }

        /// Returns a hashable representation if the expression's value is statically known.
        fn as_hashable_constant(expr: Expr<'_>) -> Option<HashableExprKind<'_>> {
            if let Some(number) = as_number(expr) {
                return Some(HashableExprKind::Number(number));
            }

            let kind = match expr.classify() {
                ast::ExprKind::Tuple(tuple) => HashableExprKind::Tuple(
                    tuple
                        .elts()
                        .iter()
                        .map(as_hashable_constant)
                        .collect::<Option<_>>()?,
                ),
                _ if expr.is_literal_expr() => {
                    HashableExprKind::Comparable(ComparableExpr::from(expr))
                }
                _ => return None,
            };

            Some(kind)
        }

        fn as_number(expr: Expr<'_>) -> Option<HashableNumber> {
            match expr.classify() {
                ast::ExprKind::BooleanLiteral(boolean) => Some(HashableNumber::real(
                    HashableReal::Integer(i128::from(u8::from(boolean.value()))),
                )),
                ast::ExprKind::NumberLiteral(number) => match number.value() {
                    Number::Int(int) => HashableReal::from_int(int).map(HashableNumber::real),
                    Number::Float(float) => {
                        Some(HashableNumber::real(HashableReal::from_float(*float)))
                    }
                    Number::Complex { real, imag } => Some(HashableNumber::complex(
                        HashableReal::from_float(*real),
                        HashableReal::from_float(*imag),
                    )),
                },
                ast::ExprKind::UnaryOp(unary) => match unary.op() {
                    ast::UnaryOp::UAdd => as_number(unary.operand()),
                    ast::UnaryOp::USub => as_number(unary.operand()).map(HashableNumber::negate),
                    ast::UnaryOp::Invert | ast::UnaryOp::Not => None,
                },
                ast::ExprKind::BinOp(binary)
                    if let op @ (ast::Operator::Add | ast::Operator::Sub) = binary.op() =>
                {
                    let real = as_number(binary.left())?.into_real()?.into_float();
                    let Number::Complex {
                        real: complex_real,
                        imag,
                    } = binary.right().as_number_literal_expr()?.value()
                    else {
                        return None;
                    };
                    let complex_real = HashableReal::from_float(*complex_real);
                    if !complex_real.is_zero() {
                        return None;
                    }
                    let mut imag = HashableReal::from_float(*imag);
                    if op.is_sub() {
                        imag.negate();
                    }
                    Some(HashableNumber::complex(real, imag))
                }
                _ => None,
            }
        }

        Self(as_hashable(expr))
    }
}
