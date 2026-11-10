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
        ComparablePatternFields::from(self.0).fmt(f)
    }
}

impl PartialEq for ComparablePattern<'_> {
    fn eq(&self, other: &Self) -> bool {
        ComparablePatternFields::from(self.0) == ComparablePatternFields::from(other.0)
    }
}

impl Eq for ComparablePattern<'_> {}

impl Hash for ComparablePattern<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ComparablePatternFields::from(self.0).hash(state);
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparablePatternFields<'a> {
    Value {
        value: ComparableExpr<'a>,
    },
    Singleton {
        value: ast::Singleton,
    },
    Sequence {
        patterns: ComparablePatternSlice<'a>,
    },
    Mapping {
        keys: ComparableExprSlice<'a>,
        patterns: ComparablePatternSlice<'a>,
        rest: Option<&'a str>,
    },
    Class {
        cls: ComparableExpr<'a>,
        patterns: ComparablePatternSlice<'a>,
        keywords: ComparableSlice<'a, ast::PatternKeyword, ComparablePatternKeyword<'a>>,
    },
    Star {
        name: Option<&'a str>,
    },
    As {
        pattern: Option<ComparablePattern<'a>>,
        name: Option<&'a str>,
    },
    Or {
        patterns: ComparablePatternSlice<'a>,
    },
}

impl<'a> From<&'a ast::Pattern> for ComparablePatternFields<'a> {
    fn from(node: &'a ast::Pattern) -> Self {
        match node {
            ast::Pattern::MatchValue(ast::PatternMatchValue {
                value,
                range: _,
                node_index: _,
            }) => Self::Value {
                value: value.into(),
            },
            ast::Pattern::MatchSingleton(ast::PatternMatchSingleton {
                value,
                range: _,
                node_index: _,
            }) => Self::Singleton { value: *value },
            ast::Pattern::MatchSequence(ast::PatternMatchSequence {
                patterns,
                range: _,
                node_index: _,
            }) => Self::Sequence {
                patterns: ComparableSlice::new(patterns),
            },
            ast::Pattern::MatchMapping(ast::PatternMatchMapping {
                keys,
                patterns,
                rest,
                range: _,
                node_index: _,
            }) => Self::Mapping {
                keys: ComparableSlice::new(keys),
                patterns: ComparableSlice::new(patterns),
                rest: rest.as_deref(),
            },
            ast::Pattern::MatchClass(ast::PatternMatchClass {
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
            }) => Self::Class {
                cls: cls.into(),
                patterns: ComparableSlice::new(patterns),
                keywords: ComparableSlice::new(keywords),
            },
            ast::Pattern::MatchStar(ast::PatternMatchStar {
                name,
                range: _,
                node_index: _,
            }) => Self::Star {
                name: name.as_deref(),
            },
            ast::Pattern::MatchAs(ast::PatternMatchAs {
                pattern,
                name,
                range: _,
                node_index: _,
            }) => Self::As {
                pattern: pattern.as_deref().map(Into::into),
                name: name.as_deref(),
            },
            ast::Pattern::MatchOr(ast::PatternMatchOr {
                patterns,
                range: _,
                node_index: _,
            }) => Self::Or {
                patterns: ComparableSlice::new(patterns),
            },
        }
    }
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

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparableNumber<'a> {
    Int(&'a ast::Int),
    Float(u64),
    Complex { real: u64, imag: u64 },
}

