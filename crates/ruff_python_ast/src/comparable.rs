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
use crate::{Expr, Number};
use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableAlias<'a> {
    name: &'a str,
    asname: Option<&'a str>,
}

impl<'a> From<&'a ast::Alias> for ComparableAlias<'a> {
    fn from(alias: &'a ast::Alias) -> Self {
        let ast::Alias {
            name,
            asname,
            range: _,
            node_index: _,
        } = alias;
        Self {
            name: name.as_str(),
            asname: asname.as_deref(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableWithItem<'a> {
    context_expr: ComparableExpr<'a>,
    optional_vars: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::WithItem> for ComparableWithItem<'a> {
    fn from(with_item: &'a ast::WithItem) -> Self {
        let ast::WithItem {
            context_expr,
            optional_vars,
            range: _,
            node_index: _,
        } = with_item;
        Self {
            context_expr: context_expr.into(),
            optional_vars: optional_vars.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparablePatternKeyword<'a> {
    attr: &'a str,
    pattern: ComparablePattern<'a>,
}

impl<'a> From<&'a ast::PatternKeyword> for ComparablePatternKeyword<'a> {
    fn from(keyword: &'a ast::PatternKeyword) -> Self {
        let ast::PatternKeyword {
            attr,
            pattern,
            range: _,
            node_index: _,
        } = keyword;
        Self {
            attr: attr.as_str(),
            pattern: pattern.into(),
        }
    }
}

/// A borrowed structural comparison key for a match pattern.
#[derive(Clone, Copy)]
pub struct ComparablePattern<'a>(&'a ast::Pattern);

impl<'a> From<&'a ast::Pattern> for ComparablePattern<'a> {
    fn from(node: &'a ast::Pattern) -> Self {
        Self(node)
    }
}

impl fmt::Debug for ComparablePattern<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for ComparablePattern<'_> {
    fn eq(&self, other: &Self) -> bool {
        match self.0 {
            ast::Pattern::MatchValue(left) => match other.0 {
                ast::Pattern::MatchValue(right) => {
                    match_value_fields(left) == match_value_fields(right)
                }
                _ => false,
            },
            ast::Pattern::MatchSingleton(left) => match other.0 {
                ast::Pattern::MatchSingleton(right) => {
                    match_singleton_fields(left) == match_singleton_fields(right)
                }
                _ => false,
            },
            ast::Pattern::MatchSequence(left) => match other.0 {
                ast::Pattern::MatchSequence(right) => {
                    match_sequence_fields(left) == match_sequence_fields(right)
                }
                _ => false,
            },
            ast::Pattern::MatchMapping(left) => match other.0 {
                ast::Pattern::MatchMapping(right) => {
                    match_mapping_fields(left) == match_mapping_fields(right)
                }
                _ => false,
            },
            ast::Pattern::MatchClass(left) => match other.0 {
                ast::Pattern::MatchClass(right) => {
                    match_class_fields(left) == match_class_fields(right)
                }
                _ => false,
            },
            ast::Pattern::MatchStar(left) => match other.0 {
                ast::Pattern::MatchStar(right) => {
                    match_star_fields(left) == match_star_fields(right)
                }
                _ => false,
            },
            ast::Pattern::MatchAs(left) => match other.0 {
                ast::Pattern::MatchAs(right) => match_as_fields(left) == match_as_fields(right),
                _ => false,
            },
            ast::Pattern::MatchOr(left) => match other.0 {
                ast::Pattern::MatchOr(right) => match_or_fields(left) == match_or_fields(right),
                _ => false,
            },
        }
    }
}

impl Eq for ComparablePattern<'_> {}

impl Hash for ComparablePattern<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self.0).hash(state);
        match self.0 {
            ast::Pattern::MatchValue(node) => match_value_fields(node).hash(state),
            ast::Pattern::MatchSingleton(node) => match_singleton_fields(node).hash(state),
            ast::Pattern::MatchSequence(node) => match_sequence_fields(node).hash(state),
            ast::Pattern::MatchMapping(node) => match_mapping_fields(node).hash(state),
            ast::Pattern::MatchClass(node) => match_class_fields(node).hash(state),
            ast::Pattern::MatchStar(node) => match_star_fields(node).hash(state),
            ast::Pattern::MatchAs(node) => match_as_fields(node).hash(state),
            ast::Pattern::MatchOr(node) => match_or_fields(node).hash(state),
        }
    }
}

fn match_value_fields(node: &ast::PatternMatchValue) -> ComparableExpr<'_> {
    let ast::PatternMatchValue {
        value,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

fn match_singleton_fields(node: &ast::PatternMatchSingleton) -> ast::Singleton {
    let ast::PatternMatchSingleton {
        value,
        range: _,
        node_index: _,
    } = node;
    *value
}

fn match_sequence_fields(node: &ast::PatternMatchSequence) -> ComparablePatternSlice<'_> {
    let ast::PatternMatchSequence {
        patterns,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(patterns)
}

#[derive(PartialEq, Eq, Hash)]
struct MatchMappingFields<'a> {
    keys: ComparableExprSlice<'a>,
    patterns: ComparablePatternSlice<'a>,
    rest: Option<&'a str>,
}

fn match_mapping_fields(node: &ast::PatternMatchMapping) -> MatchMappingFields<'_> {
    let ast::PatternMatchMapping {
        keys,
        patterns,
        rest,
        range: _,
        node_index: _,
    } = node;
    MatchMappingFields {
        keys: ComparableSlice::new(keys),
        patterns: ComparableSlice::new(patterns),
        rest: rest.as_deref(),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct MatchClassFields<'a> {
    cls: ComparableExpr<'a>,
    patterns: ComparablePatternSlice<'a>,
    keywords: ComparableSlice<'a, ast::PatternKeyword, ComparablePatternKeyword<'a>>,
}

fn match_class_fields(node: &ast::PatternMatchClass) -> MatchClassFields<'_> {
    let ast::PatternMatchClass {
        cls,
        arguments:
            ast::PatternArguments {
                patterns,
                keywords,
                range: _,
                node_index: _,
            },
        range: _,
        node_index: _,
    } = node;
    MatchClassFields {
        cls: cls.into(),
        patterns: ComparableSlice::new(patterns),
        keywords: ComparableSlice::new(keywords),
    }
}

fn match_star_fields(node: &ast::PatternMatchStar) -> Option<&str> {
    let ast::PatternMatchStar {
        name,
        range: _,
        node_index: _,
    } = node;
    name.as_deref()
}

#[derive(PartialEq, Eq, Hash)]
struct MatchAsFields<'a> {
    pattern: Option<ComparablePattern<'a>>,
    name: Option<&'a str>,
}

fn match_as_fields(node: &ast::PatternMatchAs) -> MatchAsFields<'_> {
    let ast::PatternMatchAs {
        pattern,
        name,
        range: _,
        node_index: _,
    } = node;
    MatchAsFields {
        pattern: pattern.as_deref().map(Into::into),
        name: name.as_deref(),
    }
}

fn match_or_fields(node: &ast::PatternMatchOr) -> ComparablePatternSlice<'_> {
    let ast::PatternMatchOr {
        patterns,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(patterns)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableMatchCase<'a> {
    pattern: ComparablePattern<'a>,
    guard: Option<ComparableExpr<'a>>,
    body: ComparableStmtSlice<'a>,
}

impl<'a> From<&'a ast::MatchCase> for ComparableMatchCase<'a> {
    fn from(match_case: &'a ast::MatchCase) -> Self {
        let ast::MatchCase {
            pattern,
            guard,
            body,
            range: _,
            node_index: _,
        } = match_case;
        Self {
            pattern: pattern.into(),
            guard: guard.as_ref().map(Into::into),
            body: ComparableSlice::new(body),
        }
    }
}

impl<'a> From<&'a ast::Decorator> for ComparableExpr<'a> {
    fn from(decorator: &'a ast::Decorator) -> Self {
        let ast::Decorator {
            expression,
            range: _,
            node_index: _,
        } = decorator;
        expression.into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableText<'a>(&'a str);
impl<'a> From<&'a ast::Identifier> for ComparableText<'a> {
    fn from(identifier: &'a ast::Identifier) -> Self {
        let ast::Identifier {
            id,
            range: _,
            node_index: _,
        } = identifier;
        Self(id.as_str())
    }
}

struct ComparableNumber<'a>(&'a ast::Number);

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

/// Borrows a sequence and converts its elements only as equality or hashing visits them.
struct ComparableSlice<'a, T, C> {
    items: &'a [T],
    comparable: PhantomData<fn() -> C>,
}

