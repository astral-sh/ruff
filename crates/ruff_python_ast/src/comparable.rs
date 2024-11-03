//! An equivalent object hierarchy to the `RustPython` AST hierarchy, but with the
//! ability to compare expressions for equality (via [`Eq`] and [`Hash`]).
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
use std::hash::Hash;

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum ComparableBoolOp {
    And,
    Or,
}

impl From<ast::BoolOp> for ComparableBoolOp {
    fn from(op: ast::BoolOp) -> Self {
        match op {
            ast::BoolOp::And => Self::And,
            ast::BoolOp::Or => Self::Or,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum ComparableOperator {
    Add,
    Sub,
    Mult,
    MatMult,
    Div,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
    FloorDiv,
}

impl From<ast::Operator> for ComparableOperator {
    fn from(op: ast::Operator) -> Self {
        match op {
            ast::Operator::Add => Self::Add,
            ast::Operator::Sub => Self::Sub,
            ast::Operator::Mult => Self::Mult,
            ast::Operator::MatMult => Self::MatMult,
            ast::Operator::Div => Self::Div,
            ast::Operator::Mod => Self::Mod,
            ast::Operator::Pow => Self::Pow,
            ast::Operator::LShift => Self::LShift,
            ast::Operator::RShift => Self::RShift,
            ast::Operator::BitOr => Self::BitOr,
            ast::Operator::BitXor => Self::BitXor,
            ast::Operator::BitAnd => Self::BitAnd,
            ast::Operator::FloorDiv => Self::FloorDiv,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum ComparableUnaryOp {
    Invert,
    Not,
    UAdd,
    USub,
}

impl From<ast::UnaryOp> for ComparableUnaryOp {
    fn from(op: ast::UnaryOp) -> Self {
        match op {
            ast::UnaryOp::Invert => Self::Invert,
            ast::UnaryOp::Not => Self::Not,
            ast::UnaryOp::UAdd => Self::UAdd,
            ast::UnaryOp::USub => Self::USub,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum ComparableCmpOp {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

impl From<ast::CmpOp> for ComparableCmpOp {
    fn from(op: ast::CmpOp) -> Self {
        match op {
            ast::CmpOp::Eq => Self::Eq,
            ast::CmpOp::NotEq => Self::NotEq,
            ast::CmpOp::Lt => Self::Lt,
            ast::CmpOp::LtE => Self::LtE,
            ast::CmpOp::Gt => Self::Gt,
            ast::CmpOp::GtE => Self::GtE,
            ast::CmpOp::Is => Self::Is,
            ast::CmpOp::IsNot => Self::IsNot,
            ast::CmpOp::In => Self::In,
            ast::CmpOp::NotIn => Self::NotIn,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableAlias<'a> {
    name: &'a str,
    asname: Option<&'a str>,
}

impl<'a> From<&'a ast::Alias> for ComparableAlias<'a> {
    fn from(alias: &'a ast::Alias) -> Self {
        Self {
            name: alias.name.as_str(),
            asname: alias.asname.as_deref(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableWithItem<'a> {
    context_expr: ComparableExpr<'a>,
    optional_vars: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::WithItem> for ComparableWithItem<'a> {
    fn from(with_item: &'a ast::WithItem) -> Self {
        Self {
            context_expr: (&with_item.context_expr).into(),
            optional_vars: with_item.optional_vars.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparablePatternArguments<'a> {
    patterns: Vec<ComparablePattern<'a>>,
    keywords: Vec<ComparablePatternKeyword<'a>>,
}

impl<'a> From<&'a ast::PatternArguments> for ComparablePatternArguments<'a> {
    fn from(parameters: &'a ast::PatternArguments) -> Self {
        Self {
            patterns: parameters.patterns.iter().map(Into::into).collect(),
            keywords: parameters.keywords.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparablePatternKeyword<'a> {
    attr: &'a str,
    pattern: ComparablePattern<'a>,
}

impl<'a> From<&'a ast::PatternKeyword> for ComparablePatternKeyword<'a> {
    fn from(keyword: &'a ast::PatternKeyword) -> Self {
        Self {
            attr: keyword.attr.as_str(),
            pattern: (&keyword.pattern).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchValue<'a> {
    value: ComparableExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchSingleton {
    value: ComparableSingleton,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchSequence<'a> {
    patterns: Vec<ComparablePattern<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchMapping<'a> {
    keys: Vec<ComparableExpr<'a>>,
    patterns: Vec<ComparablePattern<'a>>,
    rest: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchClass<'a> {
    cls: ComparableExpr<'a>,
    arguments: ComparablePatternArguments<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchStar<'a> {
    name: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchAs<'a> {
    pattern: Option<Box<ComparablePattern<'a>>>,
    name: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchOr<'a> {
    patterns: Vec<ComparablePattern<'a>>,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparablePattern<'a> {
    MatchValue(PatternMatchValue<'a>),
    MatchSingleton(PatternMatchSingleton),
    MatchSequence(PatternMatchSequence<'a>),
    MatchMapping(PatternMatchMapping<'a>),
    MatchClass(PatternMatchClass<'a>),
    MatchStar(PatternMatchStar<'a>),
    MatchAs(PatternMatchAs<'a>),
    MatchOr(PatternMatchOr<'a>),
}

impl<'a> From<&'a ast::Pattern> for ComparablePattern<'a> {
    fn from(pattern: &'a ast::Pattern) -> Self {
        match pattern {
            ast::Pattern::MatchValue(ast::PatternMatchValue { value, .. }) => {
                Self::MatchValue(PatternMatchValue {
                    value: value.into(),
                })
            }
            ast::Pattern::MatchSingleton(ast::PatternMatchSingleton { value, .. }) => {
                Self::MatchSingleton(PatternMatchSingleton {
                    value: value.into(),
                })
            }
            ast::Pattern::MatchSequence(ast::PatternMatchSequence { patterns, .. }) => {
                Self::MatchSequence(PatternMatchSequence {
                    patterns: patterns.iter().map(Into::into).collect(),
                })
            }
            ast::Pattern::MatchMapping(ast::PatternMatchMapping {
                keys,
                patterns,
                rest,
                ..
            }) => Self::MatchMapping(PatternMatchMapping {
                keys: keys.iter().map(Into::into).collect(),
                patterns: patterns.iter().map(Into::into).collect(),
                rest: rest.as_deref(),
            }),
            ast::Pattern::MatchClass(ast::PatternMatchClass { cls, arguments, .. }) => {
                Self::MatchClass(PatternMatchClass {
                    cls: cls.into(),
                    arguments: arguments.into(),
                })
            }
            ast::Pattern::MatchStar(ast::PatternMatchStar { name, .. }) => {
                Self::MatchStar(PatternMatchStar {
                    name: name.as_deref(),
                })
            }
            ast::Pattern::MatchAs(ast::PatternMatchAs { pattern, name, .. }) => {
                Self::MatchAs(PatternMatchAs {
                    pattern: pattern.as_ref().map(Into::into),
                    name: name.as_deref(),
                })
            }
            ast::Pattern::MatchOr(ast::PatternMatchOr { patterns, .. }) => {
                Self::MatchOr(PatternMatchOr {
                    patterns: patterns.iter().map(Into::into).collect(),
                })
            }
        }
    }
}

impl<'a> From<&'a Box<ast::Pattern>> for Box<ComparablePattern<'a>> {
    fn from(pattern: &'a Box<ast::Pattern>) -> Self {
        Box::new((pattern.as_ref()).into())
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableMatchCase<'a> {
    pattern: ComparablePattern<'a>,
    guard: Option<ComparableExpr<'a>>,
    body: Vec<ComparableStmt<'a>>,
}

impl<'a> From<&'a ast::MatchCase> for ComparableMatchCase<'a> {
    fn from(match_case: &'a ast::MatchCase) -> Self {
        Self {
            pattern: (&match_case.pattern).into(),
            guard: match_case.guard.as_ref().map(Into::into),
            body: match_case.body.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableDecorator<'a> {
    expression: ComparableExpr<'a>,
}

impl<'a> From<&'a ast::Decorator> for ComparableDecorator<'a> {
    fn from(decorator: &'a ast::Decorator) -> Self {
        Self {
            expression: (&decorator.expression).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableSingleton {
    None,
    True,
    False,
}

impl From<&ast::Singleton> for ComparableSingleton {
    fn from(singleton: &ast::Singleton) -> Self {
        match singleton {
            ast::Singleton::None => Self::None,
            ast::Singleton::True => Self::True,
            ast::Singleton::False => Self::False,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableNumber<'a> {
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

#[derive(Debug, Default, PartialEq, Eq, Hash)]
pub struct ComparableArguments<'a> {
    args: Vec<ComparableExpr<'a>>,
    keywords: Vec<ComparableKeyword<'a>>,
}

impl<'a> From<&'a ast::Arguments> for ComparableArguments<'a> {
    fn from(arguments: &'a ast::Arguments) -> Self {
        Self {
            args: arguments.args.iter().map(Into::into).collect(),
            keywords: arguments.keywords.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<&'a Box<ast::Arguments>> for ComparableArguments<'a> {
    fn from(arguments: &'a Box<ast::Arguments>) -> Self {
        (arguments.as_ref()).into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableParameters<'a> {
    posonlyargs: Vec<ComparableParameterWithDefault<'a>>,
    args: Vec<ComparableParameterWithDefault<'a>>,
    vararg: Option<ComparableParameter<'a>>,
    kwonlyargs: Vec<ComparableParameterWithDefault<'a>>,
    kwarg: Option<ComparableParameter<'a>>,
}

impl<'a> From<&'a ast::Parameters> for ComparableParameters<'a> {
    fn from(parameters: &'a ast::Parameters) -> Self {
        Self {
            posonlyargs: parameters.posonlyargs.iter().map(Into::into).collect(),
            args: parameters.args.iter().map(Into::into).collect(),
            vararg: parameters.vararg.as_ref().map(Into::into),
            kwonlyargs: parameters.kwonlyargs.iter().map(Into::into).collect(),
            kwarg: parameters.kwarg.as_ref().map(Into::into),
        }
    }
}

impl<'a> From<&'a Box<ast::Parameters>> for ComparableParameters<'a> {
    fn from(parameters: &'a Box<ast::Parameters>) -> Self {
        (parameters.as_ref()).into()
    }
}

impl<'a> From<&'a Box<ast::Parameter>> for ComparableParameter<'a> {
    fn from(arg: &'a Box<ast::Parameter>) -> Self {
        (arg.as_ref()).into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableParameter<'a> {
    arg: &'a str,
    annotation: Option<Box<ComparableExpr<'a>>>,
}

impl<'a> From<&'a ast::Parameter> for ComparableParameter<'a> {
    fn from(arg: &'a ast::Parameter) -> Self {
        Self {
            arg: arg.name.as_str(),
            annotation: arg.annotation.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableParameterWithDefault<'a> {
    def: ComparableParameter<'a>,
    default: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::ParameterWithDefault> for ComparableParameterWithDefault<'a> {
    fn from(arg: &'a ast::ParameterWithDefault) -> Self {
        Self {
            def: (&arg.parameter).into(),
            default: arg.default.as_ref().map(Into::into),
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
        Self {
            arg: keyword.arg.as_ref().map(ast::Identifier::as_str),
            value: (&keyword.value).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableComprehension<'a> {
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    ifs: Vec<ComparableExpr<'a>>,
    is_async: bool,
}

impl<'a> From<&'a ast::Comprehension> for ComparableComprehension<'a> {
    fn from(comprehension: &'a ast::Comprehension) -> Self {
        Self {
            target: (&comprehension.target).into(),
            iter: (&comprehension.iter).into(),
            ifs: comprehension.ifs.iter().map(Into::into).collect(),
            is_async: comprehension.is_async,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExceptHandlerExceptHandler<'a> {
    type_: Option<Box<ComparableExpr<'a>>>,
    name: Option<&'a str>,
    body: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableExceptHandler<'a> {
    ExceptHandler(ExceptHandlerExceptHandler<'a>),
}

impl<'a> From<&'a ast::ExceptHandler> for ComparableExceptHandler<'a> {
    fn from(except_handler: &'a ast::ExceptHandler) -> Self {
        let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_,
            name,
            body,
            ..
        }) = except_handler;
        Self::ExceptHandler(ExceptHandlerExceptHandler {
            type_: type_.as_ref().map(Into::into),
            name: name.as_deref(),
            body: body.iter().map(Into::into).collect(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableFStringElement<'a> {
    Literal(Cow<'a, str>),
    FStringExpressionElement(FStringExpressionElement<'a>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct FStringExpressionElement<'a> {
    expression: ComparableExpr<'a>,
    debug_text: Option<&'a ast::DebugText>,
    conversion: ast::ConversionFlag,
    format_spec: Option<Vec<ComparableFStringElement<'a>>>,
}

impl<'a> From<&'a ast::FStringElement> for ComparableFStringElement<'a> {
    fn from(fstring_element: &'a ast::FStringElement) -> Self {
        match fstring_element {
            ast::FStringElement::Literal(ast::FStringLiteralElement { value, .. }) => {
                Self::Literal(value.as_ref().into())
            }
            ast::FStringElement::Expression(formatted_value) => formatted_value.into(),
        }
    }
}

impl<'a> From<&'a ast::FStringExpressionElement> for ComparableFStringElement<'a> {
    fn from(fstring_expression_element: &'a ast::FStringExpressionElement) -> Self {
        let ast::FStringExpressionElement {
            expression,
            debug_text,
            conversion,
            format_spec,
            range: _,
        } = fstring_expression_element;

        Self::FStringExpressionElement(FStringExpressionElement {
            expression: (expression).into(),
            debug_text: debug_text.as_ref(),
            conversion: *conversion,
            format_spec: format_spec
                .as_ref()
                .map(|spec| spec.elements.iter().map(Into::into).collect()),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableElifElseClause<'a> {
    test: Option<ComparableExpr<'a>>,
    body: Vec<ComparableStmt<'a>>,
}

impl<'a> From<&'a ast::ElifElseClause> for ComparableElifElseClause<'a> {
    fn from(elif_else_clause: &'a ast::ElifElseClause) -> Self {
        let ast::ElifElseClause {
            range: _,
            test,
            body,
        } = elif_else_clause;
        Self {
            test: test.as_ref().map(Into::into),
            body: body.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableLiteral<'a> {
    None,
    Ellipsis,
    Bool(&'a bool),
    Str(Vec<ComparableStringLiteral<'a>>),
    Bytes(Vec<ComparableBytesLiteral<'a>>),
    Number(ComparableNumber<'a>),
}

impl<'a> From<ast::LiteralExpressionRef<'a>> for ComparableLiteral<'a> {
    fn from(literal: ast::LiteralExpressionRef<'a>) -> Self {
        match literal {
            ast::LiteralExpressionRef::NoneLiteral(_) => Self::None,
            ast::LiteralExpressionRef::EllipsisLiteral(_) => Self::Ellipsis,
            ast::LiteralExpressionRef::BooleanLiteral(ast::ExprBooleanLiteral {
                value, ..
            }) => Self::Bool(value),
            ast::LiteralExpressionRef::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                Self::Str(value.iter().map(Into::into).collect())
            }
            ast::LiteralExpressionRef::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => {
                Self::Bytes(value.iter().map(Into::into).collect())
            }
            ast::LiteralExpressionRef::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => {
                Self::Number(value.into())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableFString<'a> {
    elements: Box<[ComparableFStringElement<'a>]>,
}

impl<'a> From<&'a ast::FStringValue> for ComparableFString<'a> {
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
            elements: Vec<ComparableFStringElement<'a>>,
        }

        impl<'a> Collector<'a> {
            // The logic for concatenating adjacent string literals
            // occurs here, implicitly: when we encounter a sequence
            // of string literals, the first gets pushed to the
            // `elements` vector, while subsequent strings
            // are concatenated onto this top string.
            fn push_literal(&mut self, literal: &'a str) {
                if let Some(ComparableFStringElement::Literal(existing_literal)) =
                    self.elements.last_mut()
                {
                    existing_literal.to_mut().push_str(literal);
                } else {
                    self.elements
                        .push(ComparableFStringElement::Literal(literal.into()));
                }
            }

            fn push_expression(&mut self, expression: &'a ast::FStringExpressionElement) {
                self.elements.push(expression.into());
            }
        }

        let mut collector = Collector::default();

        for part in value {
            match part {
                ast::FStringPart::Literal(string_literal) => {
                    collector.push_literal(&string_literal.value);
                }
                ast::FStringPart::FString(fstring) => {
                    for element in &fstring.elements {
                        match element {
                            ast::FStringElement::Literal(literal) => {
                                collector.push_literal(&literal.value);
                            }
                            ast::FStringElement::Expression(expression) => {
                                collector.push_expression(expression);
                            }
                        }
                    }
                }
            }
        }

        Self {
            elements: collector.elements.into_boxed_slice(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableStringLiteral<'a> {
    value: &'a str,
}

impl<'a> From<&'a ast::StringLiteral> for ComparableStringLiteral<'a> {
    fn from(string_literal: &'a ast::StringLiteral) -> Self {
        Self {
            value: &string_literal.value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableBytesLiteral<'a> {
    value: Cow<'a, [u8]>,
}

impl<'a> From<&'a ast::BytesLiteral> for ComparableBytesLiteral<'a> {
    fn from(bytes_literal: &'a ast::BytesLiteral) -> Self {
        Self {
            value: Cow::Borrowed(&bytes_literal.value),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprBoolOp<'a> {
    op: ComparableBoolOp,
    values: Vec<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprNamed<'a> {
    target: Box<ComparableExpr<'a>>,
    value: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprBinOp<'a> {
    left: Box<ComparableExpr<'a>>,
    op: ComparableOperator,
    right: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprUnaryOp<'a> {
    op: ComparableUnaryOp,
    operand: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprLambda<'a> {
    parameters: Option<ComparableParameters<'a>>,
    body: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprIf<'a> {
    test: Box<ComparableExpr<'a>>,
    body: Box<ComparableExpr<'a>>,
    orelse: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableDictItem<'a> {
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprDict<'a> {
    items: Vec<ComparableDictItem<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSet<'a> {
    elts: Vec<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprListComp<'a> {
    elt: Box<ComparableExpr<'a>>,
    generators: Vec<ComparableComprehension<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSetComp<'a> {
    elt: Box<ComparableExpr<'a>>,
    generators: Vec<ComparableComprehension<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprDictComp<'a> {
    key: Box<ComparableExpr<'a>>,
    value: Box<ComparableExpr<'a>>,
    generators: Vec<ComparableComprehension<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprGenerator<'a> {
    elt: Box<ComparableExpr<'a>>,
    generators: Vec<ComparableComprehension<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprAwait<'a> {
    value: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprYield<'a> {
    value: Option<Box<ComparableExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprYieldFrom<'a> {
    value: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprCompare<'a> {
    left: Box<ComparableExpr<'a>>,
    ops: Vec<ComparableCmpOp>,
    comparators: Vec<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprCall<'a> {
    func: Box<ComparableExpr<'a>>,
    arguments: ComparableArguments<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprFStringExpressionElement<'a> {
    value: Box<ComparableExpr<'a>>,
    debug_text: Option<&'a ast::DebugText>,
    conversion: ast::ConversionFlag,
    format_spec: Vec<ComparableFStringElement<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprFString<'a> {
    value: ComparableFString<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprStringLiteral<'a> {
    value: ComparableStringLiteral<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprBytesLiteral<'a> {
    value: ComparableBytesLiteral<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprNumberLiteral<'a> {
    value: ComparableNumber<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprBoolLiteral {
    value: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprAttribute<'a> {
    value: Box<ComparableExpr<'a>>,
    attr: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSubscript<'a> {
    value: Box<ComparableExpr<'a>>,
    slice: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprStarred<'a> {
    value: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprName<'a> {
    id: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprList<'a> {
    elts: Vec<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprTuple<'a> {
    elts: Vec<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSlice<'a> {
    lower: Option<Box<ComparableExpr<'a>>>,
    upper: Option<Box<ComparableExpr<'a>>>,
    step: Option<Box<ComparableExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprIpyEscapeCommand<'a> {
    kind: ast::IpyEscapeKind,
    value: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableExpr<'a> {
    BoolOp(ExprBoolOp<'a>),
    NamedExpr(ExprNamed<'a>),
    BinOp(ExprBinOp<'a>),
    UnaryOp(ExprUnaryOp<'a>),
    Lambda(ExprLambda<'a>),
    IfExp(ExprIf<'a>),
    Dict(ExprDict<'a>),
    Set(ExprSet<'a>),
    ListComp(ExprListComp<'a>),
    SetComp(ExprSetComp<'a>),
    DictComp(ExprDictComp<'a>),
    GeneratorExp(ExprGenerator<'a>),
    Await(ExprAwait<'a>),
    Yield(ExprYield<'a>),
    YieldFrom(ExprYieldFrom<'a>),
    Compare(ExprCompare<'a>),
    Call(ExprCall<'a>),
    FStringExpressionElement(ExprFStringExpressionElement<'a>),
    FString(ExprFString<'a>),
    StringLiteral(ExprStringLiteral<'a>),
    BytesLiteral(ExprBytesLiteral<'a>),
    NumberLiteral(ExprNumberLiteral<'a>),
    BoolLiteral(ExprBoolLiteral),
    NoneLiteral,
    EllipsisLiteral,
    Attribute(ExprAttribute<'a>),
    Subscript(ExprSubscript<'a>),
    Starred(ExprStarred<'a>),
    Name(ExprName<'a>),
    List(ExprList<'a>),
    Tuple(ExprTuple<'a>),
    Slice(ExprSlice<'a>),
    IpyEscapeCommand(ExprIpyEscapeCommand<'a>),
}

impl<'a> From<&'a Box<ast::Expr>> for Box<ComparableExpr<'a>> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        Box::new((expr.as_ref()).into())
    }
}

impl<'a> From<&'a Box<ast::Expr>> for ComparableExpr<'a> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        (expr.as_ref()).into()
    }
}

impl<'a> From<&'a ast::Expr> for ComparableExpr<'a> {
    fn from(expr: &'a ast::Expr) -> Self {
        match expr {
            ast::Expr::BoolOp(ast::ExprBoolOp {
                op,
                values,
                range: _,
            }) => Self::BoolOp(ExprBoolOp {
                op: (*op).into(),
                values: values.iter().map(Into::into).collect(),
            }),
            ast::Expr::Named(ast::ExprNamed {
                target,
                value,
                range: _,
            }) => Self::NamedExpr(ExprNamed {
                target: target.into(),
                value: value.into(),
            }),
            ast::Expr::BinOp(ast::ExprBinOp {
                left,
                op,
                right,
                range: _,
            }) => Self::BinOp(ExprBinOp {
                left: left.into(),
                op: (*op).into(),
                right: right.into(),
            }),
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op,
                operand,
                range: _,
            }) => Self::UnaryOp(ExprUnaryOp {
                op: (*op).into(),
                operand: operand.into(),
            }),
            ast::Expr::Lambda(ast::ExprLambda {
                parameters,
                body,
                range: _,
            }) => Self::Lambda(ExprLambda {
                parameters: parameters.as_ref().map(Into::into),
                body: body.into(),
            }),
            ast::Expr::If(ast::ExprIf {
                test,
                body,
                orelse,
                range: _,
            }) => Self::IfExp(ExprIf {
                test: test.into(),
                body: body.into(),
                orelse: orelse.into(),
            }),
            ast::Expr::Dict(ast::ExprDict { items, range: _ }) => Self::Dict(ExprDict {
                items: items.iter().map(ComparableDictItem::from).collect(),
            }),
            ast::Expr::Set(ast::ExprSet { elts, range: _ }) => Self::Set(ExprSet {
                elts: elts.iter().map(Into::into).collect(),
            }),
            ast::Expr::ListComp(ast::ExprListComp {
                elt,
                generators,
                range: _,
            }) => Self::ListComp(ExprListComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::Expr::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range: _,
            }) => Self::SetComp(ExprSetComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _,
            }) => Self::DictComp(ExprDictComp {
                key: key.into(),
                value: value.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::Expr::Generator(ast::ExprGenerator {
                elt,
                generators,
                range: _,
                parenthesized: _,
            }) => Self::GeneratorExp(ExprGenerator {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::Expr::Await(ast::ExprAwait { value, range: _ }) => Self::Await(ExprAwait {
                value: value.into(),
            }),
            ast::Expr::Yield(ast::ExprYield { value, range: _ }) => Self::Yield(ExprYield {
                value: value.as_ref().map(Into::into),
            }),
            ast::Expr::YieldFrom(ast::ExprYieldFrom { value, range: _ }) => {
                Self::YieldFrom(ExprYieldFrom {
                    value: value.into(),
                })
            }
            ast::Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                range: _,
            }) => Self::Compare(ExprCompare {
                left: left.into(),
                ops: ops.iter().copied().map(Into::into).collect(),
                comparators: comparators.iter().map(Into::into).collect(),
            }),
            ast::Expr::Call(ast::ExprCall {
                func,
                arguments,
                range: _,
            }) => Self::Call(ExprCall {
                func: func.into(),
                arguments: arguments.into(),
            }),
            ast::Expr::FString(ast::ExprFString { value, range: _ }) => {
                Self::FString(ExprFString {
                    value: value.into(),
                })
            }
            ast::Expr::StringLiteral(ast::ExprStringLiteral { value, range: _ }) => {
                Self::StringLiteral(ExprStringLiteral {
                    value: ComparableStringLiteral {
                        value: value.to_str(),
                    },
                })
            }
            ast::Expr::BytesLiteral(ast::ExprBytesLiteral { value, range: _ }) => {
                Self::BytesLiteral(ExprBytesLiteral {
                    value: ComparableBytesLiteral {
                        value: Cow::from(value),
                    },
                })
            }
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral { value, range: _ }) => {
                Self::NumberLiteral(ExprNumberLiteral {
                    value: value.into(),
                })
            }
            ast::Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, range: _ }) => {
                Self::BoolLiteral(ExprBoolLiteral { value: *value })
            }
            ast::Expr::NoneLiteral(_) => Self::NoneLiteral,
            ast::Expr::EllipsisLiteral(_) => Self::EllipsisLiteral,
            ast::Expr::Attribute(ast::ExprAttribute {
                value,
                attr,
                ctx: _,
                range: _,
            }) => Self::Attribute(ExprAttribute {
                value: value.into(),
                attr: attr.as_str(),
            }),
            ast::Expr::Subscript(ast::ExprSubscript {
                value,
                slice,
                ctx: _,
                range: _,
            }) => Self::Subscript(ExprSubscript {
                value: value.into(),
                slice: slice.into(),
            }),
            ast::Expr::Starred(ast::ExprStarred {
                value,
                ctx: _,
                range: _,
            }) => Self::Starred(ExprStarred {
                value: value.into(),
            }),
            ast::Expr::Name(name) => name.into(),
            ast::Expr::List(ast::ExprList {
                elts,
                ctx: _,
                range: _,
            }) => Self::List(ExprList {
                elts: elts.iter().map(Into::into).collect(),
            }),
            ast::Expr::Tuple(ast::ExprTuple {
                elts,
                ctx: _,
                range: _,
                parenthesized: _,
            }) => Self::Tuple(ExprTuple {
                elts: elts.iter().map(Into::into).collect(),
            }),
            ast::Expr::Slice(ast::ExprSlice {
                lower,
                upper,
                step,
                range: _,
            }) => Self::Slice(ExprSlice {
                lower: lower.as_ref().map(Into::into),
                upper: upper.as_ref().map(Into::into),
                step: step.as_ref().map(Into::into),
            }),
            ast::Expr::IpyEscapeCommand(ast::ExprIpyEscapeCommand {
                kind,
                value,
                range: _,
            }) => Self::IpyEscapeCommand(ExprIpyEscapeCommand { kind: *kind, value }),
        }
    }
}

impl<'a> From<&'a ast::ExprName> for ComparableExpr<'a> {
    fn from(expr: &'a ast::ExprName) -> Self {
        Self::Name(ExprName {
            id: expr.id.as_str(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtFunctionDef<'a> {
    is_async: bool,
    decorator_list: Vec<ComparableDecorator<'a>>,
    name: &'a str,
    type_params: Option<ComparableTypeParams<'a>>,
    parameters: ComparableParameters<'a>,
    returns: Option<ComparableExpr<'a>>,
    body: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtClassDef<'a> {
    decorator_list: Vec<ComparableDecorator<'a>>,
    name: &'a str,
    type_params: Option<ComparableTypeParams<'a>>,
    arguments: ComparableArguments<'a>,
    body: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtReturn<'a> {
    value: Option<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtDelete<'a> {
    targets: Vec<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtTypeAlias<'a> {
    pub name: Box<ComparableExpr<'a>>,
    pub type_params: Option<ComparableTypeParams<'a>>,
    pub value: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableTypeParams<'a> {
    pub type_params: Vec<ComparableTypeParam<'a>>,
}

impl<'a> From<&'a ast::TypeParams> for ComparableTypeParams<'a> {
    fn from(type_params: &'a ast::TypeParams) -> Self {
        Self {
            type_params: type_params.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<&'a Box<ast::TypeParams>> for ComparableTypeParams<'a> {
    fn from(type_params: &'a Box<ast::TypeParams>) -> Self {
        type_params.as_ref().into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableTypeParam<'a> {
    TypeVar(TypeParamTypeVar<'a>),
    ParamSpec(TypeParamParamSpec<'a>),
    TypeVarTuple(TypeParamTypeVarTuple<'a>),
}

impl<'a> From<&'a ast::TypeParam> for ComparableTypeParam<'a> {
    fn from(type_param: &'a ast::TypeParam) -> Self {
        match type_param {
            ast::TypeParam::TypeVar(ast::TypeParamTypeVar {
                name,
                bound,
                default,
                range: _,
            }) => Self::TypeVar(TypeParamTypeVar {
                name: name.as_str(),
                bound: bound.as_ref().map(Into::into),
                default: default.as_ref().map(Into::into),
            }),
            ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple {
                name,
                default,
                range: _,
            }) => Self::TypeVarTuple(TypeParamTypeVarTuple {
                name: name.as_str(),
                default: default.as_ref().map(Into::into),
            }),
            ast::TypeParam::ParamSpec(ast::TypeParamParamSpec {
                name,
                default,
                range: _,
            }) => Self::ParamSpec(TypeParamParamSpec {
                name: name.as_str(),
                default: default.as_ref().map(Into::into),
            }),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TypeParamTypeVar<'a> {
    pub name: &'a str,
    pub bound: Option<Box<ComparableExpr<'a>>>,
    pub default: Option<Box<ComparableExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TypeParamParamSpec<'a> {
    pub name: &'a str,
    pub default: Option<Box<ComparableExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TypeParamTypeVarTuple<'a> {
    pub name: &'a str,
    pub default: Option<Box<ComparableExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAssign<'a> {
    targets: Vec<ComparableExpr<'a>>,
    value: ComparableExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAugAssign<'a> {
    target: ComparableExpr<'a>,
    op: ComparableOperator,
    value: ComparableExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAnnAssign<'a> {
    target: ComparableExpr<'a>,
    annotation: ComparableExpr<'a>,
    value: Option<ComparableExpr<'a>>,
    simple: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtFor<'a> {
    is_async: bool,
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    body: Vec<ComparableStmt<'a>>,
    orelse: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtWhile<'a> {
    test: ComparableExpr<'a>,
    body: Vec<ComparableStmt<'a>>,
    orelse: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtIf<'a> {
    test: ComparableExpr<'a>,
    body: Vec<ComparableStmt<'a>>,
    elif_else_clauses: Vec<ComparableElifElseClause<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtWith<'a> {
    is_async: bool,
    items: Vec<ComparableWithItem<'a>>,
    body: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtMatch<'a> {
    subject: ComparableExpr<'a>,
    cases: Vec<ComparableMatchCase<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtRaise<'a> {
    exc: Option<ComparableExpr<'a>>,
    cause: Option<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtTry<'a> {
    body: Vec<ComparableStmt<'a>>,
    handlers: Vec<ComparableExceptHandler<'a>>,
    orelse: Vec<ComparableStmt<'a>>,
    finalbody: Vec<ComparableStmt<'a>>,
    is_star: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAssert<'a> {
    test: ComparableExpr<'a>,
    msg: Option<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtImport<'a> {
    names: Vec<ComparableAlias<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtImportFrom<'a> {
    module: Option<&'a str>,
    names: Vec<ComparableAlias<'a>>,
    level: u32,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtGlobal<'a> {
    names: Vec<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtNonlocal<'a> {
    names: Vec<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtExpr<'a> {
    value: ComparableExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtIpyEscapeCommand<'a> {
    kind: ast::IpyEscapeKind,
    value: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableStmt<'a> {
    FunctionDef(StmtFunctionDef<'a>),
    ClassDef(StmtClassDef<'a>),
    Return(StmtReturn<'a>),
    Delete(StmtDelete<'a>),
    Assign(StmtAssign<'a>),
    AugAssign(StmtAugAssign<'a>),
    AnnAssign(StmtAnnAssign<'a>),
    For(StmtFor<'a>),
    While(StmtWhile<'a>),
    If(StmtIf<'a>),
    With(StmtWith<'a>),
    Match(StmtMatch<'a>),
    Raise(StmtRaise<'a>),
    Try(StmtTry<'a>),
    TypeAlias(StmtTypeAlias<'a>),
    Assert(StmtAssert<'a>),
    Import(StmtImport<'a>),
    ImportFrom(StmtImportFrom<'a>),
    Global(StmtGlobal<'a>),
    Nonlocal(StmtNonlocal<'a>),
    IpyEscapeCommand(StmtIpyEscapeCommand<'a>),
    Expr(StmtExpr<'a>),
    Pass,
    Break,
    Continue,
}

impl<'a> From<&'a ast::Stmt> for ComparableStmt<'a> {
    fn from(stmt: &'a ast::Stmt) -> Self {
        match stmt {
            ast::Stmt::FunctionDef(ast::StmtFunctionDef {
                is_async,
                name,
                parameters,
                body,
                decorator_list,
                returns,
                type_params,
                range: _,
            }) => Self::FunctionDef(StmtFunctionDef {
                is_async: *is_async,
                name: name.as_str(),
                parameters: parameters.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.as_ref().map(Into::into),
                type_params: type_params.as_ref().map(Into::into),
            }),
            ast::Stmt::ClassDef(ast::StmtClassDef {
                name,
                arguments,
                body,
                decorator_list,
                type_params,
                range: _,
            }) => Self::ClassDef(StmtClassDef {
                name: name.as_str(),
                arguments: arguments.as_ref().map(Into::into).unwrap_or_default(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                type_params: type_params.as_ref().map(Into::into),
            }),
            ast::Stmt::Return(ast::StmtReturn { value, range: _ }) => Self::Return(StmtReturn {
                value: value.as_ref().map(Into::into),
            }),
            ast::Stmt::Delete(ast::StmtDelete { targets, range: _ }) => Self::Delete(StmtDelete {
                targets: targets.iter().map(Into::into).collect(),
            }),
            ast::Stmt::TypeAlias(ast::StmtTypeAlias {
                range: _,
                name,
                type_params,
                value,
            }) => Self::TypeAlias(StmtTypeAlias {
                name: name.into(),
                type_params: type_params.as_ref().map(Into::into),
                value: value.into(),
            }),
            ast::Stmt::Assign(ast::StmtAssign {
                targets,
                value,
                range: _,
            }) => Self::Assign(StmtAssign {
                targets: targets.iter().map(Into::into).collect(),
                value: value.into(),
            }),
            ast::Stmt::AugAssign(ast::StmtAugAssign {
                target,
                op,
                value,
                range: _,
            }) => Self::AugAssign(StmtAugAssign {
                target: target.into(),
                op: (*op).into(),
                value: value.into(),
            }),
            ast::Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                annotation,
                value,
                simple,
                range: _,
            }) => Self::AnnAssign(StmtAnnAssign {
                target: target.into(),
                annotation: annotation.into(),
                value: value.as_ref().map(Into::into),
                simple: *simple,
            }),
            ast::Stmt::For(ast::StmtFor {
                is_async,
                target,
                iter,
                body,
                orelse,
                range: _,
            }) => Self::For(StmtFor {
                is_async: *is_async,
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            }),
            ast::Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range: _,
            }) => Self::While(StmtWhile {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            }),
            ast::Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                range: _,
            }) => Self::If(StmtIf {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                elif_else_clauses: elif_else_clauses.iter().map(Into::into).collect(),
            }),
            ast::Stmt::With(ast::StmtWith {
                is_async,
                items,
                body,
                range: _,
            }) => Self::With(StmtWith {
                is_async: *is_async,
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
            }),
            ast::Stmt::Match(ast::StmtMatch {
                subject,
                cases,
                range: _,
            }) => Self::Match(StmtMatch {
                subject: subject.into(),
                cases: cases.iter().map(Into::into).collect(),
            }),
            ast::Stmt::Raise(ast::StmtRaise {
                exc,
                cause,
                range: _,
            }) => Self::Raise(StmtRaise {
                exc: exc.as_ref().map(Into::into),
                cause: cause.as_ref().map(Into::into),
            }),
            ast::Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                is_star,
                range: _,
            }) => Self::Try(StmtTry {
                body: body.iter().map(Into::into).collect(),
                handlers: handlers.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                finalbody: finalbody.iter().map(Into::into).collect(),
                is_star: *is_star,
            }),
            ast::Stmt::Assert(ast::StmtAssert {
                test,
                msg,
                range: _,
            }) => Self::Assert(StmtAssert {
                test: test.into(),
                msg: msg.as_ref().map(Into::into),
            }),
            ast::Stmt::Import(ast::StmtImport { names, range: _ }) => Self::Import(StmtImport {
                names: names.iter().map(Into::into).collect(),
            }),
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                range: _,
            }) => Self::ImportFrom(StmtImportFrom {
                module: module.as_deref(),
                names: names.iter().map(Into::into).collect(),
                level: *level,
            }),
            ast::Stmt::Global(ast::StmtGlobal { names, range: _ }) => Self::Global(StmtGlobal {
                names: names.iter().map(ast::Identifier::as_str).collect(),
            }),
            ast::Stmt::Nonlocal(ast::StmtNonlocal { names, range: _ }) => {
                Self::Nonlocal(StmtNonlocal {
                    names: names.iter().map(ast::Identifier::as_str).collect(),
                })
            }
            ast::Stmt::IpyEscapeCommand(ast::StmtIpyEscapeCommand {
                kind,
                value,
                range: _,
            }) => Self::IpyEscapeCommand(StmtIpyEscapeCommand { kind: *kind, value }),
            ast::Stmt::Expr(ast::StmtExpr { value, range: _ }) => Self::Expr(StmtExpr {
                value: value.into(),
            }),
            ast::Stmt::Pass(_) => Self::Pass,
            ast::Stmt::Break(_) => Self::Break,
            ast::Stmt::Continue(_) => Self::Continue,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableMod<'a> {
    Module(ComparableModModule<'a>),
    Expression(ComparableModExpression<'a>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableModModule<'a> {
    body: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableModExpression<'a> {
    body: Box<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::Mod> for ComparableMod<'a> {
    fn from(mod_: &'a ast::Mod) -> Self {
        match mod_ {
            ast::Mod::Module(module) => Self::Module(module.into()),
            ast::Mod::Expression(expr) => Self::Expression(expr.into()),
        }
    }
}

impl<'a> From<&'a ast::ModModule> for ComparableModModule<'a> {
    fn from(module: &'a ast::ModModule) -> Self {
        Self {
            body: module.body.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<&'a ast::ModExpression> for ComparableModExpression<'a> {
    fn from(expr: &'a ast::ModExpression) -> Self {
        Self {
            body: (&expr.body).into(),
        }
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
#[derive(Debug)]
pub struct HashableExpr<'a>(ComparableExpr<'a>);

impl Hash for HashableExpr<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl PartialEq<Self> for HashableExpr<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for HashableExpr<'_> {}

impl<'a> From<&'a Expr> for HashableExpr<'a> {
    fn from(expr: &'a Expr) -> Self {
        /// Returns a version of the given expression that can be hashed and compared according to
        /// Python  semantics.
        fn as_hashable(expr: &Expr) -> ComparableExpr {
            match expr {
                Expr::Named(named) => ComparableExpr::NamedExpr(ExprNamed {
                    target: Box::new(ComparableExpr::from(&named.target)),
                    value: Box::new(as_hashable(&named.value)),
                }),
                Expr::NumberLiteral(number) => as_bool(number)
                    .map(|value| ComparableExpr::BoolLiteral(ExprBoolLiteral { value }))
                    .unwrap_or_else(|| ComparableExpr::from(expr)),
                Expr::Tuple(tuple) => ComparableExpr::Tuple(ExprTuple {
                    elts: tuple.iter().map(as_hashable).collect(),
                }),
                _ => ComparableExpr::from(expr),
            }
        }

        /// Returns the `bool` value of the given expression, if it has an equivalent hash to
        /// `True` or `False`.
        fn as_bool(number: &crate::ExprNumberLiteral) -> Option<bool> {
            match &number.value {
                Number::Int(int) => match int.as_u8() {
                    Some(0) => Some(false),
                    Some(1) => Some(true),
                    _ => None,
                },
                Number::Float(float) => match float {
                    0.0 => Some(false),
                    1.0 => Some(true),
                    _ => None,
                },
                Number::Complex { real, imag } => match (real, imag) {
                    (0.0, 0.0) => Some(false),
                    (1.0, 0.0) => Some(true),
                    _ => None,
                },
            }
        }

        Self(as_hashable(expr))
    }
}