impl<'a> From<&'a ast::Number> for ComparableNumber<'a> {
    fn from(number: &'a ast::Number) -> Self {
        match number {
            ast::Number::Int(value) => Self::Int(value),
            ast::Number::Float(value) => Self::Float(value.to_bits()),
            ast::Number::Complex { real, imag } => Self::Complex {
                real: real.to_bits(),
                imag: imag.to_bits(),
            },
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

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparableInterpolatedStringElement<'a> {
    Literal(Cow<'a, str>),
    InterpolatedElement(InterpolatedElement<'a>),
}

/// Whitespace in debug text is part of the runtime value: `f"{x =}"` and
/// `f"{x=}"` differ. Python normalizes CR and CRLF to LF at compile time.
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
struct InterpolatedElement<'a> {
    expression: ComparableExpr<'a>,
    debug_text: Option<Cow<'a, str>>,
    conversion: ast::ConversionFlag,
    format_spec: Option<
        ComparableSlice<
            'a,
            ast::InterpolatedStringElement,
            ComparableInterpolatedStringElement<'a>,
        >,
    >,
}

impl<'a> From<&'a ast::InterpolatedStringElement> for ComparableInterpolatedStringElement<'a> {
    fn from(interpolated_string_element: &'a ast::InterpolatedStringElement) -> Self {
        match interpolated_string_element {
            ast::InterpolatedStringElement::Literal(ast::InterpolatedStringLiteralElement {
                value,
                range: _,
                node_index: _,
            }) => Self::Literal(value.as_ref().into()),
            ast::InterpolatedStringElement::Interpolation(formatted_value) => {
                formatted_value.into()
            }
        }
    }
}

impl<'a> From<&'a ast::InterpolatedElement> for InterpolatedElement<'a> {
    fn from(interpolated_element: &'a ast::InterpolatedElement) -> Self {
        let ast::InterpolatedElement {
            expression,
            debug_text,
            conversion,
            format_spec,
            range: _,
            node_index: _,
        } = interpolated_element;

        Self {
            expression: (expression).into(),
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
}

impl<'a> From<&'a ast::InterpolatedElement> for ComparableInterpolatedStringElement<'a> {
    fn from(interpolated_element: &'a ast::InterpolatedElement) -> Self {
        Self::InterpolatedElement(interpolated_element.into())
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableLiteral<'a>(ComparableLiteralFields<'a>);

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparableLiteralFields<'a> {
    None,
    Ellipsis,
    Bool(&'a bool),
    Str(ComparableSlice<'a, ast::StringLiteral, ComparableText<'a>>),
    Bytes(ComparableSlice<'a, ast::BytesLiteral, ComparableBytesLiteral<'a>>),
    Number(ComparableNumber<'a>),
}

impl<'a> From<ast::LiteralExpressionRef<'a>> for ComparableLiteral<'a> {
    fn from(literal: ast::LiteralExpressionRef<'a>) -> Self {
        Self(match literal {
            ast::LiteralExpressionRef::NoneLiteral(ast::ExprNoneLiteral {
                range: _,
                node_index: _,
            }) => ComparableLiteralFields::None,
            ast::LiteralExpressionRef::EllipsisLiteral(ast::ExprEllipsisLiteral {
                range: _,
                node_index: _,
            }) => ComparableLiteralFields::Ellipsis,
            ast::LiteralExpressionRef::BooleanLiteral(ast::ExprBooleanLiteral {
                value,
                range: _,
                node_index: _,
            }) => ComparableLiteralFields::Bool(value),
            ast::LiteralExpressionRef::StringLiteral(ast::ExprStringLiteral {
                value,
                range: _,
                node_index: _,
            }) => ComparableLiteralFields::Str(ComparableSlice::new(value.as_slice())),
            ast::LiteralExpressionRef::BytesLiteral(ast::ExprBytesLiteral {
                value,
                range: _,
                node_index: _,
            }) => ComparableLiteralFields::Bytes(ComparableSlice::new(value.as_slice())),
            ast::LiteralExpressionRef::NumberLiteral(ast::ExprNumberLiteral {
                value,
                range: _,
                node_index: _,
            }) => ComparableLiteralFields::Number(value.into()),
        })
    }
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
        ComparableExprFields::from(self.0).fmt(f)
    }
}

impl PartialEq for ComparableExpr<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(&self.0) == std::mem::discriminant(&other.0)
            && ComparableExprFields::from(self.0) == ComparableExprFields::from(other.0)
    }
}

impl Eq for ComparableExpr<'_> {}