impl<'a, T, C> ComparableSlice<'a, T, C> {
    fn new(items: &'a [T]) -> Self {
        Self {
            items,
            comparable: PhantomData,
        }
    }
}

impl<'a, T, C: From<&'a T> + fmt::Debug> fmt::Debug for ComparableSlice<'a, T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.items.iter().map(C::from))
            .finish()
    }
}

impl<T, C> Default for ComparableSlice<'_, T, C> {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl<'a, T, C: From<&'a T> + PartialEq> PartialEq for ComparableSlice<'a, T, C> {
    fn eq(&self, other: &Self) -> bool {
        self.items.len() == other.items.len()
            && self
                .items
                .iter()
                .map(C::from)
                .eq(other.items.iter().map(C::from))
    }
}

impl<'a, T, C: From<&'a T> + Eq> Eq for ComparableSlice<'a, T, C> {}

impl<'a, T, C: From<&'a T> + Hash> Hash for ComparableSlice<'a, T, C> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.items.len().hash(state);
        for item in self.items {
            C::from(item).hash(state);
        }
    }
}

type ComparableStmtSlice<'a> = ComparableSlice<'a, ast::Stmt, ComparableStmt<'a>>;
type ComparablePatternSlice<'a> = ComparableSlice<'a, ast::Pattern, ComparablePattern<'a>>;
type ComparableComprehensionSlice<'a> =
    ComparableSlice<'a, ast::Comprehension, ComparableComprehension<'a>>;
type ComparableExprSlice<'a> = ComparableSlice<'a, Expr, ComparableExpr<'a>>;

#[derive(Debug, Default, PartialEq, Eq, Hash)]
struct ComparableArguments<'a> {
    args: ComparableExprSlice<'a>,
    keywords: ComparableSlice<'a, ast::Keyword, ComparableKeyword<'a>>,
}

impl<'a> From<&'a ast::Arguments> for ComparableArguments<'a> {
    fn from(arguments: &'a ast::Arguments) -> Self {
        let ast::Arguments {
            args,
            keywords,
            range: _,
            node_index: _,
        } = arguments;
        Self {
            args: ComparableSlice::new(args),
            keywords: ComparableSlice::new(keywords),
        }
    }
}

#[derive(Clone, Copy)]
struct ComparableParameters<'a>(&'a ast::Parameters);

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
    posonlyargs: ComparableSlice<'a, ast::ParameterWithDefault, ComparableParameterWithDefault<'a>>,
    args: ComparableSlice<'a, ast::ParameterWithDefault, ComparableParameterWithDefault<'a>>,
    vararg: Option<ComparableParameter<'a>>,
    kwonlyargs: ComparableSlice<'a, ast::ParameterWithDefault, ComparableParameterWithDefault<'a>>,
    kwarg: Option<ComparableParameter<'a>>,
}

impl<'a> From<&'a ast::Parameters> for ComparableParametersFields<'a> {
    fn from(parameters: &'a ast::Parameters) -> Self {
        let ast::Parameters {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
            range: _,
            node_index: _,
        } = parameters;
        Self {
            posonlyargs: ComparableSlice::new(posonlyargs),
            args: ComparableSlice::new(args),
            vararg: vararg.as_deref().map(Into::into),
            kwonlyargs: ComparableSlice::new(kwonlyargs),
            kwarg: kwarg.as_deref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableParameter<'a> {
    arg: &'a str,
    annotation: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::Parameter> for ComparableParameter<'a> {
    fn from(arg: &'a ast::Parameter) -> Self {
        let ast::Parameter {
            name,
            annotation,
            range: _,
            node_index: _,
        } = arg;
        Self {
            arg: name.as_str(),
            annotation: annotation.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableParameterWithDefault<'a> {
    def: ComparableParameter<'a>,
    default: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::ParameterWithDefault> for ComparableParameterWithDefault<'a> {
    fn from(arg: &'a ast::ParameterWithDefault) -> Self {
        let ast::ParameterWithDefault {
            parameter,
            default,
            range: _,
            node_index: _,
        } = arg;
        Self {
            def: parameter.into(),
            default: default.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableKeyword<'a> {
    arg: Option<&'a str>,
    value: ComparableExpr<'a>,
}

impl<'a> From<&'a ast::Keyword> for ComparableKeyword<'a> {
    fn from(keyword: &'a ast::Keyword) -> Self {
        let ast::Keyword {
            arg,
            value,
            range: _,
            node_index: _,
        } = keyword;
        Self {
            arg: arg.as_ref().map(ast::Identifier::as_str),
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

impl<'a> From<&'a ast::Comprehension> for ComparableComprehension<'a> {
    fn from(comprehension: &'a ast::Comprehension) -> Self {
        let ast::Comprehension {
            target,
            iter,
            ifs,
            is_async,
            range: _,
            node_index: _,
        } = comprehension;
        Self {
            target: target.into(),
            iter: iter.into(),
            ifs: ComparableSlice::new(ifs),
            is_async: *is_async,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableExceptHandler<'a> {
    type_: Option<ComparableExpr<'a>>,
    name: Option<&'a str>,
    body: ComparableStmtSlice<'a>,
}

impl<'a> From<&'a ast::ExceptHandler> for ComparableExceptHandler<'a> {
    fn from(except_handler: &'a ast::ExceptHandler) -> Self {
        let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_,
            name,
            body,
            range: _,
            node_index: _,
        }) = except_handler;
        Self {
            type_: type_.as_ref().map(Into::into),
            name: name.as_deref(),
            body: ComparableSlice::new(body),
        }
    }
}

enum ComparableStringElement<'a> {
    Literal(Cow<'a, str>),
    Interpolation(&'a ast::InterpolatedElement),
}

impl<'a> From<&'a ast::InterpolatedStringElement> for ComparableStringElement<'a> {
    fn from(element: &'a ast::InterpolatedStringElement) -> Self {
        match element {
            ast::InterpolatedStringElement::Literal(ast::InterpolatedStringLiteralElement {
                value,
                range: _,
                node_index: _,
            }) => Self::Literal(Cow::Borrowed(value)),
            ast::InterpolatedStringElement::Interpolation(interpolation) => {
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
                interpolation_fields(left) == interpolation_fields(right)
            }
            (Self::Literal(_), Self::Interpolation(_))
            | (Self::Interpolation(_), Self::Literal(_)) => false,
        }
    }
}

impl Eq for ComparableStringElement<'_> {}

impl Hash for ComparableStringElement<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Literal(text) => text.hash(state),
            Self::Interpolation(interpolation) => interpolation_fields(interpolation).hash(state),
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

#[derive(PartialEq, Eq, Hash)]
struct InterpolationFields<'a> {
    expression: ComparableExpr<'a>,
    debug_text: Option<Cow<'a, str>>,
    conversion: ast::ConversionFlag,
    format_spec:
        Option<ComparableSlice<'a, ast::InterpolatedStringElement, ComparableStringElement<'a>>>,
}

fn interpolation_fields(node: &ast::InterpolatedElement) -> InterpolationFields<'_> {
    let ast::InterpolatedElement {
        expression,
        debug_text,
        conversion,
        format_spec,
        range: _,
        node_index: _,
    } = node;

    InterpolationFields {
        expression: expression.into(),
        debug_text: debug_text.as_ref().map(comparable_debug_text),
        conversion: *conversion,
        format_spec: format_spec.as_deref().map(
            |ast::InterpolatedStringFormatSpec {
                 elements,
                 range: _,
                 node_index: _,
             }| ComparableSlice::new(elements),
        ),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableElifElseClause<'a> {
    test: Option<ComparableExpr<'a>>,
    body: ComparableStmtSlice<'a>,
}

impl<'a> From<&'a ast::ElifElseClause> for ComparableElifElseClause<'a> {
    fn from(elif_else_clause: &'a ast::ElifElseClause) -> Self {
        let ast::ElifElseClause {
            range: _,
            node_index: _,
            test,
            body,
        } = elif_else_clause;
        Self {
            test: test.as_ref().map(Into::into),
            body: ComparableSlice::new(body),
        }
    }
}

/// A structural key for literal expressions that compares each string or bytes part separately.
#[derive(Clone, Copy)]
pub struct ComparableLiteral<'a>(ast::LiteralExpressionRef<'a>);

impl<'a> From<ast::LiteralExpressionRef<'a>> for ComparableLiteral<'a> {
    fn from(literal: ast::LiteralExpressionRef<'a>) -> Self {
        Self(literal)
    }
}

impl fmt::Debug for ComparableLiteral<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for ComparableLiteral<'_> {
    fn eq(&self, other: &Self) -> bool {
        match self.0 {
            ast::LiteralExpressionRef::StringLiteral(left) => match other.0 {
                ast::LiteralExpressionRef::StringLiteral(right) => {
                    string_literal_parts(left) == string_literal_parts(right)
                }
                _ => false,
            },
            ast::LiteralExpressionRef::BytesLiteral(left) => match other.0 {
                ast::LiteralExpressionRef::BytesLiteral(right) => {
                    bytes_literal_parts(left) == bytes_literal_parts(right)
                }
                _ => false,
            },
            ast::LiteralExpressionRef::NumberLiteral(left) => match other.0 {
                ast::LiteralExpressionRef::NumberLiteral(right) => {
                    number_literal_fields(left) == number_literal_fields(right)
                }
                _ => false,
            },
            ast::LiteralExpressionRef::BooleanLiteral(left) => match other.0 {
                ast::LiteralExpressionRef::BooleanLiteral(right) => {
                    boolean_literal_fields(left) == boolean_literal_fields(right)
                }
                _ => false,
            },
            ast::LiteralExpressionRef::NoneLiteral(ast::ExprNoneLiteral {
                range: _,
                node_index: _,
            }) => matches!(other.0, ast::LiteralExpressionRef::NoneLiteral(_)),
            ast::LiteralExpressionRef::EllipsisLiteral(ast::ExprEllipsisLiteral {
                range: _,
                node_index: _,
            }) => matches!(other.0, ast::LiteralExpressionRef::EllipsisLiteral(_)),
        }
    }
}

