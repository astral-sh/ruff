//! An equivalent object hierarchy to the `RustPython` AST hierarchy, but with the
//! ability to compare expressions for equality (via [`Eq`] and [`Hash`]).

use num_bigint::BigInt;
use rustpython_parser::ast;

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum ComparableExprContext {
    Load,
    Store,
    Del,
}

impl From<&ast::ExprContext> for ComparableExprContext {
    fn from(ctx: &ast::ExprContext) -> Self {
        match ctx {
            ast::ExprContext::Load => Self::Load,
            ast::ExprContext::Store => Self::Store,
            ast::ExprContext::Del => Self::Del,
        }
    }
}

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
pub struct PatternMatchValue<'a> {
    value: ComparableExpr<'a>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct PatternMatchSingleton<'a> {
    value: ComparableConstant<'a>,
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
    patterns: Vec<ComparablePattern<'a>>,
    kwd_attrs: Vec<&'a str>,
    kwd_patterns: Vec<ComparablePattern<'a>>,
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
    MatchSingleton(PatternMatchSingleton<'a>),
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
            ast::Pattern::MatchClass(ast::PatternMatchClass {
                cls,
                patterns,
                kwd_attrs,
                kwd_patterns,
                ..
            }) => Self::MatchClass(PatternMatchClass {
                cls: cls.into(),
                patterns: patterns.iter().map(Into::into).collect(),
                kwd_attrs: kwd_attrs.iter().map(ast::Identifier::as_str).collect(),
                kwd_patterns: kwd_patterns.iter().map(Into::into).collect(),
            }),
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
        Box::new((&**pattern).into())
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
pub enum ComparableConstant<'a> {
    None,
    Bool(&'a bool),
    Str(&'a str),
    Bytes(&'a [u8]),
    Int(&'a BigInt),
    Tuple(Vec<ComparableConstant<'a>>),
    Float(u64),
    Complex { real: u64, imag: u64 },
    Ellipsis,
}

impl<'a> From<&'a ast::Constant> for ComparableConstant<'a> {
    fn from(constant: &'a ast::Constant) -> Self {
        match constant {
            ast::Constant::None => Self::None,
            ast::Constant::Bool(value) => Self::Bool(value),
            ast::Constant::Str(value) => Self::Str(value),
            ast::Constant::Bytes(value) => Self::Bytes(value),
            ast::Constant::Int(value) => Self::Int(value),
            ast::Constant::Tuple(value) => Self::Tuple(value.iter().map(Into::into).collect()),
            ast::Constant::Float(value) => Self::Float(value.to_bits()),
            ast::Constant::Complex { real, imag } => Self::Complex {
                real: real.to_bits(),
                imag: imag.to_bits(),
            },
            ast::Constant::Ellipsis => Self::Ellipsis,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableArguments<'a> {
    posonlyargs: Vec<ComparableArgWithDefault<'a>>,
    args: Vec<ComparableArgWithDefault<'a>>,
    vararg: Option<ComparableArg<'a>>,
    kwonlyargs: Vec<ComparableArgWithDefault<'a>>,
    kwarg: Option<ComparableArg<'a>>,
}

impl<'a> From<&'a ast::Arguments> for ComparableArguments<'a> {
    fn from(arguments: &'a ast::Arguments) -> Self {
        Self {
            posonlyargs: arguments.posonlyargs.iter().map(Into::into).collect(),
            args: arguments.args.iter().map(Into::into).collect(),
            vararg: arguments.vararg.as_ref().map(Into::into),
            kwonlyargs: arguments.kwonlyargs.iter().map(Into::into).collect(),
            kwarg: arguments.kwarg.as_ref().map(Into::into),
        }
    }
}

impl<'a> From<&'a Box<ast::Arguments>> for ComparableArguments<'a> {
    fn from(arguments: &'a Box<ast::Arguments>) -> Self {
        (&**arguments).into()
    }
}

impl<'a> From<&'a Box<ast::Arg>> for ComparableArg<'a> {
    fn from(arg: &'a Box<ast::Arg>) -> Self {
        (&**arg).into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableArg<'a> {
    arg: &'a str,
    annotation: Option<Box<ComparableExpr<'a>>>,
    type_comment: Option<&'a str>,
}

impl<'a> From<&'a ast::Arg> for ComparableArg<'a> {
    fn from(arg: &'a ast::Arg) -> Self {
        Self {
            arg: arg.arg.as_str(),
            annotation: arg.annotation.as_ref().map(Into::into),
            type_comment: arg.type_comment.as_deref(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableArgWithDefault<'a> {
    def: ComparableArg<'a>,
    default: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::ArgWithDefault> for ComparableArgWithDefault<'a> {
    fn from(arg: &'a ast::ArgWithDefault) -> Self {
        Self {
            def: (&arg.def).into(),
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
pub struct ExprBoolOp<'a> {
    op: ComparableBoolOp,
    values: Vec<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprNamedExpr<'a> {
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
    args: ComparableArguments<'a>,
    body: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprIfExp<'a> {
    test: Box<ComparableExpr<'a>>,
    body: Box<ComparableExpr<'a>>,
    orelse: Box<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprDict<'a> {
    keys: Vec<Option<ComparableExpr<'a>>>,
    values: Vec<ComparableExpr<'a>>,
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
pub struct ExprGeneratorExp<'a> {
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
    args: Vec<ComparableExpr<'a>>,
    keywords: Vec<ComparableKeyword<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprFormattedValue<'a> {
    value: Box<ComparableExpr<'a>>,
    conversion: ast::ConversionFlag,
    format_spec: Option<Box<ComparableExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprJoinedStr<'a> {
    values: Vec<ComparableExpr<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprConstant<'a> {
    value: ComparableConstant<'a>,
    kind: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprAttribute<'a> {
    value: Box<ComparableExpr<'a>>,
    attr: &'a str,
    ctx: ComparableExprContext,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSubscript<'a> {
    value: Box<ComparableExpr<'a>>,
    slice: Box<ComparableExpr<'a>>,
    ctx: ComparableExprContext,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprStarred<'a> {
    value: Box<ComparableExpr<'a>>,
    ctx: ComparableExprContext,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprName<'a> {
    id: &'a str,
    ctx: ComparableExprContext,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprList<'a> {
    elts: Vec<ComparableExpr<'a>>,
    ctx: ComparableExprContext,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprTuple<'a> {
    elts: Vec<ComparableExpr<'a>>,
    ctx: ComparableExprContext,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExprSlice<'a> {
    lower: Option<Box<ComparableExpr<'a>>>,
    upper: Option<Box<ComparableExpr<'a>>>,
    step: Option<Box<ComparableExpr<'a>>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableExpr<'a> {
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
    FormattedValue(ExprFormattedValue<'a>),
    JoinedStr(ExprJoinedStr<'a>),
    Constant(ExprConstant<'a>),
    Attribute(ExprAttribute<'a>),
    Subscript(ExprSubscript<'a>),
    Starred(ExprStarred<'a>),
    Name(ExprName<'a>),
    List(ExprList<'a>),
    Tuple(ExprTuple<'a>),
    Slice(ExprSlice<'a>),
}

impl<'a> From<&'a Box<ast::Expr>> for Box<ComparableExpr<'a>> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        Box::new((&**expr).into())
    }
}

impl<'a> From<&'a Box<ast::Expr>> for ComparableExpr<'a> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        (&**expr).into()
    }
}

impl<'a> From<&'a ast::Expr> for ComparableExpr<'a> {
    fn from(expr: &'a ast::Expr) -> Self {
        match expr {
            ast::Expr::BoolOp(ast::ExprBoolOp {
                op,
                values,
                range: _range,
            }) => Self::BoolOp(ExprBoolOp {
                op: (*op).into(),
                values: values.iter().map(Into::into).collect(),
            }),
            ast::Expr::NamedExpr(ast::ExprNamedExpr {
                target,
                value,
                range: _range,
            }) => Self::NamedExpr(ExprNamedExpr {
                target: target.into(),
                value: value.into(),
            }),
            ast::Expr::BinOp(ast::ExprBinOp {
                left,
                op,
                right,
                range: _range,
            }) => Self::BinOp(ExprBinOp {
                left: left.into(),
                op: (*op).into(),
                right: right.into(),
            }),
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op,
                operand,
                range: _range,
            }) => Self::UnaryOp(ExprUnaryOp {
                op: (*op).into(),
                operand: operand.into(),
            }),
            ast::Expr::Lambda(ast::ExprLambda {
                args,
                body,
                range: _range,
            }) => Self::Lambda(ExprLambda {
                args: (&**args).into(),
                body: body.into(),
            }),
            ast::Expr::IfExp(ast::ExprIfExp {
                test,
                body,
                orelse,
                range: _range,
            }) => Self::IfExp(ExprIfExp {
                test: test.into(),
                body: body.into(),
                orelse: orelse.into(),
            }),
            ast::Expr::Dict(ast::ExprDict {
                keys,
                values,
                range: _range,
            }) => Self::Dict(ExprDict {
                keys: keys
                    .iter()
                    .map(|expr| expr.as_ref().map(Into::into))
                    .collect(),
                values: values.iter().map(Into::into).collect(),
            }),
            ast::Expr::Set(ast::ExprSet {
                elts,
                range: _range,
            }) => Self::Set(ExprSet {
                elts: elts.iter().map(Into::into).collect(),
            }),
            ast::Expr::ListComp(ast::ExprListComp {
                elt,
                generators,
                range: _range,
            }) => Self::ListComp(ExprListComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::Expr::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range: _range,
            }) => Self::SetComp(ExprSetComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _range,
            }) => Self::DictComp(ExprDictComp {
                key: key.into(),
                value: value.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::Expr::GeneratorExp(ast::ExprGeneratorExp {
                elt,
                generators,
                range: _range,
            }) => Self::GeneratorExp(ExprGeneratorExp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            }),
            ast::Expr::Await(ast::ExprAwait {
                value,
                range: _range,
            }) => Self::Await(ExprAwait {
                value: value.into(),
            }),
            ast::Expr::Yield(ast::ExprYield {
                value,
                range: _range,
            }) => Self::Yield(ExprYield {
                value: value.as_ref().map(Into::into),
            }),
            ast::Expr::YieldFrom(ast::ExprYieldFrom {
                value,
                range: _range,
            }) => Self::YieldFrom(ExprYieldFrom {
                value: value.into(),
            }),
            ast::Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                range: _range,
            }) => Self::Compare(ExprCompare {
                left: left.into(),
                ops: ops.iter().copied().map(Into::into).collect(),
                comparators: comparators.iter().map(Into::into).collect(),
            }),
            ast::Expr::Call(ast::ExprCall {
                func,
                args,
                keywords,
                range: _range,
            }) => Self::Call(ExprCall {
                func: func.into(),
                args: args.iter().map(Into::into).collect(),
                keywords: keywords.iter().map(Into::into).collect(),
            }),
            ast::Expr::FormattedValue(ast::ExprFormattedValue {
                value,
                conversion,
                format_spec,
                range: _range,
            }) => Self::FormattedValue(ExprFormattedValue {
                value: value.into(),
                conversion: *conversion,
                format_spec: format_spec.as_ref().map(Into::into),
            }),
            ast::Expr::JoinedStr(ast::ExprJoinedStr {
                values,
                range: _range,
            }) => Self::JoinedStr(ExprJoinedStr {
                values: values.iter().map(Into::into).collect(),
            }),
            ast::Expr::Constant(ast::ExprConstant {
                value,
                kind,
                range: _range,
            }) => Self::Constant(ExprConstant {
                value: value.into(),
                kind: kind.as_ref().map(String::as_str),
            }),
            ast::Expr::Attribute(ast::ExprAttribute {
                value,
                attr,
                ctx,
                range: _range,
            }) => Self::Attribute(ExprAttribute {
                value: value.into(),
                attr: attr.as_str(),
                ctx: ctx.into(),
            }),
            ast::Expr::Subscript(ast::ExprSubscript {
                value,
                slice,
                ctx,
                range: _range,
            }) => Self::Subscript(ExprSubscript {
                value: value.into(),
                slice: slice.into(),
                ctx: ctx.into(),
            }),
            ast::Expr::Starred(ast::ExprStarred {
                value,
                ctx,
                range: _range,
            }) => Self::Starred(ExprStarred {
                value: value.into(),
                ctx: ctx.into(),
            }),
            ast::Expr::Name(ast::ExprName {
                id,
                ctx,
                range: _range,
            }) => Self::Name(ExprName {
                id: id.as_str(),
                ctx: ctx.into(),
            }),
            ast::Expr::List(ast::ExprList {
                elts,
                ctx,
                range: _range,
            }) => Self::List(ExprList {
                elts: elts.iter().map(Into::into).collect(),
                ctx: ctx.into(),
            }),
            ast::Expr::Tuple(ast::ExprTuple {
                elts,
                ctx,
                range: _range,
            }) => Self::Tuple(ExprTuple {
                elts: elts.iter().map(Into::into).collect(),
                ctx: ctx.into(),
            }),
            ast::Expr::Slice(ast::ExprSlice {
                lower,
                upper,
                step,
                range: _range,
            }) => Self::Slice(ExprSlice {
                lower: lower.as_ref().map(Into::into),
                upper: upper.as_ref().map(Into::into),
                step: step.as_ref().map(Into::into),
            }),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtFunctionDef<'a> {
    name: &'a str,
    args: ComparableArguments<'a>,
    body: Vec<ComparableStmt<'a>>,
    decorator_list: Vec<ComparableDecorator<'a>>,
    returns: Option<ComparableExpr<'a>>,
    type_comment: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAsyncFunctionDef<'a> {
    name: &'a str,
    args: ComparableArguments<'a>,
    body: Vec<ComparableStmt<'a>>,
    decorator_list: Vec<ComparableDecorator<'a>>,
    returns: Option<ComparableExpr<'a>>,
    type_comment: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtClassDef<'a> {
    name: &'a str,
    bases: Vec<ComparableExpr<'a>>,
    keywords: Vec<ComparableKeyword<'a>>,
    body: Vec<ComparableStmt<'a>>,
    decorator_list: Vec<ComparableDecorator<'a>>,
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
pub struct StmtAssign<'a> {
    targets: Vec<ComparableExpr<'a>>,
    value: ComparableExpr<'a>,
    type_comment: Option<&'a str>,
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
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    body: Vec<ComparableStmt<'a>>,
    orelse: Vec<ComparableStmt<'a>>,
    type_comment: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAsyncFor<'a> {
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    body: Vec<ComparableStmt<'a>>,
    orelse: Vec<ComparableStmt<'a>>,
    type_comment: Option<&'a str>,
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
    orelse: Vec<ComparableStmt<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtWith<'a> {
    items: Vec<ComparableWithItem<'a>>,
    body: Vec<ComparableStmt<'a>>,
    type_comment: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtAsyncWith<'a> {
    items: Vec<ComparableWithItem<'a>>,
    body: Vec<ComparableStmt<'a>>,
    type_comment: Option<&'a str>,
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
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StmtTryStar<'a> {
    body: Vec<ComparableStmt<'a>>,
    handlers: Vec<ComparableExceptHandler<'a>>,
    orelse: Vec<ComparableStmt<'a>>,
    finalbody: Vec<ComparableStmt<'a>>,
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
    level: Option<ast::Int>,
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
pub enum ComparableStmt<'a> {
    FunctionDef(StmtFunctionDef<'a>),
    AsyncFunctionDef(StmtAsyncFunctionDef<'a>),
    ClassDef(StmtClassDef<'a>),
    Return(StmtReturn<'a>),
    Delete(StmtDelete<'a>),
    Assign(StmtAssign<'a>),
    AugAssign(StmtAugAssign<'a>),
    AnnAssign(StmtAnnAssign<'a>),
    For(StmtFor<'a>),
    AsyncFor(StmtAsyncFor<'a>),
    While(StmtWhile<'a>),
    If(StmtIf<'a>),
    With(StmtWith<'a>),
    AsyncWith(StmtAsyncWith<'a>),
    Match(StmtMatch<'a>),
    Raise(StmtRaise<'a>),
    Try(StmtTry<'a>),
    TryStar(StmtTryStar<'a>),
    Assert(StmtAssert<'a>),
    Import(StmtImport<'a>),
    ImportFrom(StmtImportFrom<'a>),
    Global(StmtGlobal<'a>),
    Nonlocal(StmtNonlocal<'a>),
    Expr(StmtExpr<'a>),
    Pass,
    Break,
    Continue,
}

impl<'a> From<&'a ast::Stmt> for ComparableStmt<'a> {
    fn from(stmt: &'a ast::Stmt) -> Self {
        match stmt {
            ast::Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
                range: _range,
            }) => Self::FunctionDef(StmtFunctionDef {
                name: name.as_str(),
                args: args.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.as_ref().map(Into::into),
                type_comment: type_comment.as_ref().map(String::as_str),
            }),
            ast::Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
                range: _range,
            }) => Self::AsyncFunctionDef(StmtAsyncFunctionDef {
                name: name.as_str(),
                args: args.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.as_ref().map(Into::into),
                type_comment: type_comment.as_ref().map(String::as_str),
            }),
            ast::Stmt::ClassDef(ast::StmtClassDef {
                name,
                bases,
                keywords,
                body,
                decorator_list,
                range: _range,
            }) => Self::ClassDef(StmtClassDef {
                name: name.as_str(),
                bases: bases.iter().map(Into::into).collect(),
                keywords: keywords.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
            }),
            ast::Stmt::Return(ast::StmtReturn {
                value,
                range: _range,
            }) => Self::Return(StmtReturn {
                value: value.as_ref().map(Into::into),
            }),
            ast::Stmt::Delete(ast::StmtDelete {
                targets,
                range: _range,
            }) => Self::Delete(StmtDelete {
                targets: targets.iter().map(Into::into).collect(),
            }),
            ast::Stmt::Assign(ast::StmtAssign {
                targets,
                value,
                type_comment,
                range: _range,
            }) => Self::Assign(StmtAssign {
                targets: targets.iter().map(Into::into).collect(),
                value: value.into(),
                type_comment: type_comment.as_ref().map(String::as_str),
            }),
            ast::Stmt::AugAssign(ast::StmtAugAssign {
                target,
                op,
                value,
                range: _range,
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
                range: _range,
            }) => Self::AnnAssign(StmtAnnAssign {
                target: target.into(),
                annotation: annotation.into(),
                value: value.as_ref().map(Into::into),
                simple: *simple,
            }),
            ast::Stmt::For(ast::StmtFor {
                target,
                iter,
                body,
                orelse,
                type_comment,
                range: _range,
            }) => Self::For(StmtFor {
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            }),
            ast::Stmt::AsyncFor(ast::StmtAsyncFor {
                target,
                iter,
                body,
                orelse,
                type_comment,
                range: _range,
            }) => Self::AsyncFor(StmtAsyncFor {
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            }),
            ast::Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range: _range,
            }) => Self::While(StmtWhile {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            }),
            ast::Stmt::If(ast::StmtIf {
                test,
                body,
                orelse,
                range: _range,
            }) => Self::If(StmtIf {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            }),
            ast::Stmt::With(ast::StmtWith {
                items,
                body,
                type_comment,
                range: _range,
            }) => Self::With(StmtWith {
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            }),
            ast::Stmt::AsyncWith(ast::StmtAsyncWith {
                items,
                body,
                type_comment,
                range: _range,
            }) => Self::AsyncWith(StmtAsyncWith {
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            }),
            ast::Stmt::Match(ast::StmtMatch {
                subject,
                cases,
                range: _range,
            }) => Self::Match(StmtMatch {
                subject: subject.into(),
                cases: cases.iter().map(Into::into).collect(),
            }),
            ast::Stmt::Raise(ast::StmtRaise {
                exc,
                cause,
                range: _range,
            }) => Self::Raise(StmtRaise {
                exc: exc.as_ref().map(Into::into),
                cause: cause.as_ref().map(Into::into),
            }),
            ast::Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                range: _range,
            }) => Self::Try(StmtTry {
                body: body.iter().map(Into::into).collect(),
                handlers: handlers.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                finalbody: finalbody.iter().map(Into::into).collect(),
            }),
            ast::Stmt::TryStar(ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                range: _range,
            }) => Self::TryStar(StmtTryStar {
                body: body.iter().map(Into::into).collect(),
                handlers: handlers.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                finalbody: finalbody.iter().map(Into::into).collect(),
            }),
            ast::Stmt::Assert(ast::StmtAssert {
                test,
                msg,
                range: _range,
            }) => Self::Assert(StmtAssert {
                test: test.into(),
                msg: msg.as_ref().map(Into::into),
            }),
            ast::Stmt::Import(ast::StmtImport {
                names,
                range: _range,
            }) => Self::Import(StmtImport {
                names: names.iter().map(Into::into).collect(),
            }),
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                range: _range,
            }) => Self::ImportFrom(StmtImportFrom {
                module: module.as_deref(),
                names: names.iter().map(Into::into).collect(),
                level: *level,
            }),
            ast::Stmt::Global(ast::StmtGlobal {
                names,
                range: _range,
            }) => Self::Global(StmtGlobal {
                names: names.iter().map(ast::Identifier::as_str).collect(),
            }),
            ast::Stmt::Nonlocal(ast::StmtNonlocal {
                names,
                range: _range,
            }) => Self::Nonlocal(StmtNonlocal {
                names: names.iter().map(ast::Identifier::as_str).collect(),
            }),
            ast::Stmt::Expr(ast::StmtExpr {
                value,
                range: _range,
            }) => Self::Expr(StmtExpr {
                value: value.into(),
            }),
            ast::Stmt::Pass(_) => Self::Pass,
            ast::Stmt::Break(_) => Self::Break,
            ast::Stmt::Continue(_) => Self::Continue,
        }
    }
}