impl Hash for ComparableExpr<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ComparableExprFields::from(self.0).hash(state);
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

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparableExprFields<'a> {
    BoolOp {
        op: ast::BoolOp,
        values: ComparableExprSlice<'a>,
    },
    Named {
        target: ComparableExpr<'a>,
        value: ComparableExpr<'a>,
    },
    BinOp {
        left: ComparableExpr<'a>,
        op: ast::Operator,
        right: ComparableExpr<'a>,
    },
    UnaryOp {
        op: ast::UnaryOp,
        operand: ComparableExpr<'a>,
    },
    Lambda {
        parameters: Option<ComparableParameters<'a>>,
        body: ComparableExpr<'a>,
    },
    If {
        test: ComparableExpr<'a>,
        body: ComparableExpr<'a>,
        orelse: ComparableExpr<'a>,
    },
    Dict {
        items: ComparableSlice<'a, ast::DictItem, ComparableDictItem<'a>>,
    },
    Set {
        elts: ComparableExprSlice<'a>,
    },
    ListComp {
        elt: ComparableExpr<'a>,
        generators: ComparableSlice<'a, ast::Comprehension, ComparableComprehension<'a>>,
    },
    SetComp {
        elt: ComparableExpr<'a>,
        generators: ComparableSlice<'a, ast::Comprehension, ComparableComprehension<'a>>,
    },
    DictComp {
        key: Option<ComparableExpr<'a>>,
        value: ComparableExpr<'a>,
        generators: ComparableSlice<'a, ast::Comprehension, ComparableComprehension<'a>>,
    },
    Generator {
        elt: ComparableExpr<'a>,
        generators: ComparableSlice<'a, ast::Comprehension, ComparableComprehension<'a>>,
    },
    Await {
        value: ComparableExpr<'a>,
    },
    Yield {
        value: Option<ComparableExpr<'a>>,
    },
    YieldFrom {
        value: ComparableExpr<'a>,
    },
    Compare {
        ops: &'a [ast::CmpOp],
        operands: ComparableExprSlice<'a>,
    },
    Call {
        func: ComparableExpr<'a>,
        arguments: ComparableArguments<'a>,
    },
    FString {
        elements: Box<[ComparableInterpolatedStringElement<'a>]>,
    },
    TString {
        strings: Box<[Cow<'a, str>]>,
        interpolations: Box<[InterpolatedElement<'a>]>,
    },
    StringLiteral {
        value: &'a str,
    },
    BytesLiteral {
        value: Cow<'a, [u8]>,
    },
    NumberLiteral {
        value: ComparableNumber<'a>,
    },
    BooleanLiteral {
        value: bool,
    },
    NoneLiteral,
    EllipsisLiteral,
    Attribute {
        value: ComparableExpr<'a>,
        attr: &'a str,
    },
    Subscript {
        value: ComparableExpr<'a>,
        slice: ComparableExpr<'a>,
    },
    Starred {
        value: ComparableExpr<'a>,
    },
    Name {
        id: &'a str,
    },
    List {
        elts: ComparableExprSlice<'a>,
    },
    Tuple {
        elts: ComparableExprSlice<'a>,
    },
    Slice {
        lower: Option<ComparableExpr<'a>>,
        upper: Option<ComparableExpr<'a>>,
        step: Option<ComparableExpr<'a>>,
    },
    IpyEscapeCommand {
        kind: ast::IpyEscapeKind,
        value: &'a str,
    },
}

impl<'a> From<ast::ExprRef<'a>> for ComparableExprFields<'a> {
    fn from(node: ast::ExprRef<'a>) -> Self {
        match node {
            ast::ExprRef::BoolOp(ast::ExprBoolOp {
                op,
                values,
                range: _,
                node_index: _,
            }) => Self::BoolOp {
                op: *op,
                values: ComparableSlice::new(values),
            },
            ast::ExprRef::Named(ast::ExprNamed {
                target,
                value,
                range: _,
                node_index: _,
            }) => Self::Named {
                target: target.into(),
                value: value.into(),
            },
            ast::ExprRef::BinOp(ast::ExprBinOp {
                left,
                op,
                right,
                range: _,
                node_index: _,
            }) => Self::BinOp {
                left: left.into(),
                op: *op,
                right: right.into(),
            },
            ast::ExprRef::UnaryOp(ast::ExprUnaryOp {
                op,
                operand,
                range: _,
                node_index: _,
            }) => Self::UnaryOp {
                op: *op,
                operand: operand.into(),
            },
            ast::ExprRef::Lambda(ast::ExprLambda {
                parameters,
                body,
                range: _,
                node_index: _,
            }) => Self::Lambda {
                parameters: parameters.as_deref().map(ComparableParameters),
                body: body.into(),
            },
            ast::ExprRef::If(ast::ExprIf {
                test,
                body,
                orelse,
                range: _,
                node_index: _,
            }) => Self::If {
                test: test.into(),
                body: body.into(),
                orelse: orelse.into(),
            },
            ast::ExprRef::Dict(ast::ExprDict {
                items,
                range: _,
                node_index: _,
            }) => Self::Dict {
                items: ComparableSlice::new(items),
            },
            ast::ExprRef::Set(ast::ExprSet {
                elts,
                range: _,
                node_index: _,
            }) => Self::Set {
                elts: ComparableSlice::new(elts),
            },
            ast::ExprRef::ListComp(ast::ExprListComp {
                elt,
                generators,
                range: _,
                node_index: _,
            }) => Self::ListComp {
                elt: elt.into(),
                generators: ComparableSlice::new(generators),
            },
            ast::ExprRef::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range: _,
                node_index: _,
            }) => Self::SetComp {
                elt: elt.into(),
                generators: ComparableSlice::new(generators),
            },
            ast::ExprRef::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _,
                node_index: _,
            }) => Self::DictComp {
                key: key.as_ref().map(Into::into),
                value: value.into(),
                generators: ComparableSlice::new(generators),
            },
            ast::ExprRef::Generator(ast::ExprGenerator {
                elt,
                generators,
                range: _,
                node_index: _,
                parenthesized: _,
            }) => Self::Generator {
                elt: elt.into(),
                generators: ComparableSlice::new(generators),
            },
            ast::ExprRef::Await(ast::ExprAwait {
                value,
                range: _,
                node_index: _,
            }) => Self::Await {
                value: value.into(),
            },
            ast::ExprRef::Yield(ast::ExprYield {
                value,
                range: _,
                node_index: _,
            }) => Self::Yield {
                value: value.as_ref().map(Into::into),
            },
            ast::ExprRef::YieldFrom(ast::ExprYieldFrom {
                value,
                range: _,
                node_index: _,
            }) => Self::YieldFrom {
                value: value.into(),
            },
            ast::ExprRef::Compare(ast::ExprCompare {
                ops,
                operands,
                range: _,
                node_index: _,
            }) => Self::Compare {
                ops,
                operands: ComparableSlice::new(operands),
            },
            ast::ExprRef::Call(ast::ExprCall {
                func,
                arguments,
                range_start: _,
                node_index: _,
            }) => Self::Call {
                func: func.into(),
                arguments: arguments.into(),
            },
            ast::ExprRef::FString(ast::ExprFString {
                value,
                range: _,
                node_index: _,
            }) => value.into(),
            ast::ExprRef::TString(ast::ExprTString {
                value,
                range: _,
                node_index: _,
            }) => value.into(),
            ast::ExprRef::StringLiteral(ast::ExprStringLiteral {
                value,
                range: _,
                node_index: _,
            }) => Self::StringLiteral {
                value: value.to_str(),
            },
            ast::ExprRef::BytesLiteral(ast::ExprBytesLiteral {
                value,
                range: _,
                node_index: _,
            }) => Self::BytesLiteral {
                value: Cow::from(value),
            },
            ast::ExprRef::NumberLiteral(ast::ExprNumberLiteral {
                value,
                range: _,
                node_index: _,
            }) => Self::NumberLiteral {
                value: value.into(),
            },
            ast::ExprRef::BooleanLiteral(ast::ExprBooleanLiteral {
                value,
                range: _,
                node_index: _,
            }) => Self::BooleanLiteral { value: *value },
            ast::ExprRef::NoneLiteral(ast::ExprNoneLiteral {
                range: _,
                node_index: _,
            }) => Self::NoneLiteral,
            ast::ExprRef::EllipsisLiteral(ast::ExprEllipsisLiteral {
                range: _,
                node_index: _,
            }) => Self::EllipsisLiteral,
            ast::ExprRef::Attribute(ast::ExprAttribute {
                value,
                attr,
                ctx: _,
                range: _,
                node_index: _,
            }) => Self::Attribute {
                value: value.into(),
                attr: attr.as_str(),
            },
            ast::ExprRef::Subscript(ast::ExprSubscript {
                value,
                slice,
                ctx: _,
                range: _,
                node_index: _,
            }) => Self::Subscript {
                value: value.into(),
                slice: slice.into(),
            },
            ast::ExprRef::Starred(ast::ExprStarred {
                value,
                ctx: _,
                range: _,
                node_index: _,
            }) => Self::Starred {
                value: value.into(),
            },
            ast::ExprRef::Name(ast::ExprName {
                id,
                ctx: _,
                range: _,
                node_index: _,
            }) => Self::Name { id: id.as_str() },
            ast::ExprRef::List(ast::ExprList {
                elts,
                ctx: _,
                range: _,
                node_index: _,
            }) => Self::List {
                elts: ComparableSlice::new(elts),
            },
            ast::ExprRef::Tuple(ast::ExprTuple {
                elts,
                ctx: _,
                range: _,
                node_index: _,
                parenthesized: _,
            }) => Self::Tuple {
                elts: ComparableSlice::new(elts),
            },
            ast::ExprRef::Slice(ast::ExprSlice {
                lower,
                upper,
                step,
                range: _,
                node_index: _,
            }) => Self::Slice {
                lower: lower.as_ref().map(Into::into),
                upper: upper.as_ref().map(Into::into),
                step: step.as_ref().map(Into::into),
            },
            ast::ExprRef::IpyEscapeCommand(ast::ExprIpyEscapeCommand {
                kind,
                value,
                range: _,
                node_index: _,
            }) => Self::IpyEscapeCommand { kind: *kind, value },
        }
    }
}