impl Eq for ComparableLiteral<'_> {}

impl Hash for ComparableLiteral<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(&self.0).hash(state);
        match self.0 {
            ast::LiteralExpressionRef::StringLiteral(node) => {
                string_literal_parts(node).hash(state);
            }
            ast::LiteralExpressionRef::BytesLiteral(node) => bytes_literal_parts(node).hash(state),
            ast::LiteralExpressionRef::NumberLiteral(node) => {
                number_literal_fields(node).hash(state);
            }
            ast::LiteralExpressionRef::BooleanLiteral(node) => {
                boolean_literal_fields(node).hash(state);
            }
            ast::LiteralExpressionRef::NoneLiteral(ast::ExprNoneLiteral {
                range: _,
                node_index: _,
            }) => {}
            ast::LiteralExpressionRef::EllipsisLiteral(ast::ExprEllipsisLiteral {
                range: _,
                node_index: _,
            }) => {}
        }
    }
}

fn string_literal_parts(
    node: &ast::ExprStringLiteral,
) -> ComparableSlice<'_, ast::StringLiteral, ComparableText<'_>> {
    let ast::ExprStringLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(value.as_slice())
}

fn bytes_literal_parts(
    node: &ast::ExprBytesLiteral,
) -> ComparableSlice<'_, ast::BytesLiteral, ComparableBytesLiteral<'_>> {
    let ast::ExprBytesLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(value.as_slice())
}

impl<'a> From<&'a ast::StringLiteral> for ComparableText<'a> {
    fn from(literal: &'a ast::StringLiteral) -> Self {
        let ast::StringLiteral {
            value,
            range: _,
            node_index: _,
            flags: _,
        } = literal;
        Self(value)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableBytesLiteral<'a>(&'a [u8]);
impl<'a> From<&'a ast::BytesLiteral> for ComparableBytesLiteral<'a> {
    fn from(literal: &'a ast::BytesLiteral) -> Self {
        let ast::BytesLiteral {
            value,
            range: _,
            node_index: _,
            flags: _,
        } = literal;
        Self(value)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableDictItem<'a> {
    key: Option<ComparableExpr<'a>>,
    value: ComparableExpr<'a>,
}

impl<'a> From<&'a ast::DictItem> for ComparableDictItem<'a> {
    fn from(ast::DictItem { key, value }: &'a ast::DictItem) -> Self {
        Self {
            key: key.as_ref().map(ComparableExpr::from),
            value: value.into(),
        }
    }
}

/// A borrowed expression key that ignores source locations and expression context.
#[derive(Clone, Copy)]
pub struct ComparableExpr<'a>(ast::ExprRef<'a>);

impl fmt::Debug for ComparableExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for ComparableExpr<'_> {
    fn eq(&self, other: &Self) -> bool {
        match self.0 {
            ast::ExprRef::BoolOp(left) => match other.0 {
                ast::ExprRef::BoolOp(right) => bool_op_fields(left) == bool_op_fields(right),
                _ => false,
            },
            ast::ExprRef::Named(left) => match other.0 {
                ast::ExprRef::Named(right) => named_fields(left) == named_fields(right),
                _ => false,
            },
            ast::ExprRef::BinOp(left) => match other.0 {
                ast::ExprRef::BinOp(right) => bin_op_fields(left) == bin_op_fields(right),
                _ => false,
            },
            ast::ExprRef::UnaryOp(left) => match other.0 {
                ast::ExprRef::UnaryOp(right) => unary_op_fields(left) == unary_op_fields(right),
                _ => false,
            },
            ast::ExprRef::Lambda(left) => match other.0 {
                ast::ExprRef::Lambda(right) => lambda_fields(left) == lambda_fields(right),
                _ => false,
            },
            ast::ExprRef::If(left) => match other.0 {
                ast::ExprRef::If(right) => if_fields(left) == if_fields(right),
                _ => false,
            },
            ast::ExprRef::Dict(left) => match other.0 {
                ast::ExprRef::Dict(right) => dict_fields(left) == dict_fields(right),
                _ => false,
            },
            ast::ExprRef::Set(left) => match other.0 {
                ast::ExprRef::Set(right) => set_fields(left) == set_fields(right),
                _ => false,
            },
            ast::ExprRef::ListComp(left) => match other.0 {
                ast::ExprRef::ListComp(right) => list_comp_fields(left) == list_comp_fields(right),
                _ => false,
            },
            ast::ExprRef::SetComp(left) => match other.0 {
                ast::ExprRef::SetComp(right) => set_comp_fields(left) == set_comp_fields(right),
                _ => false,
            },
            ast::ExprRef::DictComp(left) => match other.0 {
                ast::ExprRef::DictComp(right) => dict_comp_fields(left) == dict_comp_fields(right),
                _ => false,
            },
            ast::ExprRef::Generator(left) => match other.0 {
                ast::ExprRef::Generator(right) => generator_fields(left) == generator_fields(right),
                _ => false,
            },
            ast::ExprRef::Await(left) => match other.0 {
                ast::ExprRef::Await(right) => await_fields(left) == await_fields(right),
                _ => false,
            },
            ast::ExprRef::Yield(left) => match other.0 {
                ast::ExprRef::Yield(right) => yield_fields(left) == yield_fields(right),
                _ => false,
            },
            ast::ExprRef::YieldFrom(left) => match other.0 {
                ast::ExprRef::YieldFrom(right) => {
                    yield_from_fields(left) == yield_from_fields(right)
                }
                _ => false,
            },
            ast::ExprRef::Compare(left) => match other.0 {
                ast::ExprRef::Compare(right) => compare_fields(left) == compare_fields(right),
                _ => false,
            },
            ast::ExprRef::Call(left) => match other.0 {
                ast::ExprRef::Call(right) => call_fields(left) == call_fields(right),
                _ => false,
            },
            ast::ExprRef::FString(left) => match other.0 {
                ast::ExprRef::FString(right) => f_string_fields(left) == f_string_fields(right),
                _ => false,
            },
            ast::ExprRef::TString(left) => match other.0 {
                ast::ExprRef::TString(right) => t_string_fields(left) == t_string_fields(right),
                _ => false,
            },
            ast::ExprRef::StringLiteral(left) => match other.0 {
                ast::ExprRef::StringLiteral(right) => {
                    string_literal_fields(left) == string_literal_fields(right)
                }
                _ => false,
            },
            ast::ExprRef::BytesLiteral(left) => match other.0 {
                ast::ExprRef::BytesLiteral(right) => {
                    bytes_literal_fields(left) == bytes_literal_fields(right)
                }
                _ => false,
            },
            ast::ExprRef::NumberLiteral(left) => match other.0 {
                ast::ExprRef::NumberLiteral(right) => {
                    number_literal_fields(left) == number_literal_fields(right)
                }
                _ => false,
            },
            ast::ExprRef::BooleanLiteral(left) => match other.0 {
                ast::ExprRef::BooleanLiteral(right) => {
                    boolean_literal_fields(left) == boolean_literal_fields(right)
                }
                _ => false,
            },
            ast::ExprRef::NoneLiteral(ast::ExprNoneLiteral {
                range: _,
                node_index: _,
            }) => matches!(other.0, ast::ExprRef::NoneLiteral(_)),
            ast::ExprRef::EllipsisLiteral(ast::ExprEllipsisLiteral {
                range: _,
                node_index: _,
            }) => matches!(other.0, ast::ExprRef::EllipsisLiteral(_)),
            ast::ExprRef::Attribute(left) => match other.0 {
                ast::ExprRef::Attribute(right) => attribute_fields(left) == attribute_fields(right),
                _ => false,
            },
            ast::ExprRef::Subscript(left) => match other.0 {
                ast::ExprRef::Subscript(right) => subscript_fields(left) == subscript_fields(right),
                _ => false,
            },
            ast::ExprRef::Starred(left) => match other.0 {
                ast::ExprRef::Starred(right) => starred_fields(left) == starred_fields(right),
                _ => false,
            },
            ast::ExprRef::Name(left) => match other.0 {
                ast::ExprRef::Name(right) => name_fields(left) == name_fields(right),
                _ => false,
            },
            ast::ExprRef::List(left) => match other.0 {
                ast::ExprRef::List(right) => list_fields(left) == list_fields(right),
                _ => false,
            },
            ast::ExprRef::Tuple(left) => match other.0 {
                ast::ExprRef::Tuple(right) => tuple_fields(left) == tuple_fields(right),
                _ => false,
            },
            ast::ExprRef::Slice(left) => match other.0 {
                ast::ExprRef::Slice(right) => slice_fields(left) == slice_fields(right),
                _ => false,
            },
            ast::ExprRef::IpyEscapeCommand(left) => match other.0 {
                ast::ExprRef::IpyEscapeCommand(right) => {
                    ipy_escape_command_fields(left) == ipy_escape_command_fields(right)
                }
                _ => false,
            },
        }
    }
}

