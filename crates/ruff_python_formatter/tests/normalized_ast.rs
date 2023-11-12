//! An equivalent object hierarchy to the `RustPython` AST hierarchy, but with the
//! ability to compare nodes for equality after formatting.
//!
//! Vis-à-vis comparing ASTs, comparing these normalized representations does the following:
//! - Removes all locations from the AST.
//! - Ignores non-abstraction information that we've encoded into the AST, e.g., the difference
//!   between `class C: ...` and `class C(): ...`, which is part of our AST but not `CPython`'s.
//! - Normalize strings. The formatter can re-indent docstrings, so we need to compare string
//!   contents ignoring whitespace. (Black does the same.)
//! - Ignores nested tuples in deletions. (Black does the same.)

use itertools::Either::{Left, Right};

use ruff_python_ast as ast;

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
enum NormalizedBoolOp {
    And,
    Or,
}

impl From<ast::BoolOp> for NormalizedBoolOp {
    fn from(op: ast::BoolOp) -> Self {
        match op {
            ast::BoolOp::And => Self::And,
            ast::BoolOp::Or => Self::Or,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
enum NormalizedOperator {
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

impl From<ast::Operator> for NormalizedOperator {
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
enum NormalizedUnaryOp {
    Invert,
    Not,
    UAdd,
    USub,
}

impl From<ast::UnaryOp> for NormalizedUnaryOp {
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
enum NormalizedCmpOp {
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

impl From<ast::CmpOp> for NormalizedCmpOp {
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
struct NormalizedAlias<'a> {
    name: &'a str,
    asname: Option<&'a str>,
}

impl<'a> From<&'a ast::Alias> for NormalizedAlias<'a> {
    fn from(alias: &'a ast::Alias) -> Self {
        Self {
            name: alias.name.as_str(),
            asname: alias.asname.as_deref(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedWithItem<'a> {
    context_expr: NormalizedExpr<'a>,
    optional_vars: Option<NormalizedExpr<'a>>,
}

impl<'a> From<&'a ast::WithItem> for NormalizedWithItem<'a> {
    fn from(with_item: &'a ast::WithItem) -> Self {
        Self {
            context_expr: (&with_item.context_expr).into(),
            optional_vars: with_item.optional_vars.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedPatternArguments<'a> {
    patterns: Vec<NormalizedPattern<'a>>,
    keywords: Vec<NormalizedPatternKeyword<'a>>,
}

impl<'a> From<&'a ast::PatternArguments> for NormalizedPatternArguments<'a> {
    fn from(parameters: &'a ast::PatternArguments) -> Self {
        Self {
            patterns: parameters.patterns.iter().map(Into::into).collect(),
            keywords: parameters.keywords.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedPatternKeyword<'a> {
    attr: &'a str,
    pattern: NormalizedPattern<'a>,
}

impl<'a> From<&'a ast::PatternKeyword> for NormalizedPatternKeyword<'a> {
    fn from(keyword: &'a ast::PatternKeyword) -> Self {
        Self {
            attr: keyword.attr.as_str(),
            pattern: (&keyword.pattern).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct PatternMatchValue<'a> {
    value: NormalizedExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct PatternMatchSingleton {
    value: NormalizedSingleton,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct PatternMatchSequence<'a> {
    patterns: Vec<NormalizedPattern<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct PatternMatchMapping<'a> {
    keys: Vec<NormalizedExpr<'a>>,
    patterns: Vec<NormalizedPattern<'a>>,
    rest: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct PatternMatchClass<'a> {
    cls: NormalizedExpr<'a>,
    arguments: NormalizedPatternArguments<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct PatternMatchStar<'a> {
    name: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct PatternMatchAs<'a> {
    pattern: Option<Box<NormalizedPattern<'a>>>,
    name: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct PatternMatchOr<'a> {
    patterns: Vec<NormalizedPattern<'a>>,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, PartialEq, Eq, Hash)]
enum NormalizedPattern<'a> {
    MatchValue(PatternMatchValue<'a>),
    MatchSingleton(PatternMatchSingleton),
    MatchSequence(PatternMatchSequence<'a>),
    MatchMapping(PatternMatchMapping<'a>),
    MatchClass(PatternMatchClass<'a>),
    MatchStar(PatternMatchStar<'a>),
    MatchAs(PatternMatchAs<'a>),
    MatchOr(PatternMatchOr<'a>),
}

impl<'a> From<&'a ast::Pattern> for NormalizedPattern<'a> {
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

impl<'a> From<&'a Box<ast::Pattern>> for Box<NormalizedPattern<'a>> {
    fn from(pattern: &'a Box<ast::Pattern>) -> Self {
        Box::new((pattern.as_ref()).into())
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedMatchCase<'a> {
    pattern: NormalizedPattern<'a>,
    guard: Option<NormalizedExpr<'a>>,
    body: Vec<NormalizedStmt<'a>>,
}

impl<'a> From<&'a ast::MatchCase> for NormalizedMatchCase<'a> {
    fn from(match_case: &'a ast::MatchCase) -> Self {
        Self {
            pattern: (&match_case.pattern).into(),
            guard: match_case.guard.as_ref().map(Into::into),
            body: match_case.body.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedDecorator<'a> {
    expression: NormalizedExpr<'a>,
}

impl<'a> From<&'a ast::Decorator> for NormalizedDecorator<'a> {
    fn from(decorator: &'a ast::Decorator) -> Self {
        Self {
            expression: (&decorator.expression).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum NormalizedSingleton {
    None,
    True,
    False,
}

impl From<&ast::Singleton> for NormalizedSingleton {
    fn from(singleton: &ast::Singleton) -> Self {
        match singleton {
            ast::Singleton::None => Self::None,
            ast::Singleton::True => Self::True,
            ast::Singleton::False => Self::False,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum NormalizedNumber<'a> {
    Int(&'a ast::Int),
    Float(u64),
    Complex { real: u64, imag: u64 },
}

impl<'a> From<&'a ast::Number> for NormalizedNumber<'a> {
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

#[derive(Debug, PartialEq, Eq, Hash, Default)]
struct NormalizedArguments<'a> {
    args: Vec<NormalizedExpr<'a>>,
    keywords: Vec<NormalizedKeyword<'a>>,
}

impl<'a> From<&'a ast::Arguments> for NormalizedArguments<'a> {
    fn from(arguments: &'a ast::Arguments) -> Self {
        Self {
            args: arguments.args.iter().map(Into::into).collect(),
            keywords: arguments.keywords.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<&'a Box<ast::Arguments>> for NormalizedArguments<'a> {
    fn from(arguments: &'a Box<ast::Arguments>) -> Self {
        (arguments.as_ref()).into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedParameters<'a> {
    posonlyargs: Vec<NormalizedParameterWithDefault<'a>>,
    args: Vec<NormalizedParameterWithDefault<'a>>,
    vararg: Option<NormalizedParameter<'a>>,
    kwonlyargs: Vec<NormalizedParameterWithDefault<'a>>,
    kwarg: Option<NormalizedParameter<'a>>,
}

impl<'a> From<&'a ast::Parameters> for NormalizedParameters<'a> {
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

impl<'a> From<&'a Box<ast::Parameters>> for NormalizedParameters<'a> {
    fn from(parameters: &'a Box<ast::Parameters>) -> Self {
        (parameters.as_ref()).into()
    }
}

impl<'a> From<&'a Box<ast::Parameter>> for NormalizedParameter<'a> {
    fn from(arg: &'a Box<ast::Parameter>) -> Self {
        (arg.as_ref()).into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedParameter<'a> {
    arg: &'a str,
    annotation: Option<Box<NormalizedExpr<'a>>>,
}

impl<'a> From<&'a ast::Parameter> for NormalizedParameter<'a> {
    fn from(arg: &'a ast::Parameter) -> Self {
        Self {
            arg: arg.name.as_str(),
            annotation: arg.annotation.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedParameterWithDefault<'a> {
    def: NormalizedParameter<'a>,
    default: Option<NormalizedExpr<'a>>,
}

impl<'a> From<&'a ast::ParameterWithDefault> for NormalizedParameterWithDefault<'a> {
    fn from(arg: &'a ast::ParameterWithDefault) -> Self {
        Self {
            def: (&arg.parameter).into(),
            default: arg.default.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedKeyword<'a> {
    arg: Option<&'a str>,
    value: NormalizedExpr<'a>,
}

impl<'a> From<&'a ast::Keyword> for NormalizedKeyword<'a> {
    fn from(keyword: &'a ast::Keyword) -> Self {
        Self {
            arg: keyword.arg.as_ref().map(ast::Identifier::as_str),
            value: (&keyword.value).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedComprehension<'a> {
    target: NormalizedExpr<'a>,
    iter: NormalizedExpr<'a>,
    ifs: Vec<NormalizedExpr<'a>>,
    is_async: bool,
}

impl<'a> From<&'a ast::Comprehension> for NormalizedComprehension<'a> {
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
struct ExceptHandlerExceptHandler<'a> {
    type_: Option<Box<NormalizedExpr<'a>>>,
    name: Option<&'a str>,
    body: Vec<NormalizedStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum NormalizedExceptHandler<'a> {
    ExceptHandler(ExceptHandlerExceptHandler<'a>),
}

impl<'a> From<&'a ast::ExceptHandler> for NormalizedExceptHandler<'a> {
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
struct NormalizedElifElseClause<'a> {
    test: Option<NormalizedExpr<'a>>,
    body: Vec<NormalizedStmt<'a>>,
}

impl<'a> From<&'a ast::ElifElseClause> for NormalizedElifElseClause<'a> {
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
struct ExprBoolOp<'a> {
    op: NormalizedBoolOp,
    values: Vec<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprNamedExpr<'a> {
    target: Box<NormalizedExpr<'a>>,
    value: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprBinOp<'a> {
    left: Box<NormalizedExpr<'a>>,
    op: NormalizedOperator,
    right: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprUnaryOp<'a> {
    op: NormalizedUnaryOp,
    operand: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprLambda<'a> {
    parameters: Option<NormalizedParameters<'a>>,
    body: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprIfExp<'a> {
    test: Box<NormalizedExpr<'a>>,
    body: Box<NormalizedExpr<'a>>,
    orelse: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprDict<'a> {
    keys: Vec<Option<NormalizedExpr<'a>>>,
    values: Vec<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprSet<'a> {
    elts: Vec<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprListComp<'a> {
    elt: Box<NormalizedExpr<'a>>,
    generators: Vec<NormalizedComprehension<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprSetComp<'a> {
    elt: Box<NormalizedExpr<'a>>,
    generators: Vec<NormalizedComprehension<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprDictComp<'a> {
    key: Box<NormalizedExpr<'a>>,
    value: Box<NormalizedExpr<'a>>,
    generators: Vec<NormalizedComprehension<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprGeneratorExp<'a> {
    elt: Box<NormalizedExpr<'a>>,
    generators: Vec<NormalizedComprehension<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprAwait<'a> {
    value: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprYield<'a> {
    value: Option<Box<NormalizedExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprYieldFrom<'a> {
    value: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprCompare<'a> {
    left: Box<NormalizedExpr<'a>>,
    ops: Vec<NormalizedCmpOp>,
    comparators: Vec<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprCall<'a> {
    func: Box<NormalizedExpr<'a>>,
    arguments: NormalizedArguments<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprFormattedValue<'a> {
    value: Box<NormalizedExpr<'a>>,
    debug_text: Option<&'a ast::DebugText>,
    conversion: ast::ConversionFlag,
    format_spec: Option<Box<NormalizedExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprFString<'a> {
    values: Vec<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum NormalizedLiteral<'a> {
    None,
    Ellipsis,
    Bool(&'a bool),
    Str(String),
    Bytes(&'a [u8]),
    Number(NormalizedNumber<'a>),
}

impl<'a> From<ast::LiteralExpressionRef<'a>> for NormalizedLiteral<'a> {
    fn from(literal: ast::LiteralExpressionRef<'a>) -> Self {
        match literal {
            ast::LiteralExpressionRef::NoneLiteral(_) => Self::None,
            ast::LiteralExpressionRef::EllipsisLiteral(_) => Self::Ellipsis,
            ast::LiteralExpressionRef::BooleanLiteral(ast::ExprBooleanLiteral {
                value, ..
            }) => Self::Bool(value),
            ast::LiteralExpressionRef::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                Self::Str(normalize(value))
            }
            ast::LiteralExpressionRef::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => {
                Self::Bytes(value)
            }
            ast::LiteralExpressionRef::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => {
                Self::Number(value.into())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprStringLiteral {
    value: String,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprBytesLiteral<'a> {
    value: &'a [u8],
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprNumberLiteral<'a> {
    value: NormalizedNumber<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprBoolLiteral<'a> {
    value: &'a bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprAttribute<'a> {
    value: Box<NormalizedExpr<'a>>,
    attr: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprSubscript<'a> {
    value: Box<NormalizedExpr<'a>>,
    slice: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprStarred<'a> {
    value: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprName<'a> {
    id: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprList<'a> {
    elts: Vec<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprTuple<'a> {
    elts: Vec<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprSlice<'a> {
    lower: Option<Box<NormalizedExpr<'a>>>,
    upper: Option<Box<NormalizedExpr<'a>>>,
    step: Option<Box<NormalizedExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ExprIpyEscapeCommand<'a> {
    kind: ast::IpyEscapeKind,
    value: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum NormalizedExpr<'a> {
    BoolOp(ExprBoolOp<'a>),
    NamedExpr(ExprNamedExpr<'a>),
    BinOp(ExprBinOp<'a>),
    UnaryOp(ExprUnaryOp<'a>),
    Lambda(ExprLambda<'a>),
    IfExp(ExprIfExp<'a>),
    Dict(ExprDict<'a>),
    Set(ExprSet<'a>),
    ListComp(ExprListComp<'a>),
    SetComp(ExprSetComp<'a>),
    DictComp(ExprDictComp<'a>),
    GeneratorExp(ExprGeneratorExp<'a>),
    Await(ExprAwait<'a>),
    Yield(ExprYield<'a>),
    YieldFrom(ExprYieldFrom<'a>),
    Compare(ExprCompare<'a>),
    Call(ExprCall<'a>),
    NormalizedValue(ExprFormattedValue<'a>),
    FString(ExprFString<'a>),
    StringLiteral(ExprStringLiteral),
    BytesLiteral(ExprBytesLiteral<'a>),
    NumberLiteral(ExprNumberLiteral<'a>),
    BoolLiteral(ExprBoolLiteral<'a>),
    NoneLiteral,
    EllispsisLiteral,
    Attribute(ExprAttribute<'a>),
    Subscript(ExprSubscript<'a>),
    Starred(ExprStarred<'a>),
    Name(ExprName<'a>),
    List(ExprList<'a>),
    Tuple(ExprTuple<'a>),
    Slice(ExprSlice<'a>),
    IpyEscapeCommand(ExprIpyEscapeCommand<'a>),
}

impl<'a> From<&'a Box<ast::Expr>> for Box<NormalizedExpr<'a>> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        Box::new((expr.as_ref()).into())
    }
}

impl<'a> From<&'a Box<ast::Expr>> for NormalizedExpr<'a> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        (expr.as_ref()).into()
    }
}

impl<'a> From<&'a ast::Expr> for NormalizedExpr<'a> {
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
            ast::Expr::NamedExpr(ast::ExprNamedExpr {
                target,
                value,
                range: _,
            }) => Self::NamedExpr(ExprNamedExpr {
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
            ast::Expr::IfExp(ast::ExprIfExp {
                test,
                body,
                orelse,
                range: _,
            }) => Self::IfExp(ExprIfExp {
                test: test.into(),
                body: body.into(),
                orelse: orelse.into(),
            }),
            ast::Expr::Dict(ast::ExprDict {
                keys,
                values,
                range: _,
            }) => Self::Dict(ExprDict {
                keys: keys
                    .iter()
                    .map(|expr| expr.as_ref().map(Into::into))
                    .collect(),
                values: values.iter().map(Into::into).collect(),
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
            ast::Expr::GeneratorExp(ast::ExprGeneratorExp {
                elt,
                generators,
                range: _,
            }) => Self::GeneratorExp(ExprGeneratorExp {
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
            ast::Expr::FormattedValue(ast::ExprFormattedValue {
                value,
                conversion,
                debug_text,
                format_spec,
                range: _,
            }) => Self::NormalizedValue(ExprFormattedValue {
                value: value.into(),
                conversion: *conversion,
                debug_text: debug_text.as_ref(),
                format_spec: format_spec.as_ref().map(Into::into),
            }),
            ast::Expr::FString(ast::ExprFString {
                values,
                implicit_concatenated: _,
                range: _,
            }) => Self::FString(ExprFString {
                values: values.iter().map(Into::into).collect(),
            }),
            ast::Expr::StringLiteral(ast::ExprStringLiteral {
                value,
                // Compare strings based on resolved value, not representation (i.e., ignore whether
                // the string was implicitly concatenated).
                implicit_concatenated: _,
                unicode: _,
                range: _,
            }) => Self::StringLiteral(ExprStringLiteral {
                value: normalize(value),
            }),
            ast::Expr::BytesLiteral(ast::ExprBytesLiteral {
                value,
                // Compare bytes based on resolved value, not representation (i.e., ignore whether
                // the bytes was implicitly concatenated).
                implicit_concatenated: _,
                range: _,
            }) => Self::BytesLiteral(ExprBytesLiteral { value }),
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral { value, range: _ }) => {
                Self::NumberLiteral(ExprNumberLiteral {
                    value: value.into(),
                })
            }
            ast::Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, range: _ }) => {
                Self::BoolLiteral(ExprBoolLiteral { value })
            }
            ast::Expr::NoneLiteral(_) => Self::NoneLiteral,
            ast::Expr::EllipsisLiteral(_) => Self::EllispsisLiteral,
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
            }) => Self::IpyEscapeCommand(ExprIpyEscapeCommand {
                kind: *kind,
                value: value.as_str(),
            }),
        }
    }
}

impl<'a> From<&'a ast::ExprName> for NormalizedExpr<'a> {
    fn from(expr: &'a ast::ExprName) -> Self {
        Self::Name(ExprName {
            id: expr.id.as_str(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtFunctionDef<'a> {
    is_async: bool,
    decorator_list: Vec<NormalizedDecorator<'a>>,
    name: &'a str,
    type_params: Option<NormalizedTypeParams<'a>>,
    parameters: NormalizedParameters<'a>,
    returns: Option<NormalizedExpr<'a>>,
    body: Vec<NormalizedStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtClassDef<'a> {
    decorator_list: Vec<NormalizedDecorator<'a>>,
    name: &'a str,
    type_params: Option<NormalizedTypeParams<'a>>,
    arguments: NormalizedArguments<'a>,
    body: Vec<NormalizedStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtReturn<'a> {
    value: Option<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtDelete<'a> {
    targets: Vec<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtTypeAlias<'a> {
    name: Box<NormalizedExpr<'a>>,
    type_params: Option<NormalizedTypeParams<'a>>,
    value: Box<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NormalizedTypeParams<'a> {
    type_params: Vec<NormalizedTypeParam<'a>>,
}

impl<'a> From<&'a ast::TypeParams> for NormalizedTypeParams<'a> {
    fn from(type_params: &'a ast::TypeParams) -> Self {
        Self {
            type_params: type_params.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<&'a Box<ast::TypeParams>> for NormalizedTypeParams<'a> {
    fn from(type_params: &'a Box<ast::TypeParams>) -> Self {
        type_params.as_ref().into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum NormalizedTypeParam<'a> {
    TypeVar(TypeParamTypeVar<'a>),
    ParamSpec(TypeParamParamSpec<'a>),
    TypeVarTuple(TypeParamTypeVarTuple<'a>),
}

impl<'a> From<&'a ast::TypeParam> for NormalizedTypeParam<'a> {
    fn from(type_param: &'a ast::TypeParam) -> Self {
        match type_param {
            ast::TypeParam::TypeVar(ast::TypeParamTypeVar {
                name,
                bound,
                range: _,
            }) => Self::TypeVar(TypeParamTypeVar {
                name: name.as_str(),
                bound: bound.as_ref().map(Into::into),
            }),
            ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple { name, range: _ }) => {
                Self::TypeVarTuple(TypeParamTypeVarTuple {
                    name: name.as_str(),
                })
            }
            ast::TypeParam::ParamSpec(ast::TypeParamParamSpec { name, range: _ }) => {
                Self::ParamSpec(TypeParamParamSpec {
                    name: name.as_str(),
                })
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct TypeParamTypeVar<'a> {
    name: &'a str,
    bound: Option<Box<NormalizedExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct TypeParamParamSpec<'a> {
    name: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct TypeParamTypeVarTuple<'a> {
    name: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAssign<'a> {
    targets: Vec<NormalizedExpr<'a>>,
    value: NormalizedExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAugAssign<'a> {
    target: NormalizedExpr<'a>,
    op: NormalizedOperator,
    value: NormalizedExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAnnAssign<'a> {
    target: NormalizedExpr<'a>,
    annotation: NormalizedExpr<'a>,
    value: Option<NormalizedExpr<'a>>,
    simple: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtFor<'a> {
    is_async: bool,
    target: NormalizedExpr<'a>,
    iter: NormalizedExpr<'a>,
    body: Vec<NormalizedStmt<'a>>,
    orelse: Vec<NormalizedStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtWhile<'a> {
    test: NormalizedExpr<'a>,
    body: Vec<NormalizedStmt<'a>>,
    orelse: Vec<NormalizedStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtIf<'a> {
    test: NormalizedExpr<'a>,
    body: Vec<NormalizedStmt<'a>>,
    elif_else_clauses: Vec<NormalizedElifElseClause<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtWith<'a> {
    is_async: bool,
    items: Vec<NormalizedWithItem<'a>>,
    body: Vec<NormalizedStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtMatch<'a> {
    subject: NormalizedExpr<'a>,
    cases: Vec<NormalizedMatchCase<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtRaise<'a> {
    exc: Option<NormalizedExpr<'a>>,
    cause: Option<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtTry<'a> {
    body: Vec<NormalizedStmt<'a>>,
    handlers: Vec<NormalizedExceptHandler<'a>>,
    orelse: Vec<NormalizedStmt<'a>>,
    finalbody: Vec<NormalizedStmt<'a>>,
    is_star: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAssert<'a> {
    test: NormalizedExpr<'a>,
    msg: Option<NormalizedExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtImport<'a> {
    names: Vec<NormalizedAlias<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtImportFrom<'a> {
    module: Option<&'a str>,
    names: Vec<NormalizedAlias<'a>>,
    level: Option<u32>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtGlobal<'a> {
    names: Vec<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtNonlocal<'a> {
    names: Vec<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtExpr<'a> {
    value: NormalizedExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtIpyEscapeCommand<'a> {
    kind: ast::IpyEscapeKind,
    value: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum NormalizedStmt<'a> {
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

impl<'a> From<&'a ast::Stmt> for NormalizedStmt<'a> {
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
                // Like Black, flatten all tuples, as we may insert parentheses, which changes the
                // AST but not the semantics.
                targets: targets
                    .iter()
                    .flat_map(|target| {
                        if let ast::Expr::Tuple(tuple) = target {
                            Left(tuple.elts.iter())
                        } else {
                            Right(std::iter::once(target))
                        }
                    })
                    .map(Into::into)
                    .collect(),
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
            }) => Self::IpyEscapeCommand(StmtIpyEscapeCommand {
                kind: *kind,
                value: value.as_str(),
            }),
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
pub(crate) enum NormalizedMod<'a> {
    Module(NormalizedModModule<'a>),
    Expression(NormalizedModExpression<'a>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct NormalizedModModule<'a> {
    body: Vec<NormalizedStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct NormalizedModExpression<'a> {
    body: Box<NormalizedExpr<'a>>,
}

impl<'a> From<&'a ast::Mod> for NormalizedMod<'a> {
    fn from(mod_: &'a ast::Mod) -> Self {
        match mod_ {
            ast::Mod::Module(module) => Self::Module(module.into()),
            ast::Mod::Expression(expr) => Self::Expression(expr.into()),
        }
    }
}

impl<'a> From<&'a ast::ModModule> for NormalizedModModule<'a> {
    fn from(module: &'a ast::ModModule) -> Self {
        Self {
            body: module.body.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<&'a ast::ModExpression> for NormalizedModExpression<'a> {
    fn from(expr: &'a ast::ModExpression) -> Self {
        Self {
            body: (&expr.body).into(),
        }
    }
}

/// Normalize a string by (1) stripping any leading and trailing space from each line, and
/// (2) removing any blank lines from the start and end of the string.
fn normalize(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}