impl<'a> From<&'a ast::FStringValue> for ComparableExprFields<'a> {
    // The approach below is somewhat complicated, so it may
    // require some justification.
    //
    // Suppose given an f-string of the form
    // `f"{foo!r} one" " and two " f" and three {bar!s}"`
    // This decomposes as:
    // - An `FStringPart::FString`, `f"{foo!r} one"` with elements
    //      - `FStringElement::Expression` encoding `{foo!r}`
    //      - `FStringElement::Literal` encoding " one"
    // - An `FStringPart::Literal` capturing `" and two "`
    // - An `FStringPart::FString`, `f" and three {bar!s}"` with elements
    //      - `FStringElement::Literal` encoding " and three "
    //      - `FStringElement::Expression` encoding `{bar!s}`
    //
    // We would like to extract from this a vector of (comparable) f-string
    // _elements_ which alternate between expression elements and literal
    // elements. In order to do so, we need to concatenate adjacent string
    // literals. String literals may be separated for two reasons: either
    // they appear in adjacent string literal parts, or else a string literal
    // part is adjacent to a string literal _element_ inside of an f-string part.
    fn from(value: &'a ast::FStringValue) -> Self {
        #[derive(Default)]
        struct Collector<'a> {
            elements: Vec<ComparableInterpolatedStringElement<'a>>,
        }

        impl<'a> Collector<'a> {
            // The logic for concatenating adjacent string literals
            // occurs here, implicitly: when we encounter a sequence
            // of string literals, the first gets pushed to the
            // `elements` vector, while subsequent strings
            // are concatenated onto this top string.
            fn push_literal(&mut self, literal: &'a str) {
                if let Some(ComparableInterpolatedStringElement::Literal(existing_literal)) =
                    self.elements.last_mut()
                {
                    existing_literal.to_mut().push_str(literal);
                } else {
                    self.elements
                        .push(ComparableInterpolatedStringElement::Literal(literal.into()));
                }
            }

            fn push_expression(&mut self, expression: &'a ast::InterpolatedElement) {
                self.elements.push(expression.into());
            }
        }

        let mut collector = Collector::default();

        for part in value {
            match part {
                ast::FStringPartRef::Literal(string_literal) => {
                    collector.push_literal(&string_literal.value);
                }
                ast::FStringPartRef::FString(fstring) => {
                    for element in &fstring.elements {
                        match element {
                            ast::InterpolatedStringElement::Literal(literal) => {
                                collector.push_literal(&literal.value);
                            }
                            ast::InterpolatedStringElement::Interpolation(expression) => {
                                collector.push_expression(expression);
                            }
                        }
                    }
                }
            }
        }

        Self::FString {
            elements: collector.elements.into_boxed_slice(),
        }
    }
}