impl Eq for ComparableExpr<'_> {}

impl Hash for ComparableExpr<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(&self.0).hash(state);
        match self.0 {
            ast::ExprRef::BoolOp(node) => bool_op_fields(node).hash(state),
            ast::ExprRef::Named(node) => named_fields(node).hash(state),
            ast::ExprRef::BinOp(node) => bin_op_fields(node).hash(state),
            ast::ExprRef::UnaryOp(node) => unary_op_fields(node).hash(state),
            ast::ExprRef::Lambda(node) => lambda_fields(node).hash(state),
            ast::ExprRef::If(node) => if_fields(node).hash(state),
            ast::ExprRef::Dict(node) => dict_fields(node).hash(state),
            ast::ExprRef::Set(node) => set_fields(node).hash(state),
            ast::ExprRef::ListComp(node) => list_comp_fields(node).hash(state),
            ast::ExprRef::SetComp(node) => set_comp_fields(node).hash(state),
            ast::ExprRef::DictComp(node) => dict_comp_fields(node).hash(state),
            ast::ExprRef::Generator(node) => generator_fields(node).hash(state),
            ast::ExprRef::Await(node) => await_fields(node).hash(state),
            ast::ExprRef::Yield(node) => yield_fields(node).hash(state),
            ast::ExprRef::YieldFrom(node) => yield_from_fields(node).hash(state),
            ast::ExprRef::Compare(node) => compare_fields(node).hash(state),
            ast::ExprRef::Call(node) => call_fields(node).hash(state),
            ast::ExprRef::FString(node) => f_string_fields(node).hash(state),
            ast::ExprRef::TString(node) => t_string_fields(node).hash(state),
            ast::ExprRef::StringLiteral(node) => string_literal_fields(node).hash(state),
            ast::ExprRef::BytesLiteral(node) => bytes_literal_fields(node).hash(state),
            ast::ExprRef::NumberLiteral(node) => number_literal_fields(node).hash(state),
            ast::ExprRef::BooleanLiteral(node) => boolean_literal_fields(node).hash(state),
            ast::ExprRef::NoneLiteral(ast::ExprNoneLiteral {
                range: _,
                node_index: _,
            }) => {}
            ast::ExprRef::EllipsisLiteral(ast::ExprEllipsisLiteral {
                range: _,
                node_index: _,
            }) => {}
            ast::ExprRef::Attribute(node) => attribute_fields(node).hash(state),
            ast::ExprRef::Subscript(node) => subscript_fields(node).hash(state),
            ast::ExprRef::Starred(node) => starred_fields(node).hash(state),
            ast::ExprRef::Name(node) => name_fields(node).hash(state),
            ast::ExprRef::List(node) => list_fields(node).hash(state),
            ast::ExprRef::Tuple(node) => tuple_fields(node).hash(state),
            ast::ExprRef::Slice(node) => slice_fields(node).hash(state),
            ast::ExprRef::IpyEscapeCommand(node) => ipy_escape_command_fields(node).hash(state),
        }
    }
}

impl<'a> From<&'a ast::Expr> for ComparableExpr<'a> {
    fn from(expr: &'a ast::Expr) -> Self {
        Self(expr.into())
    }
}

impl<'a> From<&'a ast::ExprName> for ComparableExpr<'a> {
    fn from(name: &'a ast::ExprName) -> Self {
        Self(name.into())
    }
}

impl<'a> From<&'a Box<ast::Expr>> for ComparableExpr<'a> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        (expr.as_ref()).into()
    }
}

#[derive(PartialEq, Eq, Hash)]
struct BoolOpFields<'a> {
    op: ast::BoolOp,
    values: ComparableExprSlice<'a>,
}

fn bool_op_fields(node: &ast::ExprBoolOp) -> BoolOpFields<'_> {
    let ast::ExprBoolOp {
        op,
        values,
        range: _,
        node_index: _,
    } = node;
    BoolOpFields {
        op: *op,
        values: ComparableSlice::new(values),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct NamedFields<'a> {
    target: ComparableExpr<'a>,
    value: ComparableExpr<'a>,
}

fn named_fields(node: &ast::ExprNamed) -> NamedFields<'_> {
    let ast::ExprNamed {
        target,
        value,
        range: _,
        node_index: _,
    } = node;
    NamedFields {
        target: target.into(),
        value: value.into(),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct BinOpFields<'a> {
    left: ComparableExpr<'a>,
    op: ast::Operator,
    right: ComparableExpr<'a>,
}

fn bin_op_fields(node: &ast::ExprBinOp) -> BinOpFields<'_> {
    let ast::ExprBinOp {
        left,
        op,
        right,
        range: _,
        node_index: _,
    } = node;
    BinOpFields {
        left: left.into(),
        op: *op,
        right: right.into(),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct UnaryOpFields<'a> {
    op: ast::UnaryOp,
    operand: ComparableExpr<'a>,
}

fn unary_op_fields(node: &ast::ExprUnaryOp) -> UnaryOpFields<'_> {
    let ast::ExprUnaryOp {
        op,
        operand,
        range: _,
        node_index: _,
    } = node;
    UnaryOpFields {
        op: *op,
        operand: operand.into(),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct LambdaFields<'a> {
    parameters: Option<ComparableParameters<'a>>,
    body: ComparableExpr<'a>,
}

fn lambda_fields(node: &ast::ExprLambda) -> LambdaFields<'_> {
    let ast::ExprLambda {
        parameters,
        body,
        range: _,
        node_index: _,
    } = node;
    LambdaFields {
        parameters: parameters.as_deref().map(ComparableParameters),
        body: body.into(),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct IfFields<'a> {
    test: ComparableExpr<'a>,
    body: ComparableExpr<'a>,
    orelse: ComparableExpr<'a>,
}

fn if_fields(node: &ast::ExprIf) -> IfFields<'_> {
    let ast::ExprIf {
        test,
        body,
        orelse,
        range: _,
        node_index: _,
    } = node;
    IfFields {
        test: test.into(),
        body: body.into(),
        orelse: orelse.into(),
    }
}

fn dict_fields(node: &ast::ExprDict) -> ComparableSlice<'_, ast::DictItem, ComparableDictItem<'_>> {
    let ast::ExprDict {
        items,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(items)
}

fn set_fields(node: &ast::ExprSet) -> ComparableExprSlice<'_> {
    let ast::ExprSet {
        elts,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(elts)
}

#[derive(PartialEq, Eq, Hash)]
struct ComprehensionExprFields<'a> {
    elt: ComparableExpr<'a>,
    generators: ComparableComprehensionSlice<'a>,
}

