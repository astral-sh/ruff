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

impl<'a, 'ast> From<&'a ast::Alias<'ast>> for ComparableAlias<'ast> {
    fn from(alias: &'a ast::Alias<'ast>) -> Self {
        Self {
            name: alias.name.as_str(),
            asname: alias.asname.as_ref().map(|name| name.as_str()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableWithItem<'a, 'ast> {
    context_expr: ComparableExpr<'a, 'ast>,
    optional_vars: Option<ComparableExpr<'a, 'ast>>,
}

impl<'a, 'ast> From<&'a ast::WithItem<'ast>> for ComparableWithItem<'a, 'ast> {
    fn from(with_item: &'a ast::WithItem<'ast>) -> Self {
        Self {
            context_expr: (&with_item.context_expr).into(),
            optional_vars: with_item.optional_vars.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparablePatternArguments<'a, 'ast> {
    patterns: Vec<ComparablePattern<'a, 'ast>>,
    keywords: Vec<ComparablePatternKeyword<'a, 'ast>>,
}

impl<'a, 'ast> From<&'a ast::PatternArguments<'ast>> for ComparablePatternArguments<'a, 'ast> {
    fn from(parameters: &'a ast::PatternArguments<'ast>) -> Self {
        Self {
            patterns: parameters.patterns.iter().map(Into::into).collect(),
            keywords: parameters.keywords.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparablePatternKeyword<'a, 'ast> {
    attr: &'ast str,
    pattern: ComparablePattern<'a, 'ast>,
}

impl<'a, 'ast> From<&'a ast::PatternKeyword<'ast>> for ComparablePatternKeyword<'a, 'ast> {
    fn from(keyword: &'a ast::PatternKeyword<'ast>) -> Self {
        Self {
            attr: keyword.attr.as_str(),
            pattern: (&keyword.pattern).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchValue<'a, 'ast> {
    value: ComparableExpr<'a, 'ast>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchSingleton {
    value: ComparableSingleton,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchSequence<'a, 'ast> {
    patterns: Vec<ComparablePattern<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchMapping<'a, 'ast> {
    keys: Vec<ComparableExpr<'a, 'ast>>,
    patterns: Vec<ComparablePattern<'a, 'ast>>,
    rest: Option<&'ast str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchClass<'a, 'ast> {
    cls: ComparableExpr<'a, 'ast>,
    arguments: ComparablePatternArguments<'a, 'ast>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchStar<'ast> {
    name: Option<&'ast str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchAs<'a, 'ast> {
    pattern: Option<Box<ComparablePattern<'a, 'ast>>>,
    name: Option<&'ast str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchOr<'a, 'ast> {
    patterns: Vec<ComparablePattern<'a, 'ast>>,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparablePattern<'a, 'ast> {
    MatchValue(PatternMatchValue<'a, 'ast>),
    MatchSingleton(PatternMatchSingleton),
    MatchSequence(PatternMatchSequence<'a, 'ast>),
    MatchMapping(PatternMatchMapping<'a, 'ast>),
    MatchClass(PatternMatchClass<'a, 'ast>),
    MatchStar(PatternMatchStar<'ast>),
    MatchAs(PatternMatchAs<'a, 'ast>),
    MatchOr(PatternMatchOr<'a, 'ast>),
}

impl<'a, 'ast> From<&'a ast::Pattern<'ast>> for ComparablePattern<'a, 'ast> {
    fn from(pattern: &'a ast::Pattern<'ast>) -> Self {
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
                rest: rest.as_ref().map(|rest| rest.as_str()),
            }),
            ast::Pattern::MatchClass(ast::PatternMatchClass { cls, arguments, .. }) => {
                Self::MatchClass(PatternMatchClass {
                    cls: cls.into(),
                    arguments: arguments.into(),
                })
            }
            ast::Pattern::MatchStar(ast::PatternMatchStar { name, .. }) => {
                Self::MatchStar(PatternMatchStar {
                    name: name.as_ref().map(|name| name.as_str()),
                })
            }
            ast::Pattern::MatchAs(ast::PatternMatchAs { pattern, name, .. }) => {
                Self::MatchAs(PatternMatchAs {
                    pattern: pattern.as_ref().map(|pattern| pattern.into()),
                    name: name.as_ref().map(|name| name.as_str()),
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

impl<'a, 'ast> From<&'a ruff_allocator::Box<'ast, ast::Pattern<'ast>>>
    for Box<ComparablePattern<'a, 'ast>>
{
    fn from(pattern: &'a ruff_allocator::Box<'a, ast::Pattern<'ast>>) -> Self {
        Box::new((&**pattern).into())
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableMatchCase<'a, 'ast> {
    pattern: ComparablePattern<'a, 'ast>,
    guard: Option<ComparableExpr<'a, 'ast>>,
    body: Vec<ComparableStmt<'a, 'ast>>,
}

impl<'a, 'ast> From<&'a ast::MatchCase<'ast>> for ComparableMatchCase<'a, 'ast> {
    fn from(match_case: &'a ast::MatchCase<'ast>) -> Self {
        Self {
            pattern: (&match_case.pattern).into(),
            guard: match_case.guard.as_ref().map(Into::into),
            body: match_case.body.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableDecorator<'a, 'ast> {
    expression: ComparableExpr<'a, 'ast>,
}

impl<'a, 'ast> From<&'a ast::Decorator<'ast>> for ComparableDecorator<'a, 'ast> {
    fn from(decorator: &'a ast::Decorator<'ast>) -> Self {
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
pub enum ComparableNumber<'a, 'ast> {
    Int(&'a ast::Int<'ast>),
    Float(u64),
    Complex { real: u64, imag: u64 },
}

impl<'a, 'ast> From<&'a ast::Number<'ast>> for ComparableNumber<'a, 'ast> {
    fn from(number: &'a ast::Number<'ast>) -> Self {
        match number {
            ast::Number::Int(value) => Self::Int(&value),
            ast::Number::Float(value) => Self::Float(value.to_bits()),
            ast::Number::Complex { real, imag } => Self::Complex {
                real: real.to_bits(),
                imag: imag.to_bits(),
            },
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Hash)]
pub struct ComparableArguments<'a, 'ast> {
    args: Vec<ComparableExpr<'a, 'ast>>,
    keywords: Vec<ComparableKeyword<'a, 'ast>>,
}

impl<'a, 'ast> From<&'a ast::Arguments<'ast>> for ComparableArguments<'a, 'ast> {
    fn from(arguments: &'a ast::Arguments<'ast>) -> Self {
        Self {
            args: arguments.args.iter().map(Into::into).collect(),
            keywords: arguments.keywords.iter().map(Into::into).collect(),
        }
    }
}

impl<'a, 'ast> From<&'a ruff_allocator::Box<'ast, ast::Arguments<'ast>>>
    for ComparableArguments<'a, 'ast>
{
    fn from(arguments: &'a ruff_allocator::Box<'a, ast::Arguments<'ast>>) -> Self {
        (arguments.as_ref()).into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableParameters<'a, 'ast> {
    posonlyargs: Vec<ComparableParameterWithDefault<'a, 'ast>>,
    args: Vec<ComparableParameterWithDefault<'a, 'ast>>,
    vararg: Option<ComparableParameter<'a, 'ast>>,
    kwonlyargs: Vec<ComparableParameterWithDefault<'a, 'ast>>,
    kwarg: Option<ComparableParameter<'a, 'ast>>,
}

impl<'a, 'ast> From<&'a ast::Parameters<'ast>> for ComparableParameters<'a, 'ast> {
    fn from(parameters: &'a ast::Parameters<'ast>) -> Self {
        Self {
            posonlyargs: parameters.posonlyargs.iter().map(Into::into).collect(),
            args: parameters.args.iter().map(Into::into).collect(),
            vararg: parameters.vararg.as_ref().map(Into::into),
            kwonlyargs: parameters.kwonlyargs.iter().map(Into::into).collect(),
            kwarg: parameters.kwarg.as_ref().map(Into::into),
        }
    }
}

impl<'a, 'ast> From<&'a ruff_allocator::Box<'ast, ast::Parameters<'ast>>>
    for ComparableParameters<'a, 'ast>
{
    fn from(parameters: &'a ruff_allocator::Box<'ast, ast::Parameters<'ast>>) -> Self {
        (&*parameters).into()
    }
}

impl<'a, 'ast> From<&'a ruff_allocator::Box<'ast, ast::Parameter<'ast>>>
    for ComparableParameter<'a, 'ast>
{
    fn from(arg: &'a ruff_allocator::Box<'ast, ast::Parameter<'ast>>) -> Self {
        (&*arg).into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableParameter<'a, 'ast> {
    arg: &'ast str,
    annotation: Option<Box<ComparableExpr<'a, 'ast>>>,
}

impl<'a, 'ast> From<&'a ast::Parameter<'ast>> for ComparableParameter<'a, 'ast> {
    fn from(arg: &'a ast::Parameter<'ast>) -> Self {
        Self {
            arg: arg.name.as_str(),
            annotation: arg.annotation.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableParameterWithDefault<'a, 'ast> {
    def: ComparableParameter<'a, 'ast>,
    default: Option<ComparableExpr<'a, 'ast>>,
}

impl<'a, 'ast> From<&'a ast::ParameterWithDefault<'ast>>
    for ComparableParameterWithDefault<'a, 'ast>
{
    fn from(arg: &'a ast::ParameterWithDefault<'ast>) -> Self {
        Self {
            def: (&arg.parameter).into(),
            default: arg.default.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableKeyword<'a, 'ast> {
    arg: Option<&'ast str>,
    value: ComparableExpr<'a, 'ast>,
}

impl<'a, 'ast> From<&'a ast::Keyword<'ast>> for ComparableKeyword<'a, 'ast> {
    fn from(keyword: &'a ast::Keyword<'ast>) -> Self {
        Self {
            arg: keyword.arg.as_ref().map(ast::Identifier::as_str),
            value: (&keyword.value).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableComprehension<'a, 'ast> {
    target: ComparableExpr<'a, 'ast>,
    iter: ComparableExpr<'a, 'ast>,
    ifs: Vec<ComparableExpr<'a, 'ast>>,
    is_async: bool,
}

impl<'a, 'ast> From<&'a ast::Comprehension<'ast>> for ComparableComprehension<'a, 'ast> {
    fn from(comprehension: &'a ast::Comprehension<'ast>) -> Self {
        Self {
            target: (&comprehension.target).into(),
            iter: (&comprehension.iter).into(),
            ifs: comprehension.ifs.iter().map(Into::into).collect(),
            is_async: comprehension.is_async,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExceptHandlerExceptHandler<'a, 'ast> {
    type_: Option<Box<ComparableExpr<'a, 'ast>>>,
    name: Option<&'ast str>,
    body: Vec<ComparableStmt<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableExceptHandler<'a, 'ast> {
    ExceptHandler(ExceptHandlerExceptHandler<'a, 'ast>),
}

impl<'a, 'ast> From<&'a ast::ExceptHandler<'ast>> for ComparableExceptHandler<'a, 'ast> {
    fn from(except_handler: &'a ast::ExceptHandler<'ast>) -> Self {
        let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_,
            name,
            body,
            ..
        }) = except_handler;
        Self::ExceptHandler(ExceptHandlerExceptHandler {
            type_: type_.as_ref().map(Into::into),
            name: name.as_ref().map(|name| name.as_str()),
            body: body.iter().map(Into::into).collect(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableFStringElement<'a, 'ast> {
    Literal(&'ast str),
    FStringExpressionElement(FStringExpressionElement<'a, 'ast>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct FStringExpressionElement<'a, 'ast> {
    expression: ComparableExpr<'a, 'ast>,
    debug_text: Option<&'a ast::DebugText>,
    conversion: ast::ConversionFlag,
    format_spec: Option<Vec<ComparableFStringElement<'a, 'ast>>>,
}

impl<'a, 'ast> From<&'a ast::FStringElement<'ast>> for ComparableFStringElement<'a, 'ast> {
    fn from(fstring_element: &'a ast::FStringElement<'ast>) -> Self {
        match fstring_element {
            ast::FStringElement::Literal(ast::FStringLiteralElement { value, .. }) => {
                Self::Literal(value)
            }
            ast::FStringElement::Expression(formatted_value) => {
                Self::FStringExpressionElement(FStringExpressionElement {
                    expression: (&formatted_value.expression).into(),
                    debug_text: formatted_value.debug_text.as_ref(),
                    conversion: formatted_value.conversion,
                    format_spec: formatted_value
                        .format_spec
                        .as_ref()
                        .map(|spec| spec.elements.iter().map(Into::into).collect()),
                })
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableElifElseClause<'a, 'ast> {
    test: Option<ComparableExpr<'a, 'ast>>,
    body: Vec<ComparableStmt<'a, 'ast>>,
}

impl<'a, 'ast> From<&'a ast::ElifElseClause<'ast>> for ComparableElifElseClause<'a, 'ast> {
    fn from(elif_else_clause: &'a ast::ElifElseClause<'ast>) -> Self {
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
pub enum ComparableLiteral<'a, 'ast> {
    None,
    Ellipsis,
    Bool(bool),
    Str(Vec<ComparableStringLiteral<'ast>>),
    Bytes(Vec<ComparableBytesLiteral<'ast>>),
    Number(ComparableNumber<'a, 'ast>),
}

impl<'a, 'ast> From<ast::LiteralExpressionRef<'a, 'ast>> for ComparableLiteral<'a, 'ast> {
    fn from(literal: ast::LiteralExpressionRef<'a, 'ast>) -> Self {
        match literal {
            ast::LiteralExpressionRef::NoneLiteral(_) => Self::None,
            ast::LiteralExpressionRef::EllipsisLiteral(_) => Self::Ellipsis,
            ast::LiteralExpressionRef::BooleanLiteral(ast::ExprBooleanLiteral {
                value, ..
            }) => Self::Bool(*value),
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
pub struct ComparableFString<'a, 'ast> {
    elements: Vec<ComparableFStringElement<'a, 'ast>>,
}

impl<'a, 'ast> From<&'a ast::FString<'ast>> for ComparableFString<'a, 'ast> {
    fn from(fstring: &'a ast::FString<'ast>) -> Self {
        Self {
            elements: fstring.elements.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableFStringPart<'a, 'ast> {
    Literal(ComparableStringLiteral<'ast>),
    FString(ComparableFString<'a, 'ast>),
}

impl<'a, 'ast> From<&'a ast::FStringPart<'ast>> for ComparableFStringPart<'a, 'ast> {
    fn from(f_string_part: &'a ast::FStringPart<'ast>) -> Self {
        match f_string_part {
            ast::FStringPart::Literal(string_literal) => Self::Literal(string_literal.into()),
            ast::FStringPart::FString(f_string) => Self::FString(f_string.into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableStringLiteral<'a> {
    value: &'a str,
}

impl<'a, 'ast> From<&'a ast::StringLiteral<'ast>> for ComparableStringLiteral<'ast> {
    fn from(string_literal: &'a ast::StringLiteral<'ast>) -> Self {
        Self {
            value: &string_literal.value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableBytesLiteral<'ast> {
    value: &'ast [u8],
}

impl<'a, 'ast> From<&'a ast::BytesLiteral<'ast>> for ComparableBytesLiteral<'ast> {
    fn from(bytes_literal: &'a ast::BytesLiteral<'ast>) -> Self {
        Self {
            value: &bytes_literal.value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprBoolOp<'a, 'ast> {
    op: ComparableBoolOp,
    values: Vec<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprNamed<'a, 'ast> {
    target: Box<ComparableExpr<'a, 'ast>>,
    value: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprBinOp<'a, 'ast> {
    left: Box<ComparableExpr<'a, 'ast>>,
    op: ComparableOperator,
    right: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprUnaryOp<'a, 'ast> {
    op: ComparableUnaryOp,
    operand: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprLambda<'a, 'ast> {
    parameters: Option<ComparableParameters<'a, 'ast>>,
    body: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprIf<'a, 'ast> {
    test: Box<ComparableExpr<'a, 'ast>>,
    body: Box<ComparableExpr<'a, 'ast>>,
    orelse: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableDictItem<'a, 'ast> {
    key: Option<ComparableExpr<'a, 'ast>>,
    value: ComparableExpr<'a, 'ast>,
}

impl<'a, 'ast> From<&'a ast::DictItem<'ast>> for ComparableDictItem<'a, 'ast> {
    fn from(ast::DictItem { key, value }: &'a ast::DictItem<'ast>) -> Self {
        Self {
            key: key.as_ref().map(ComparableExpr::from),
            value: value.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprDict<'a, 'ast> {
    items: Vec<ComparableDictItem<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSet<'a, 'ast> {
    elts: Vec<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprListComp<'a, 'ast> {
    elt: Box<ComparableExpr<'a, 'ast>>,
    generators: Vec<ComparableComprehension<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSetComp<'a, 'ast> {
    elt: Box<ComparableExpr<'a, 'ast>>,
    generators: Vec<ComparableComprehension<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprDictComp<'a, 'ast> {
    key: Box<ComparableExpr<'a, 'ast>>,
    value: Box<ComparableExpr<'a, 'ast>>,
    generators: Vec<ComparableComprehension<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprGenerator<'a, 'ast> {
    elt: Box<ComparableExpr<'a, 'ast>>,
    generators: Vec<ComparableComprehension<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprAwait<'a, 'ast> {
    value: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprYield<'a, 'ast> {
    value: Option<Box<ComparableExpr<'a, 'ast>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprYieldFrom<'a, 'ast> {
    value: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprCompare<'a, 'ast> {
    left: Box<ComparableExpr<'a, 'ast>>,
    ops: Vec<ComparableCmpOp>,
    comparators: Vec<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprCall<'a, 'ast> {
    func: Box<ComparableExpr<'a, 'ast>>,
    arguments: ComparableArguments<'a, 'ast>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprFStringExpressionElement<'a, 'ast> {
    value: Box<ComparableExpr<'a, 'ast>>,
    debug_text: Option<&'a ast::DebugText>,
    conversion: ast::ConversionFlag,
    format_spec: Vec<ComparableFStringElement<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprFString<'a, 'ast> {
    parts: Vec<ComparableFStringPart<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprStringLiteral<'ast> {
    parts: Vec<ComparableStringLiteral<'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprBytesLiteral<'ast> {
    parts: Vec<ComparableBytesLiteral<'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprNumberLiteral<'a, 'ast> {
    value: ComparableNumber<'a, 'ast>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprBoolLiteral {
    value: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprAttribute<'a, 'ast> {
    value: Box<ComparableExpr<'a, 'ast>>,
    attr: &'ast str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSubscript<'a, 'ast> {
    value: Box<ComparableExpr<'a, 'ast>>,
    slice: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprStarred<'a, 'ast> {
    value: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprName<'ast> {
    id: &'ast str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprList<'a, 'ast> {
    elts: Vec<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprTuple<'a, 'ast> {
    elts: Vec<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSlice<'a, 'ast> {
    lower: Option<Box<ComparableExpr<'a, 'ast>>>,
    upper: Option<Box<ComparableExpr<'a, 'ast>>>,
    step: Option<Box<ComparableExpr<'a, 'ast>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprIpyEscapeCommand<'ast> {
    kind: ast::IpyEscapeKind,
    value: &'ast str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableExpr<'a, 'ast> {
    BoolOp(ExprBoolOp<'a, 'ast>),
    NamedExpr(ExprNamed<'a, 'ast>),
    BinOp(ExprBinOp<'a, 'ast>),
    UnaryOp(ExprUnaryOp<'a, 'ast>),
    Lambda(ExprLambda<'a, 'ast>),
    IfExp(ExprIf<'a, 'ast>),
    Dict(ExprDict<'a, 'ast>),
    Set(ExprSet<'a, 'ast>),
    ListComp(ExprListComp<'a, 'ast>),
    SetComp(ExprSetComp<'a, 'ast>),
    DictComp(ExprDictComp<'a, 'ast>),
    GeneratorExp(ExprGenerator<'a, 'ast>),
    Await(ExprAwait<'a, 'ast>),
    Yield(ExprYield<'a, 'ast>),
    YieldFrom(ExprYieldFrom<'a, 'ast>),
    Compare(ExprCompare<'a, 'ast>),
    Call(ExprCall<'a, 'ast>),
    FStringExpressionElement(ExprFStringExpressionElement<'a, 'ast>),
    FString(ExprFString<'a, 'ast>),
    StringLiteral(ExprStringLiteral<'ast>),
    BytesLiteral(ExprBytesLiteral<'ast>),
    NumberLiteral(ExprNumberLiteral<'a, 'ast>),
    BoolLiteral(ExprBoolLiteral),
    NoneLiteral,
    EllipsisLiteral,
    Attribute(ExprAttribute<'a, 'ast>),
    Subscript(ExprSubscript<'a, 'ast>),
    Starred(ExprStarred<'a, 'ast>),
    Name(ExprName<'ast>),
    List(ExprList<'a, 'ast>),
    Tuple(ExprTuple<'a, 'ast>),
    Slice(ExprSlice<'a, 'ast>),
    IpyEscapeCommand(ExprIpyEscapeCommand<'ast>),
}

impl<'a, 'ast> From<&'a ruff_allocator::Box<'ast, ast::Expr<'ast>>>
    for Box<ComparableExpr<'a, 'ast>>
{
    fn from(expr: &'a ruff_allocator::Box<'ast, ast::Expr<'ast>>) -> Self {
        Box::new((&*expr).into())
    }
}

impl<'a, 'ast> From<&'a ruff_allocator::Box<'ast, ast::Expr<'ast>>> for ComparableExpr<'a, 'ast> {
    fn from(expr: &'a ruff_allocator::Box<'ast, ast::Expr<'ast>>) -> Self {
        (&*expr).into()
    }
}

impl<'a, 'ast> From<&'a ast::Expr<'ast>> for ComparableExpr<'a, 'ast> {
    fn from(expr: &'a ast::Expr<'ast>) -> Self {
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
                    parts: value.iter().map(Into::into).collect(),
                })
            }
            ast::Expr::StringLiteral(ast::ExprStringLiteral { value, range: _ }) => {
                Self::StringLiteral(ExprStringLiteral {
                    parts: value.iter().map(Into::into).collect(),
                })
            }
            ast::Expr::BytesLiteral(ast::ExprBytesLiteral { value, range: _ }) => {
                Self::BytesLiteral(ExprBytesLiteral {
                    parts: value.iter().map(Into::into).collect(),
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

impl<'a, 'ast> From<&'a ast::ExprName<'ast>> for ComparableExpr<'a, 'ast> {
    fn from(expr: &'a ast::ExprName<'ast>) -> Self {
        Self::Name(ExprName { id: expr.id })
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtFunctionDef<'a, 'ast> {
    is_async: bool,
    decorator_list: Vec<ComparableDecorator<'a, 'ast>>,
    name: &'ast str,
    type_params: Option<ComparableTypeParams<'a, 'ast>>,
    parameters: ComparableParameters<'a, 'ast>,
    returns: Option<ComparableExpr<'a, 'ast>>,
    body: Vec<ComparableStmt<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtClassDef<'a, 'ast> {
    decorator_list: Vec<ComparableDecorator<'a, 'ast>>,
    name: &'ast str,
    type_params: Option<ComparableTypeParams<'a, 'ast>>,
    arguments: ComparableArguments<'a, 'ast>,
    body: Vec<ComparableStmt<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtReturn<'a, 'ast> {
    value: Option<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtDelete<'a, 'ast> {
    targets: Vec<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtTypeAlias<'a, 'ast> {
    pub name: Box<ComparableExpr<'a, 'ast>>,
    pub type_params: Option<ComparableTypeParams<'a, 'ast>>,
    pub value: Box<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableTypeParams<'a, 'ast> {
    pub type_params: Vec<ComparableTypeParam<'a, 'ast>>,
}

impl<'a, 'ast> From<&'a ast::TypeParams<'ast>> for ComparableTypeParams<'a, 'ast> {
    fn from(type_params: &'a ast::TypeParams<'ast>) -> Self {
        Self {
            type_params: type_params.iter().map(Into::into).collect(),
        }
    }
}

impl<'a, 'ast> From<&'a ruff_allocator::Box<'ast, ast::TypeParams<'ast>>>
    for ComparableTypeParams<'a, 'ast>
{
    fn from(type_params: &'a ruff_allocator::Box<'a, ast::TypeParams<'ast>>) -> Self {
        type_params.as_ref().into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableTypeParam<'a, 'ast> {
    TypeVar(TypeParamTypeVar<'a, 'ast>),
    ParamSpec(TypeParamParamSpec<'a, 'ast>),
    TypeVarTuple(TypeParamTypeVarTuple<'a, 'ast>),
}

impl<'a, 'ast> From<&'a ast::TypeParam<'ast>> for ComparableTypeParam<'a, 'ast> {
    fn from(type_param: &'a ast::TypeParam<'ast>) -> Self {
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
pub struct TypeParamTypeVar<'a, 'ast> {
    pub name: &'ast str,
    pub bound: Option<Box<ComparableExpr<'a, 'ast>>>,
    pub default: Option<Box<ComparableExpr<'a, 'ast>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TypeParamParamSpec<'a, 'ast> {
    pub name: &'ast str,
    pub default: Option<Box<ComparableExpr<'a, 'ast>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TypeParamTypeVarTuple<'a, 'ast> {
    pub name: &'ast str,
    pub default: Option<Box<ComparableExpr<'a, 'ast>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAssign<'a, 'ast> {
    targets: Vec<ComparableExpr<'a, 'ast>>,
    value: ComparableExpr<'a, 'ast>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAugAssign<'a, 'ast> {
    target: ComparableExpr<'a, 'ast>,
    op: ComparableOperator,
    value: ComparableExpr<'a, 'ast>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAnnAssign<'a, 'ast> {
    target: ComparableExpr<'a, 'ast>,
    annotation: ComparableExpr<'a, 'ast>,
    value: Option<ComparableExpr<'a, 'ast>>,
    simple: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtFor<'a, 'ast> {
    is_async: bool,
    target: ComparableExpr<'a, 'ast>,
    iter: ComparableExpr<'a, 'ast>,
    body: Vec<ComparableStmt<'a, 'ast>>,
    orelse: Vec<ComparableStmt<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtWhile<'a, 'ast> {
    test: ComparableExpr<'a, 'ast>,
    body: Vec<ComparableStmt<'a, 'ast>>,
    orelse: Vec<ComparableStmt<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtIf<'a, 'ast> {
    test: ComparableExpr<'a, 'ast>,
    body: Vec<ComparableStmt<'a, 'ast>>,
    elif_else_clauses: Vec<ComparableElifElseClause<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtWith<'a, 'ast> {
    is_async: bool,
    items: Vec<ComparableWithItem<'a, 'ast>>,
    body: Vec<ComparableStmt<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtMatch<'a, 'ast> {
    subject: ComparableExpr<'a, 'ast>,
    cases: Vec<ComparableMatchCase<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtRaise<'a, 'ast> {
    exc: Option<ComparableExpr<'a, 'ast>>,
    cause: Option<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtTry<'a, 'ast> {
    body: Vec<ComparableStmt<'a, 'ast>>,
    handlers: Vec<ComparableExceptHandler<'a, 'ast>>,
    orelse: Vec<ComparableStmt<'a, 'ast>>,
    finalbody: Vec<ComparableStmt<'a, 'ast>>,
    is_star: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAssert<'a, 'ast> {
    test: ComparableExpr<'a, 'ast>,
    msg: Option<ComparableExpr<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtImport<'ast> {
    names: Vec<ComparableAlias<'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtImportFrom<'ast> {
    module: Option<&'ast str>,
    names: Vec<ComparableAlias<'ast>>,
    level: u32,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtGlobal<'ast> {
    names: Vec<&'ast str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtNonlocal<'ast> {
    names: Vec<&'ast str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtExpr<'a, 'ast> {
    value: ComparableExpr<'a, 'ast>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtIpyEscapeCommand<'ast> {
    kind: ast::IpyEscapeKind,
    value: &'ast str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableStmt<'a, 'ast> {
    FunctionDef(StmtFunctionDef<'a, 'ast>),
    ClassDef(StmtClassDef<'a, 'ast>),
    Return(StmtReturn<'a, 'ast>),
    Delete(StmtDelete<'a, 'ast>),
    Assign(StmtAssign<'a, 'ast>),
    AugAssign(StmtAugAssign<'a, 'ast>),
    AnnAssign(StmtAnnAssign<'a, 'ast>),
    For(StmtFor<'a, 'ast>),
    While(StmtWhile<'a, 'ast>),
    If(StmtIf<'a, 'ast>),
    With(StmtWith<'a, 'ast>),
    Match(StmtMatch<'a, 'ast>),
    Raise(StmtRaise<'a, 'ast>),
    Try(StmtTry<'a, 'ast>),
    TypeAlias(StmtTypeAlias<'a, 'ast>),
    Assert(StmtAssert<'a, 'ast>),
    Import(StmtImport<'ast>),
    ImportFrom(StmtImportFrom<'ast>),
    Global(StmtGlobal<'ast>),
    Nonlocal(StmtNonlocal<'ast>),
    IpyEscapeCommand(StmtIpyEscapeCommand<'ast>),
    Expr(StmtExpr<'a, 'ast>),
    Pass,
    Break,
    Continue,
}

impl<'a, 'ast> From<&'a ast::Stmt<'ast>> for ComparableStmt<'a, 'ast> {
    fn from(stmt: &'a ast::Stmt<'ast>) -> Self {
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
                module: module.as_ref().map(|name| name.as_str()),
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
pub enum ComparableMod<'a, 'ast> {
    Module(ComparableModModule<'a, 'ast>),
    Expression(ComparableModExpression<'a, 'ast>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableModModule<'a, 'ast> {
    body: Vec<ComparableStmt<'a, 'ast>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableModExpression<'a, 'ast> {
    body: ComparableExpr<'a, 'ast>,
}

impl<'a, 'ast> From<&'a ast::Mod<'ast>> for ComparableMod<'a, 'ast> {
    fn from(mod_: &'a ast::Mod<'ast>) -> Self {
        match mod_ {
            ast::Mod::Module(module) => Self::Module(module.into()),
            ast::Mod::Expression(expr) => Self::Expression(expr.into()),
        }
    }
}

impl<'a, 'ast> From<&'a ast::ModModule<'ast>> for ComparableModModule<'a, 'ast> {
    fn from(module: &'a ast::ModModule<'ast>) -> Self {
        Self {
            body: module.body.iter().map(Into::into).collect(),
        }
    }
}

impl<'a, 'ast> From<&'a ast::ModExpression<'ast>> for ComparableModExpression<'a, 'ast> {
    fn from(expr: &'a ast::ModExpression<'ast>) -> Self {
        Self {
            body: (&expr.body).into(),
        }
    }
}