impl<'a> From<&'a ast::TStringValue> for ComparableExprFields<'a> {
    // We model the comparable fields on the actual
    // [CPython implementation] of a `string.templatelib.Template` object.
    //
    // As in CPython, we must be careful to ensure that the length
    // of `strings` is always one more than the length of `interpolations` -
    // that way we can recover the original reading order by interleaving
    // starting with `strings`. This is how we can tell the
    // difference between, e.g. `t"{foo}bar"` and `t"bar{foo}"`.
    //
    // - [CPython implementation](https://github.com/python/cpython/blob/c91ad5da9d92eac4718e4da8d53689c3cc24535e/Python/codegen.c#L4052-L4103)
    fn from(value: &'a ast::TStringValue) -> Self {
        struct Collector<'a> {
            strings: Vec<Cow<'a, str>>,
            interpolations: Vec<InterpolatedElement<'a>>,
        }

        impl Default for Collector<'_> {
            fn default() -> Self {
                Self {
                    strings: vec![Cow::Borrowed("")],
                    interpolations: vec![],
                }
            }
        }

        impl<'a> Collector<'a> {
            // The logic for concatenating adjacent string literals
            // occurs here, implicitly: when we encounter a sequence
            // of string literals, the first gets pushed to the
            // `strings` vector, while subsequent strings
            // are concatenated onto this top string.
            fn push_literal(&mut self, literal: &'a str) {
                if let Some(existing_literal) = self.strings.last_mut() {
                    existing_literal.to_mut().push_str(literal);
                } else {
                    self.strings.push(Cow::Borrowed(literal));
                }
            }

            fn start_new_literal(&mut self) {
                self.strings.push(Cow::Borrowed(""));
            }

            fn push_tstring_interpolation(&mut self, expression: &'a ast::InterpolatedElement) {
                self.interpolations.push(expression.into());
                self.start_new_literal();
            }
        }

        let mut collector = Collector::default();

        for element in value.elements() {
            match element {
                ast::InterpolatedStringElement::Literal(literal) => {
                    collector.push_literal(&literal.value);
                }
                ast::InterpolatedStringElement::Interpolation(interpolation) => {
                    collector.push_tstring_interpolation(interpolation);
                }
            }
        }

        Self::TString {
            strings: collector.strings.into_boxed_slice(),
            interpolations: collector.interpolations.into_boxed_slice(),
        }
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

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparableTypeParam<'a> {
    TypeVar {
        name: &'a str,
        bound: Option<ComparableExpr<'a>>,
        default: Option<ComparableExpr<'a>>,
    },
    ParamSpec {
        name: &'a str,
        default: Option<ComparableExpr<'a>>,
    },
    TypeVarTuple {
        name: &'a str,
        default: Option<ComparableExpr<'a>>,
    },
}