impl<'a> ComprehensionExprFields<'a> {
    fn new(elt: &'a Expr, generators: &'a [ast::Comprehension]) -> Self {
        Self {
            elt: elt.into(),
            generators: ComparableSlice::new(generators),
        }
    }
}

fn list_comp_fields(node: &ast::ExprListComp) -> ComprehensionExprFields<'_> {
    let ast::ExprListComp {
        elt,
        generators,
        range: _,
        node_index: _,
    } = node;
    ComprehensionExprFields::new(elt, generators)
}

fn set_comp_fields(node: &ast::ExprSetComp) -> ComprehensionExprFields<'_> {
    let ast::ExprSetComp {
        elt,
        generators,
        range: _,
        node_index: _,
    } = node;
    ComprehensionExprFields::new(elt, generators)
}

fn dict_comp_fields(
    node: &ast::ExprDictComp,
) -> (
    Option<ComparableExpr<'_>>,
    ComparableExpr<'_>,
    ComparableComprehensionSlice<'_>,
) {
    let ast::ExprDictComp {
        key,
        value,
        generators,
        range: _,
        node_index: _,
    } = node;
    (
        key.as_ref().map(Into::into),
        value.into(),
        ComparableSlice::new(generators),
    )
}

fn generator_fields(node: &ast::ExprGenerator) -> ComprehensionExprFields<'_> {
    let ast::ExprGenerator {
        elt,
        generators,
        range: _,
        node_index: _,
        parenthesized: _,
    } = node;
    ComprehensionExprFields::new(elt, generators)
}

fn await_fields(node: &ast::ExprAwait) -> ComparableExpr<'_> {
    let ast::ExprAwait {
        value,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

fn yield_fields(node: &ast::ExprYield) -> Option<ComparableExpr<'_>> {
    let ast::ExprYield {
        value,
        range: _,
        node_index: _,
    } = node;
    value.as_ref().map(Into::into)
}

fn yield_from_fields(node: &ast::ExprYieldFrom) -> ComparableExpr<'_> {
    let ast::ExprYieldFrom {
        value,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

#[derive(PartialEq, Eq, Hash)]
struct CompareFields<'a> {
    ops: &'a [ast::CmpOp],
    operands: ComparableExprSlice<'a>,
}

fn compare_fields(node: &ast::ExprCompare) -> CompareFields<'_> {
    let ast::ExprCompare {
        ops,
        operands,
        range: _,
        node_index: _,
    } = node;
    CompareFields {
        ops,
        operands: ComparableSlice::new(operands),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct CallFields<'a> {
    func: ComparableExpr<'a>,
    arguments: ComparableArguments<'a>,
}

fn call_fields(node: &ast::ExprCall) -> CallFields<'_> {
    let ast::ExprCall {
        func,
        arguments,
        range_start: _,
        node_index: _,
    } = node;
    CallFields {
        func: func.into(),
        arguments: arguments.into(),
    }
}

fn f_string_fields(node: &ast::ExprFString) -> ComparableInterpolatedString<'_> {
    let ast::ExprFString {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableInterpolatedString::FString(value)
}

fn t_string_fields(node: &ast::ExprTString) -> ComparableInterpolatedString<'_> {
    let ast::ExprTString {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableInterpolatedString::TString(value)
}

fn string_literal_fields(node: &ast::ExprStringLiteral) -> &str {
    let ast::ExprStringLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    value.to_str()
}

fn bytes_literal_fields(node: &ast::ExprBytesLiteral) -> ComparableBytes<'_> {
    let ast::ExprBytesLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableBytes(value)
}

fn number_literal_fields(node: &ast::ExprNumberLiteral) -> ComparableNumber<'_> {
    let ast::ExprNumberLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableNumber(value)
}

fn boolean_literal_fields(node: &ast::ExprBooleanLiteral) -> bool {
    let ast::ExprBooleanLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    *value
}

#[derive(PartialEq, Eq, Hash)]
struct AttributeFields<'a> {
    value: ComparableExpr<'a>,
    attr: &'a str,
}

fn attribute_fields(node: &ast::ExprAttribute) -> AttributeFields<'_> {
    let ast::ExprAttribute {
        value,
        attr,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    AttributeFields {
        value: value.into(),
        attr: attr.as_str(),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct SubscriptFields<'a> {
    value: ComparableExpr<'a>,
    slice: ComparableExpr<'a>,
}

fn subscript_fields(node: &ast::ExprSubscript) -> SubscriptFields<'_> {
    let ast::ExprSubscript {
        value,
        slice,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    SubscriptFields {
        value: value.into(),
        slice: slice.into(),
    }
}

fn starred_fields(node: &ast::ExprStarred) -> ComparableExpr<'_> {
    let ast::ExprStarred {
        value,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

fn name_fields(node: &ast::ExprName) -> &str {
    let ast::ExprName {
        id,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    id.as_str()
}

fn list_fields(node: &ast::ExprList) -> ComparableExprSlice<'_> {
    let ast::ExprList {
        elts,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(elts)
}

fn tuple_fields(node: &ast::ExprTuple) -> ComparableExprSlice<'_> {
    let ast::ExprTuple {
        elts,
        ctx: _,
        range: _,
        node_index: _,
        parenthesized: _,
    } = node;
    ComparableSlice::new(elts)
}

#[derive(PartialEq, Eq, Hash)]
struct SliceFields<'a> {
    lower: Option<ComparableExpr<'a>>,
    upper: Option<ComparableExpr<'a>>,
    step: Option<ComparableExpr<'a>>,
}

fn slice_fields(node: &ast::ExprSlice) -> SliceFields<'_> {
    let ast::ExprSlice {
        lower,
        upper,
        step,
        range: _,
        node_index: _,
    } = node;
    SliceFields {
        lower: lower.as_ref().map(Into::into),
        upper: upper.as_ref().map(Into::into),
        step: step.as_ref().map(Into::into),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct IpyEscapeCommandFields<'a> {
    kind: ast::IpyEscapeKind,
    value: &'a str,
}

fn ipy_escape_command_fields(node: &ast::ExprIpyEscapeCommand) -> IpyEscapeCommandFields<'_> {
    let ast::ExprIpyEscapeCommand {
        kind,
        value,
        range: _,
        node_index: _,
    } = node;
    IpyEscapeCommandFields { kind: *kind, value }
}

