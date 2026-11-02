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

mod flat;

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

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableWithItem<'a> {
    context_expr: ComparableExpr<'a>,
    optional_vars: Option<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparablePatternArguments<'a> {
    patterns: Vec<ComparablePattern<'a>>,
    keywords: Vec<ComparablePatternKeyword<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparablePatternKeyword<'a> {
    attr: &'a str,
    pattern: ComparablePattern<'a>,
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
    items: Vec<ast::MappingPatternItem<ComparableExpr<'a>, ComparablePattern<'a>, &'a str>>,
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableMatchCase<'a> {
    pattern: ComparablePattern<'a>,
    guard: Option<ComparableExpr<'a>>,
    body: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableDecorator<'a> {
    expression: ComparableExpr<'a>,
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableParameters<'a> {
    posonlyargs: Vec<ComparableParameterWithDefault<'a>>,
    args: Vec<ComparableParameterWithDefault<'a>>,
    vararg: Option<ComparableParameter<'a>>,
    kwonlyargs: Vec<ComparableParameterWithDefault<'a>>,
    kwarg: Option<ComparableParameter<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableParameter<'a> {
    arg: &'a str,
    annotation: Option<Box<ComparableExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableParameterWithDefault<'a> {
    def: ComparableParameter<'a>,
    default: Option<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableKeyword<'a> {
    arg: Option<&'a str>,
    value: ComparableExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableComprehension<'a> {
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    ifs: Vec<ComparableExpr<'a>>,
    is_async: bool,
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableInterpolatedStringElement<'a> {
    Literal(Cow<'a, str>),
    InterpolatedElement(InterpolatedElement<'a>),
}

/// Comparable wrapper for [`ast::DebugText`].
///
/// Compares the full debug text (leading + expression source + trailing) rather than only the
/// expression source, because whitespace is part of the f-string's runtime output: `f"{x =}"`
/// produces `"x =<value>"` while `f"{x=}"` produces `"x=<value>"`, making them distinct
/// `Literal` types.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableDebugText<'a> {
    text: Cow<'a, str>,
}

impl<'a> From<&'a ast::DebugText> for ComparableDebugText<'a> {
    fn from(debug_text: &'a ast::DebugText) -> Self {
        // Normalizing newlines is safe because Python normalizes `\r\n` and `\r` to `\n`
        // at compile time, so they produce identical runtime values.
        Self {
            text: normalize_newlines(debug_text.as_str()),
        }
    }
}

fn normalize_newlines(contents: &str) -> Cow<'_, str> {
    if contents.contains('\r') {
        Cow::Owned(contents.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(contents)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct InterpolatedElement<'a> {
    expression: ComparableExpr<'a>,
    debug_text: Option<ComparableDebugText<'a>>,
    conversion: ast::ConversionFlag,
    format_spec: Option<Vec<ComparableInterpolatedStringElement<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableElifElseClause<'a> {
    test: Option<ComparableExpr<'a>>,
    body: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableFString<'a> {
    elements: Box<[ComparableInterpolatedStringElement<'a>]>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableTString<'a> {
    strings: Box<[ComparableInterpolatedStringElement<'a>]>,
    interpolations: Box<[InterpolatedElement<'a>]>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableStringLiteral<'a> {
    value: &'a str,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableBytesLiteral<'a> {
    value: Cow<'a, [u8]>,
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
    key: Option<Box<ComparableExpr<'a>>>,
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
    ops: Vec<ComparableCmpOp>,
    operands: Vec<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprCall<'a> {
    func: Box<ComparableExpr<'a>>,
    arguments: ComparableArguments<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprInterpolatedElement<'a> {
    value: Box<ComparableExpr<'a>>,
    debug_text: Option<ComparableDebugText<'a>>,
    conversion: ast::ConversionFlag,
    format_spec: Vec<ComparableInterpolatedStringElement<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprFString<'a> {
    value: ComparableFString<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprTString<'a> {
    value: ComparableTString<'a>,
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
    FStringExpressionElement(ExprInterpolatedElement<'a>),
    FString(ExprFString<'a>),
    TStringInterpolationElement(ExprInterpolatedElement<'a>),
    TString(ExprTString<'a>),
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableTypeParam<'a> {
    TypeVar(TypeParamTypeVar<'a>),
    ParamSpec(TypeParamParamSpec<'a>),
    TypeVarTuple(TypeParamTypeVarTuple<'a>),
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
    is_lazy: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtImportFrom<'a> {
    module: Option<&'a str>,
    names: Vec<ComparableAlias<'a>>,
    level: u32,
    is_lazy: bool,
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