impl<'a> From<&'a ast::TypeParam> for ComparableTypeParam<'a> {
    fn from(type_param: &'a ast::TypeParam) -> Self {
        match type_param {
            ast::TypeParam::TypeVar(ast::TypeParamTypeVar {
                name,
                bound,
                default,
                range: _,
                node_index: _,
            }) => Self::TypeVar {
                name: name.as_str(),
                bound: bound.as_ref().map(Into::into),
                default: default.as_ref().map(Into::into),
            },
            ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple {
                name,
                default,
                range: _,
                node_index: _,
            }) => Self::TypeVarTuple {
                name: name.as_str(),
                default: default.as_ref().map(Into::into),
            },
            ast::TypeParam::ParamSpec(ast::TypeParamParamSpec {
                name,
                default,
                range: _,
                node_index: _,
            }) => Self::ParamSpec {
                name: name.as_str(),
                default: default.as_ref().map(Into::into),
            },
        }
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
        ComparableStmtFields::from(self.0).fmt(f)
    }
}

impl PartialEq for ComparableStmt<'_> {
    fn eq(&self, other: &Self) -> bool {
        ComparableStmtFields::from(self.0) == ComparableStmtFields::from(other.0)
    }
}

impl Eq for ComparableStmt<'_> {}

impl Hash for ComparableStmt<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ComparableStmtFields::from(self.0).hash(state);
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparableStmtFields<'a> {
    FunctionDef {
        is_async: bool,
        decorator_list: ComparableSlice<'a, ast::Decorator, ComparableExpr<'a>>,
        name: &'a str,
        type_params: Option<ComparableTypeParams<'a>>,
        parameters: ComparableParameters<'a>,
        returns: Option<ComparableExpr<'a>>,
        body: ComparableStmtSlice<'a>,
    },
    ClassDef {
        decorator_list: ComparableSlice<'a, ast::Decorator, ComparableExpr<'a>>,
        name: &'a str,
        type_params: Option<ComparableTypeParams<'a>>,
        arguments: ComparableArguments<'a>,
        body: ComparableStmtSlice<'a>,
    },
    Return {
        value: Option<ComparableExpr<'a>>,
    },
    Delete {
        targets: ComparableExprSlice<'a>,
    },
    Assign {
        targets: ComparableExprSlice<'a>,
        value: ComparableExpr<'a>,
    },
    AugAssign {
        target: ComparableExpr<'a>,
        op: ast::Operator,
        value: ComparableExpr<'a>,
    },
    AnnAssign {
        target: ComparableExpr<'a>,
        annotation: ComparableExpr<'a>,
        value: Option<ComparableExpr<'a>>,
        simple: bool,
    },
    For {
        is_async: bool,
        target: ComparableExpr<'a>,
        iter: ComparableExpr<'a>,
        body: ComparableStmtSlice<'a>,
        orelse: ComparableStmtSlice<'a>,
    },
    While {
        test: ComparableExpr<'a>,
        body: ComparableStmtSlice<'a>,
        orelse: ComparableStmtSlice<'a>,
    },
    If {
        test: ComparableExpr<'a>,
        body: ComparableStmtSlice<'a>,
        elif_else_clauses: ComparableSlice<'a, ast::ElifElseClause, ComparableElifElseClause<'a>>,
    },
    With {
        is_async: bool,
        items: ComparableSlice<'a, ast::WithItem, ComparableWithItem<'a>>,
        body: ComparableStmtSlice<'a>,
    },
    Match {
        subject: ComparableExpr<'a>,
        cases: ComparableSlice<'a, ast::MatchCase, ComparableMatchCase<'a>>,
    },
    Raise {
        exc: Option<ComparableExpr<'a>>,
        cause: Option<ComparableExpr<'a>>,
    },
    Try {
        body: ComparableStmtSlice<'a>,
        handlers: ComparableSlice<'a, ast::ExceptHandler, ComparableExceptHandler<'a>>,
        orelse: ComparableStmtSlice<'a>,
        finalbody: ComparableStmtSlice<'a>,
        is_star: bool,
    },
    TypeAlias {
        name: ComparableExpr<'a>,
        type_params: Option<ComparableTypeParams<'a>>,
        value: ComparableExpr<'a>,
    },
    Assert {
        test: ComparableExpr<'a>,
        msg: Option<ComparableExpr<'a>>,
    },
    Import {
        names: ComparableSlice<'a, ast::Alias, ComparableAlias<'a>>,
        is_lazy: bool,
    },
    ImportFrom {
        module: Option<&'a str>,
        names: ComparableSlice<'a, ast::Alias, ComparableAlias<'a>>,
        level: u32,
        is_lazy: bool,
    },
    Global {
        names: ComparableSlice<'a, ast::Identifier, ComparableText<'a>>,
    },
    Nonlocal {
        names: ComparableSlice<'a, ast::Identifier, ComparableText<'a>>,
    },
    IpyEscapeCommand {
        kind: ast::IpyEscapeKind,
        value: &'a str,
    },
    Expr {
        value: ComparableExpr<'a>,
    },
    Pass,
    Break,
    Continue,
}