#[derive(Clone, Copy)]
enum ComparableInterpolatedString<'a> {
    FString(&'a ast::FStringValue),
    TString(&'a ast::TStringValue),
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
            FString(ast::FStringParts<'a>),
            TString(std::slice::Iter<'a, ast::TString>),
        }

        let mut parts = match self {
            Self::FString(value) => Parts::FString(value.iter()),
            Self::TString(value) => Parts::TString(value.as_slice().iter()),
        };
        let mut elements = [].iter();
        std::iter::from_fn(move || {
            loop {
                if let Some(element) = elements.next() {
                    return Some(ComparableStringElement::from(element));
                }
                elements = match &mut parts {
                    Parts::FString(parts) => match parts.next()? {
                        ast::FStringPartRef::FString(ast::FString {
                            elements,
                            range: _,
                            node_index: _,
                            flags: _,
                        }) => elements.iter(),
                        ast::FStringPartRef::Literal(part) => {
                            return Some(ComparableStringElement::Literal(Cow::Borrowed(
                                ComparableText::from(part).0,
                            )));
                        }
                    },
                    Parts::TString(parts) => {
                        let ast::TString {
                            elements,
                            range: _,
                            node_index: _,
                            flags: _,
                        } = parts.next()?;
                        elements.iter()
                    }
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

impl Eq for ComparableInterpolatedString<'_> {}

impl Hash for ComparableInterpolatedString<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        for element in self.normalized_elements() {
            Some(element).hash(state);
        }
        Option::<ComparableStringElement>::None.hash(state);
    }
}

struct ComparableBytes<'a>(&'a ast::BytesLiteralValue);

impl PartialEq for ComparableBytes<'_> {
    fn eq(&self, other: &Self) -> bool {
        if let ([left], [right]) = (self.0.as_slice(), other.0.as_slice()) {
            return left.as_slice() == right.as_slice();
        }

        let mut right_parts = other
            .0
            .iter()
            .map(ast::BytesLiteral::as_slice)
            .filter(|part| !part.is_empty());
        let mut right: &[u8] = &[];
        for part in self.0 {
            let mut left = part.as_slice();
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
        right.is_empty() && right_parts.next().is_none()
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

type ComparableTypeParams<'a> = ComparableSlice<'a, ast::TypeParam, ComparableTypeParam<'a>>;
impl<'a> From<&'a ast::TypeParams> for ComparableTypeParams<'a> {
    fn from(params: &'a ast::TypeParams) -> Self {
        let ast::TypeParams {
            type_params,
            range: _,
            node_index: _,
        } = params;
        Self::new(type_params)
    }
}

struct ComparableTypeParam<'a>(&'a ast::TypeParam);

impl<'a> From<&'a ast::TypeParam> for ComparableTypeParam<'a> {
    fn from(node: &'a ast::TypeParam) -> Self {
        Self(node)
    }
}

impl PartialEq for ComparableTypeParam<'_> {
    fn eq(&self, other: &Self) -> bool {
        match self.0 {
            ast::TypeParam::TypeVar(left) => match other.0 {
                ast::TypeParam::TypeVar(right) => type_var_fields(left) == type_var_fields(right),
                _ => false,
            },
            ast::TypeParam::TypeVarTuple(left) => match other.0 {
                ast::TypeParam::TypeVarTuple(right) => {
                    type_var_tuple_fields(left) == type_var_tuple_fields(right)
                }
                _ => false,
            },
            ast::TypeParam::ParamSpec(left) => match other.0 {
                ast::TypeParam::ParamSpec(right) => {
                    param_spec_fields(left) == param_spec_fields(right)
                }
                _ => false,
            },
        }
    }
}

impl Eq for ComparableTypeParam<'_> {}

impl Hash for ComparableTypeParam<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self.0).hash(state);
        match self.0 {
            ast::TypeParam::TypeVar(node) => type_var_fields(node).hash(state),
            ast::TypeParam::TypeVarTuple(node) => {
                type_var_tuple_fields(node).hash(state);
            }
            ast::TypeParam::ParamSpec(node) => param_spec_fields(node).hash(state),
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
struct TypeVarFields<'a> {
    name: &'a str,
    bound: Option<ComparableExpr<'a>>,
    default: Option<ComparableExpr<'a>>,
}

fn type_var_fields(node: &ast::TypeParamTypeVar) -> TypeVarFields<'_> {
    let ast::TypeParamTypeVar {
        name,
        bound,
        default,
        range: _,
        node_index: _,
    } = node;
    TypeVarFields {
        name: name.as_str(),
        bound: bound.as_ref().map(Into::into),
        default: default.as_ref().map(Into::into),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct VariadicTypeParamFields<'a> {
    name: &'a str,
    default: Option<ComparableExpr<'a>>,
}

fn type_var_tuple_fields(node: &ast::TypeParamTypeVarTuple) -> VariadicTypeParamFields<'_> {
    let ast::TypeParamTypeVarTuple {
        name,
        default,
        range: _,
        node_index: _,
    } = node;
    VariadicTypeParamFields {
        name: name.as_str(),
        default: default.as_ref().map(Into::into),
    }
}

fn param_spec_fields(node: &ast::TypeParamParamSpec) -> VariadicTypeParamFields<'_> {
    let ast::TypeParamParamSpec {
        name,
        default,
        range: _,
        node_index: _,
    } = node;
    VariadicTypeParamFields {
        name: name.as_str(),
        default: default.as_ref().map(Into::into),
    }
}

/// A borrowed structural comparison key for a statement.
#[derive(Clone, Copy)]
pub struct ComparableStmt<'a>(&'a ast::Stmt);

impl<'a> From<&'a ast::Stmt> for ComparableStmt<'a> {
    fn from(node: &'a ast::Stmt) -> Self {
        Self(node)
    }
}

impl fmt::Debug for ComparableStmt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for ComparableStmt<'_> {
    fn eq(&self, other: &Self) -> bool {
        match self.0 {
            ast::Stmt::FunctionDef(left) => match other.0 {
                ast::Stmt::FunctionDef(right) => {
                    stmt_function_def_fields(left) == stmt_function_def_fields(right)
                }
                _ => false,
            },
            ast::Stmt::ClassDef(left) => match other.0 {
                ast::Stmt::ClassDef(right) => {
                    stmt_class_def_fields(left) == stmt_class_def_fields(right)
                }
                _ => false,
            },
            ast::Stmt::Return(left) => match other.0 {
                ast::Stmt::Return(right) => stmt_return_fields(left) == stmt_return_fields(right),
                _ => false,
            },
            ast::Stmt::Delete(left) => match other.0 {
                ast::Stmt::Delete(right) => stmt_delete_fields(left) == stmt_delete_fields(right),
                _ => false,
            },
            ast::Stmt::TypeAlias(left) => match other.0 {
                ast::Stmt::TypeAlias(right) => {
                    stmt_type_alias_fields(left) == stmt_type_alias_fields(right)
                }
                _ => false,
            },
            ast::Stmt::Assign(left) => match other.0 {
                ast::Stmt::Assign(right) => stmt_assign_fields(left) == stmt_assign_fields(right),
                _ => false,
            },
            ast::Stmt::AugAssign(left) => match other.0 {
                ast::Stmt::AugAssign(right) => {
                    stmt_aug_assign_fields(left) == stmt_aug_assign_fields(right)
                }
                _ => false,
            },
            ast::Stmt::AnnAssign(left) => match other.0 {
                ast::Stmt::AnnAssign(right) => {
                    stmt_ann_assign_fields(left) == stmt_ann_assign_fields(right)
                }
                _ => false,
            },
            ast::Stmt::For(left) => match other.0 {
                ast::Stmt::For(right) => stmt_for_fields(left) == stmt_for_fields(right),
                _ => false,
            },
            ast::Stmt::While(left) => match other.0 {
                ast::Stmt::While(right) => stmt_while_fields(left) == stmt_while_fields(right),
                _ => false,
            },
            ast::Stmt::If(left) => match other.0 {
                ast::Stmt::If(right) => stmt_if_fields(left) == stmt_if_fields(right),
                _ => false,
            },
            ast::Stmt::With(left) => match other.0 {
                ast::Stmt::With(right) => stmt_with_fields(left) == stmt_with_fields(right),
                _ => false,
            },
            ast::Stmt::Match(left) => match other.0 {
                ast::Stmt::Match(right) => stmt_match_fields(left) == stmt_match_fields(right),
                _ => false,
            },
            ast::Stmt::Raise(left) => match other.0 {
                ast::Stmt::Raise(right) => stmt_raise_fields(left) == stmt_raise_fields(right),
                _ => false,
            },
            ast::Stmt::Try(left) => match other.0 {
                ast::Stmt::Try(right) => stmt_try_fields(left) == stmt_try_fields(right),
                _ => false,
            },
            ast::Stmt::Assert(left) => match other.0 {
                ast::Stmt::Assert(right) => stmt_assert_fields(left) == stmt_assert_fields(right),
                _ => false,
            },
            ast::Stmt::Import(left) => match other.0 {
                ast::Stmt::Import(right) => stmt_import_fields(left) == stmt_import_fields(right),
                _ => false,
            },
            ast::Stmt::ImportFrom(left) => match other.0 {
                ast::Stmt::ImportFrom(right) => {
                    stmt_import_from_fields(left) == stmt_import_from_fields(right)
                }
                _ => false,
            },
            ast::Stmt::Global(left) => match other.0 {
                ast::Stmt::Global(right) => stmt_global_fields(left) == stmt_global_fields(right),
                _ => false,
            },
            ast::Stmt::Nonlocal(left) => match other.0 {
                ast::Stmt::Nonlocal(right) => {
                    stmt_nonlocal_fields(left) == stmt_nonlocal_fields(right)
                }
                _ => false,
            },
            ast::Stmt::IpyEscapeCommand(left) => match other.0 {
                ast::Stmt::IpyEscapeCommand(right) => {
                    stmt_ipy_escape_command_fields(left) == stmt_ipy_escape_command_fields(right)
                }
                _ => false,
            },
            ast::Stmt::Expr(left) => match other.0 {
                ast::Stmt::Expr(right) => stmt_expr_fields(left) == stmt_expr_fields(right),
                _ => false,
            },
            ast::Stmt::Pass(ast::StmtPass {
                range: _,
                node_index: _,
            }) => matches!(other.0, ast::Stmt::Pass(_)),
            ast::Stmt::Break(ast::StmtBreak {
                range: _,
                node_index: _,
            }) => matches!(other.0, ast::Stmt::Break(_)),
            ast::Stmt::Continue(ast::StmtContinue {
                range: _,
                node_index: _,
            }) => matches!(other.0, ast::Stmt::Continue(_)),
        }
    }
}

impl Eq for ComparableStmt<'_> {}