impl<'a> From<&'a ast::Stmt> for ComparableStmtFields<'a> {
    fn from(node: &'a ast::Stmt) -> Self {
        match node {
            ast::Stmt::FunctionDef(ast::StmtFunctionDef {
                is_async,
                name,
                parameters,
                body,
                decorator_list,
                returns,
                type_params,
                range: _,
                node_index: _,
            }) => Self::FunctionDef {
                is_async: *is_async,
                name: name.as_str(),
                parameters: ComparableParameters(parameters),
                body: ComparableSlice::new(body),
                decorator_list: ComparableSlice::new(decorator_list),
                returns: returns.as_ref().map(Into::into),
                type_params: type_params.as_deref().map(Into::into),
            },
            ast::Stmt::ClassDef(ast::StmtClassDef {
                name,
                arguments,
                body,
                decorator_list,
                type_params,
                range: _,
                node_index: _,
            }) => Self::ClassDef {
                name: name.as_str(),
                arguments: arguments.as_deref().map(Into::into).unwrap_or_default(),
                body: ComparableSlice::new(body),
                decorator_list: ComparableSlice::new(decorator_list),
                type_params: type_params.as_deref().map(Into::into),
            },
            ast::Stmt::Return(ast::StmtReturn {
                value,
                range: _,
                node_index: _,
            }) => Self::Return {
                value: value.as_ref().map(Into::into),
            },
            ast::Stmt::Delete(ast::StmtDelete {
                targets,
                range: _,
                node_index: _,
            }) => Self::Delete {
                targets: ComparableSlice::new(targets),
            },
            ast::Stmt::TypeAlias(ast::StmtTypeAlias {
                range: _,
                node_index: _,
                name,
                type_params,
                value,
            }) => Self::TypeAlias {
                name: name.into(),
                type_params: type_params.as_deref().map(Into::into),
                value: value.into(),
            },
            ast::Stmt::Assign(ast::StmtAssign {
                targets,
                value,
                range: _,
                node_index: _,
            }) => Self::Assign {
                targets: ComparableSlice::new(targets),
                value: value.into(),
            },
            ast::Stmt::AugAssign(ast::StmtAugAssign {
                target,
                op,
                value,
                range: _,
                node_index: _,
            }) => Self::AugAssign {
                target: target.into(),
                op: *op,
                value: value.into(),
            },
            ast::Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                annotation,
                value,
                simple,
                range: _,
                node_index: _,
            }) => Self::AnnAssign {
                target: target.into(),
                annotation: annotation.into(),
                value: value.as_ref().map(Into::into),
                simple: *simple,
            },
            ast::Stmt::For(ast::StmtFor {
                is_async,
                target,
                iter,
                body,
                orelse,
                range: _,
                node_index: _,
            }) => Self::For {
                is_async: *is_async,
                target: target.into(),
                iter: iter.into(),
                body: ComparableSlice::new(body),
                orelse: ComparableSlice::new(orelse),
            },
            ast::Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range: _,
                node_index: _,
            }) => Self::While {
                test: test.into(),
                body: ComparableSlice::new(body),
                orelse: ComparableSlice::new(orelse),
            },
            ast::Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                range: _,
                node_index: _,
            }) => Self::If {
                test: test.into(),
                body: ComparableSlice::new(body),
                elif_else_clauses: ComparableSlice::new(elif_else_clauses),
            },
            ast::Stmt::With(ast::StmtWith {
                is_async,
                items,
                body,
                range: _,
                node_index: _,
            }) => Self::With {
                is_async: *is_async,
                items: ComparableSlice::new(items),
                body: ComparableSlice::new(body),
            },
            ast::Stmt::Match(ast::StmtMatch {
                subject,
                cases,
                range: _,
                node_index: _,
            }) => Self::Match {
                subject: subject.into(),
                cases: ComparableSlice::new(cases),
            },
            ast::Stmt::Raise(ast::StmtRaise {
                exc,
                cause,
                range: _,
                node_index: _,
            }) => Self::Raise {
                exc: exc.as_ref().map(Into::into),
                cause: cause.as_ref().map(Into::into),
            },
            ast::Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                is_star,
                range: _,
                node_index: _,
            }) => Self::Try {
                body: ComparableSlice::new(body),
                handlers: ComparableSlice::new(handlers),
                orelse: ComparableSlice::new(orelse),
                finalbody: ComparableSlice::new(finalbody),
                is_star: *is_star,
            },
            ast::Stmt::Assert(ast::StmtAssert {
                test,
                msg,
                range: _,
                node_index: _,
            }) => Self::Assert {
                test: test.into(),
                msg: msg.as_ref().map(Into::into),
            },
            ast::Stmt::Import(ast::StmtImport {
                names,
                is_lazy,
                range: _,
                node_index: _,
            }) => Self::Import {
                names: ComparableSlice::new(names),
                is_lazy: *is_lazy,
            },
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                is_lazy,
                range: _,
                node_index: _,
            }) => Self::ImportFrom {
                module: module.as_deref(),
                names: ComparableSlice::new(names),
                level: *level,
                is_lazy: *is_lazy,
            },
            ast::Stmt::Global(ast::StmtGlobal {
                names,
                range: _,
                node_index: _,
            }) => Self::Global {
                names: ComparableSlice::new(names),
            },
            ast::Stmt::Nonlocal(ast::StmtNonlocal {
                names,
                range: _,
                node_index: _,
            }) => Self::Nonlocal {
                names: ComparableSlice::new(names),
            },
            ast::Stmt::IpyEscapeCommand(ast::StmtIpyEscapeCommand {
                kind,
                value,
                range: _,
                node_index: _,
            }) => Self::IpyEscapeCommand { kind: *kind, value },
            ast::Stmt::Expr(ast::StmtExpr {
                value,
                range: _,
                node_index: _,
            }) => Self::Expr {
                value: value.into(),
            },
            ast::Stmt::Pass(ast::StmtPass {
                range: _,
                node_index: _,
            }) => Self::Pass,
            ast::Stmt::Break(ast::StmtBreak {
                range: _,
                node_index: _,
            }) => Self::Break,
            ast::Stmt::Continue(ast::StmtContinue {
                range: _,
                node_index: _,
            }) => Self::Continue,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableMod<'a>(ComparableModFields<'a>);

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparableModFields<'a> {
    Module { body: ComparableStmtSlice<'a> },
    Expression { body: ComparableExpr<'a> },
}

impl<'a> From<&'a ast::Mod> for ComparableMod<'a> {
    fn from(module: &'a ast::Mod) -> Self {
        Self(match module {
            ast::Mod::Module(ast::ModModule {
                body,
                range: _,
                node_index: _,
            }) => ComparableModFields::Module {
                body: ComparableSlice::new(body),
            },
            ast::Mod::Expression(ast::ModExpression {
                body,
                range: _,
                node_index: _,
            }) => ComparableModFields::Expression { body: body.into() },
        })
    }
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