impl Hash for ComparableStmt<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self.0).hash(state);
        match self.0 {
            ast::Stmt::FunctionDef(node) => stmt_function_def_fields(node).hash(state),
            ast::Stmt::ClassDef(node) => stmt_class_def_fields(node).hash(state),
            ast::Stmt::Return(node) => stmt_return_fields(node).hash(state),
            ast::Stmt::Delete(node) => stmt_delete_fields(node).hash(state),
            ast::Stmt::TypeAlias(node) => stmt_type_alias_fields(node).hash(state),
            ast::Stmt::Assign(node) => stmt_assign_fields(node).hash(state),
            ast::Stmt::AugAssign(node) => stmt_aug_assign_fields(node).hash(state),
            ast::Stmt::AnnAssign(node) => stmt_ann_assign_fields(node).hash(state),
            ast::Stmt::For(node) => stmt_for_fields(node).hash(state),
            ast::Stmt::While(node) => stmt_while_fields(node).hash(state),
            ast::Stmt::If(node) => stmt_if_fields(node).hash(state),
            ast::Stmt::With(node) => stmt_with_fields(node).hash(state),
            ast::Stmt::Match(node) => stmt_match_fields(node).hash(state),
            ast::Stmt::Raise(node) => stmt_raise_fields(node).hash(state),
            ast::Stmt::Try(node) => stmt_try_fields(node).hash(state),
            ast::Stmt::Assert(node) => stmt_assert_fields(node).hash(state),
            ast::Stmt::Import(node) => stmt_import_fields(node).hash(state),
            ast::Stmt::ImportFrom(node) => stmt_import_from_fields(node).hash(state),
            ast::Stmt::Global(node) => stmt_global_fields(node).hash(state),
            ast::Stmt::Nonlocal(node) => stmt_nonlocal_fields(node).hash(state),
            ast::Stmt::IpyEscapeCommand(node) => stmt_ipy_escape_command_fields(node).hash(state),
            ast::Stmt::Expr(node) => stmt_expr_fields(node).hash(state),
            ast::Stmt::Pass(ast::StmtPass {
                range: _,
                node_index: _,
            }) => {}
            ast::Stmt::Break(ast::StmtBreak {
                range: _,
                node_index: _,
            }) => {}
            ast::Stmt::Continue(ast::StmtContinue {
                range: _,
                node_index: _,
            }) => {}
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtFunctionDefFields<'a> {
    is_async: bool,
    decorator_list: ComparableSlice<'a, ast::Decorator, ComparableExpr<'a>>,
    name: &'a str,
    type_params: Option<ComparableTypeParams<'a>>,
    parameters: ComparableParameters<'a>,
    returns: Option<ComparableExpr<'a>>,
    body: ComparableStmtSlice<'a>,
}

fn stmt_function_def_fields(node: &ast::StmtFunctionDef) -> StmtFunctionDefFields<'_> {
    let ast::StmtFunctionDef {
        is_async,
        name,
        parameters,
        body,
        decorator_list,
        returns,
        type_params,
        range: _,
        node_index: _,
    } = node;
    StmtFunctionDefFields {
        is_async: *is_async,
        name: name.as_str(),
        parameters: ComparableParameters(parameters),
        body: ComparableSlice::new(body),
        decorator_list: ComparableSlice::new(decorator_list),
        returns: returns.as_ref().map(Into::into),
        type_params: type_params.as_deref().map(Into::into),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtClassDefFields<'a> {
    decorator_list: ComparableSlice<'a, ast::Decorator, ComparableExpr<'a>>,
    name: &'a str,
    type_params: Option<ComparableTypeParams<'a>>,
    arguments: ComparableArguments<'a>,
    body: ComparableStmtSlice<'a>,
}

fn stmt_class_def_fields(node: &ast::StmtClassDef) -> StmtClassDefFields<'_> {
    let ast::StmtClassDef {
        name,
        arguments,
        body,
        decorator_list,
        type_params,
        range: _,
        node_index: _,
    } = node;
    StmtClassDefFields {
        name: name.as_str(),
        arguments: arguments.as_deref().map(Into::into).unwrap_or_default(),
        body: ComparableSlice::new(body),
        decorator_list: ComparableSlice::new(decorator_list),
        type_params: type_params.as_deref().map(Into::into),
    }
}

fn stmt_return_fields(node: &ast::StmtReturn) -> Option<ComparableExpr<'_>> {
    let ast::StmtReturn {
        value,
        range: _,
        node_index: _,
    } = node;
    value.as_ref().map(Into::into)
}

fn stmt_delete_fields(node: &ast::StmtDelete) -> ComparableExprSlice<'_> {
    let ast::StmtDelete {
        targets,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(targets)
}

#[derive(PartialEq, Eq, Hash)]
struct StmtTypeAliasFields<'a> {
    name: ComparableExpr<'a>,
    type_params: Option<ComparableTypeParams<'a>>,
    value: ComparableExpr<'a>,
}

fn stmt_type_alias_fields(node: &ast::StmtTypeAlias) -> StmtTypeAliasFields<'_> {
    let ast::StmtTypeAlias {
        range: _,
        node_index: _,
        name,
        type_params,
        value,
    } = node;
    StmtTypeAliasFields {
        name: name.into(),
        type_params: type_params.as_deref().map(Into::into),
        value: value.into(),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtAssignFields<'a> {
    targets: ComparableExprSlice<'a>,
    value: ComparableExpr<'a>,
}

fn stmt_assign_fields(node: &ast::StmtAssign) -> StmtAssignFields<'_> {
    let ast::StmtAssign {
        targets,
        value,
        range: _,
        node_index: _,
    } = node;
    StmtAssignFields {
        targets: ComparableSlice::new(targets),
        value: value.into(),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtAugAssignFields<'a> {
    target: ComparableExpr<'a>,
    op: ast::Operator,
    value: ComparableExpr<'a>,
}

fn stmt_aug_assign_fields(node: &ast::StmtAugAssign) -> StmtAugAssignFields<'_> {
    let ast::StmtAugAssign {
        target,
        op,
        value,
        range: _,
        node_index: _,
    } = node;
    StmtAugAssignFields {
        target: target.into(),
        op: *op,
        value: value.into(),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtAnnAssignFields<'a> {
    target: ComparableExpr<'a>,
    annotation: ComparableExpr<'a>,
    value: Option<ComparableExpr<'a>>,
    simple: bool,
}

fn stmt_ann_assign_fields(node: &ast::StmtAnnAssign) -> StmtAnnAssignFields<'_> {
    let ast::StmtAnnAssign {
        target,
        annotation,
        value,
        simple,
        range: _,
        node_index: _,
    } = node;
    StmtAnnAssignFields {
        target: target.into(),
        annotation: annotation.into(),
        value: value.as_ref().map(Into::into),
        simple: *simple,
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtForFields<'a> {
    is_async: bool,
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    body: ComparableStmtSlice<'a>,
    orelse: ComparableStmtSlice<'a>,
}

fn stmt_for_fields(node: &ast::StmtFor) -> StmtForFields<'_> {
    let ast::StmtFor {
        is_async,
        target,
        iter,
        body,
        orelse,
        range: _,
        node_index: _,
    } = node;
    StmtForFields {
        is_async: *is_async,
        target: target.into(),
        iter: iter.into(),
        body: ComparableSlice::new(body),
        orelse: ComparableSlice::new(orelse),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtWhileFields<'a> {
    test: ComparableExpr<'a>,
    body: ComparableStmtSlice<'a>,
    orelse: ComparableStmtSlice<'a>,
}

fn stmt_while_fields(node: &ast::StmtWhile) -> StmtWhileFields<'_> {
    let ast::StmtWhile {
        test,
        body,
        orelse,
        range: _,
        node_index: _,
    } = node;
    StmtWhileFields {
        test: test.into(),
        body: ComparableSlice::new(body),
        orelse: ComparableSlice::new(orelse),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtIfFields<'a> {
    test: ComparableExpr<'a>,
    body: ComparableStmtSlice<'a>,
    elif_else_clauses: ComparableSlice<'a, ast::ElifElseClause, ComparableElifElseClause<'a>>,
}

fn stmt_if_fields(node: &ast::StmtIf) -> StmtIfFields<'_> {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        range: _,
        node_index: _,
    } = node;
    StmtIfFields {
        test: test.into(),
        body: ComparableSlice::new(body),
        elif_else_clauses: ComparableSlice::new(elif_else_clauses),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtWithFields<'a> {
    is_async: bool,
    items: ComparableSlice<'a, ast::WithItem, ComparableWithItem<'a>>,
    body: ComparableStmtSlice<'a>,
}

fn stmt_with_fields(node: &ast::StmtWith) -> StmtWithFields<'_> {
    let ast::StmtWith {
        is_async,
        items,
        body,
        range: _,
        node_index: _,
    } = node;
    StmtWithFields {
        is_async: *is_async,
        items: ComparableSlice::new(items),
        body: ComparableSlice::new(body),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtMatchFields<'a> {
    subject: ComparableExpr<'a>,
    cases: ComparableSlice<'a, ast::MatchCase, ComparableMatchCase<'a>>,
}

fn stmt_match_fields(node: &ast::StmtMatch) -> StmtMatchFields<'_> {
    let ast::StmtMatch {
        subject,
        cases,
        range: _,
        node_index: _,
    } = node;
    StmtMatchFields {
        subject: subject.into(),
        cases: ComparableSlice::new(cases),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtRaiseFields<'a> {
    exc: Option<ComparableExpr<'a>>,
    cause: Option<ComparableExpr<'a>>,
}

fn stmt_raise_fields(node: &ast::StmtRaise) -> StmtRaiseFields<'_> {
    let ast::StmtRaise {
        exc,
        cause,
        range: _,
        node_index: _,
    } = node;
    StmtRaiseFields {
        exc: exc.as_ref().map(Into::into),
        cause: cause.as_ref().map(Into::into),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtTryFields<'a> {
    body: ComparableStmtSlice<'a>,
    handlers: ComparableSlice<'a, ast::ExceptHandler, ComparableExceptHandler<'a>>,
    orelse: ComparableStmtSlice<'a>,
    finalbody: ComparableStmtSlice<'a>,
    is_star: bool,
}

fn stmt_try_fields(node: &ast::StmtTry) -> StmtTryFields<'_> {
    let ast::StmtTry {
        body,
        handlers,
        orelse,
        finalbody,
        is_star,
        range: _,
        node_index: _,
    } = node;
    StmtTryFields {
        body: ComparableSlice::new(body),
        handlers: ComparableSlice::new(handlers),
        orelse: ComparableSlice::new(orelse),
        finalbody: ComparableSlice::new(finalbody),
        is_star: *is_star,
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtAssertFields<'a> {
    test: ComparableExpr<'a>,
    msg: Option<ComparableExpr<'a>>,
}

fn stmt_assert_fields(node: &ast::StmtAssert) -> StmtAssertFields<'_> {
    let ast::StmtAssert {
        test,
        msg,
        range: _,
        node_index: _,
    } = node;
    StmtAssertFields {
        test: test.into(),
        msg: msg.as_ref().map(Into::into),
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtImportFields<'a> {
    names: ComparableSlice<'a, ast::Alias, ComparableAlias<'a>>,
    is_lazy: bool,
}

fn stmt_import_fields(node: &ast::StmtImport) -> StmtImportFields<'_> {
    let ast::StmtImport {
        names,
        is_lazy,
        range: _,
        node_index: _,
    } = node;
    StmtImportFields {
        names: ComparableSlice::new(names),
        is_lazy: *is_lazy,
    }
}

#[derive(PartialEq, Eq, Hash)]
struct StmtImportFromFields<'a> {
    module: Option<&'a str>,
    names: ComparableSlice<'a, ast::Alias, ComparableAlias<'a>>,
    level: u32,
    is_lazy: bool,
}

fn stmt_import_from_fields(node: &ast::StmtImportFrom) -> StmtImportFromFields<'_> {
    let ast::StmtImportFrom {
        module,
        names,
        level,
        is_lazy,
        range: _,
        node_index: _,
    } = node;
    StmtImportFromFields {
        module: module.as_deref(),
        names: ComparableSlice::new(names),
        level: *level,
        is_lazy: *is_lazy,
    }
}

fn stmt_global_fields(
    node: &ast::StmtGlobal,
) -> ComparableSlice<'_, ast::Identifier, ComparableText<'_>> {
    let ast::StmtGlobal {
        names,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(names)
}

fn stmt_nonlocal_fields(
    node: &ast::StmtNonlocal,
) -> ComparableSlice<'_, ast::Identifier, ComparableText<'_>> {
    let ast::StmtNonlocal {
        names,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(names)
}

fn stmt_ipy_escape_command_fields(node: &ast::StmtIpyEscapeCommand) -> IpyEscapeCommandFields<'_> {
    let ast::StmtIpyEscapeCommand {
        kind,
        value,
        range: _,
        node_index: _,
    } = node;
    IpyEscapeCommandFields { kind: *kind, value }
}

fn stmt_expr_fields(node: &ast::StmtExpr) -> ComparableExpr<'_> {
    let ast::StmtExpr {
        value,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

#[derive(Clone, Copy)]
pub struct ComparableMod<'a>(&'a ast::Mod);

impl<'a> From<&'a ast::Mod> for ComparableMod<'a> {
    fn from(node: &'a ast::Mod) -> Self {
        Self(node)
    }
}

impl fmt::Debug for ComparableMod<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for ComparableMod<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self.0, other.0) {
            (ast::Mod::Module(left), ast::Mod::Module(right)) => {
                mod_module_fields(left) == mod_module_fields(right)
            }
            (ast::Mod::Expression(left), ast::Mod::Expression(right)) => {
                mod_expression_fields(left) == mod_expression_fields(right)
            }
            (ast::Mod::Module(_), ast::Mod::Expression(_))
            | (ast::Mod::Expression(_), ast::Mod::Module(_)) => false,
        }
    }
}

impl Eq for ComparableMod<'_> {}

impl Hash for ComparableMod<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self.0).hash(state);
        match self.0 {
            ast::Mod::Module(node) => mod_module_fields(node).hash(state),
            ast::Mod::Expression(node) => mod_expression_fields(node).hash(state),
        }
    }
}

fn mod_module_fields(node: &ast::ModModule) -> ComparableStmtSlice<'_> {
    let ast::ModModule {
        body,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(body)
}

fn mod_expression_fields(node: &ast::ModExpression) -> ComparableExpr<'_> {
    let ast::ModExpression {
        body,
        range: _,
        node_index: _,
    } = node;
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

impl<'a> From<&'a Expr> for HashableExpr<'a> {
    fn from(expr: &'a Expr) -> Self {
        /// Returns a version of the given expression that can be hashed and compared according to
        /// Python  semantics.
        fn as_hashable(expr: &Expr) -> HashableExprKind<'_> {
            if let Some(constant) = as_hashable_constant(expr) {
                return constant;
            }

            match expr {
                Expr::Named(named) => HashableExprKind::NamedExpr {
                    target: ComparableExpr::from(&named.target),
                    value: Box::new(as_hashable(&named.value)),
                },
                _ => HashableExprKind::Comparable(ComparableExpr::from(expr)),
            }
        }

        /// Returns a hashable representation if the expression's value is statically known.
        fn as_hashable_constant(expr: &Expr) -> Option<HashableExprKind<'_>> {
            if let Some(number) = as_number(expr) {
                return Some(HashableExprKind::Number(number));
            }

            let kind = match expr {
                Expr::Tuple(tuple) => HashableExprKind::Tuple(
                    tuple
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

        fn as_number(expr: &Expr) -> Option<HashableNumber> {
            match expr {
                Expr::BooleanLiteral(boolean) => Some(HashableNumber::real(HashableReal::Integer(
                    i128::from(u8::from(boolean.value)),
                ))),
                Expr::NumberLiteral(number) => match &number.value {
                    Number::Int(int) => HashableReal::from_int(int).map(HashableNumber::real),
                    Number::Float(float) => {
                        Some(HashableNumber::real(HashableReal::from_float(*float)))
                    }
                    Number::Complex { real, imag } => Some(HashableNumber::complex(
                        HashableReal::from_float(*real),
                        HashableReal::from_float(*imag),
                    )),
                },
                Expr::UnaryOp(ast::ExprUnaryOp { op, operand, .. }) => match op {
                    ast::UnaryOp::UAdd => as_number(operand),
                    ast::UnaryOp::USub => as_number(operand).map(HashableNumber::negate),
                    ast::UnaryOp::Invert | ast::UnaryOp::Not => None,
                },
                Expr::BinOp(ast::ExprBinOp {
                    left,
                    op: op @ (ast::Operator::Add | ast::Operator::Sub),
                    right,
                    ..
                }) => {
                    let real = as_number(left)?.into_real()?.into_float();
                    let Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value:
                            Number::Complex {
                                real: complex_real,
                                imag,
                            },
                        ..
                    }) = right.as_ref()
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
