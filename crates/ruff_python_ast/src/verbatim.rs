//! An equivalent object hierarchy to the `RustPython` AST hierarchy, but with an additional
//! `Verbatim` node type that represents a verbatim string of Python code. Used to generate
//! source code from the AST.

use num_bigint::BigInt;
use ruff_text_size::TextRange;
use rustpython_ast::Ranged;
use rustpython_parser::ast;

#[derive(Debug, Copy, Clone)]
pub enum Boolop {
    And,
    Or,
}

impl From<ast::Boolop> for Boolop {
    fn from(op: ast::Boolop) -> Self {
        match op {
            ast::Boolop::And => Self::And,
            ast::Boolop::Or => Self::Or,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Operator {
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

impl From<ast::Operator> for Operator {
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

#[derive(Debug, Copy, Clone)]
pub enum Unaryop {
    Invert,
    Not,
    UAdd,
    USub,
}

impl From<ast::Unaryop> for Unaryop {
    fn from(op: ast::Unaryop) -> Self {
        match op {
            ast::Unaryop::Invert => Self::Invert,
            ast::Unaryop::Not => Self::Not,
            ast::Unaryop::UAdd => Self::UAdd,
            ast::Unaryop::USub => Self::USub,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Cmpop {
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

impl From<ast::Cmpop> for Cmpop {
    fn from(op: ast::Cmpop) -> Self {
        match op {
            ast::Cmpop::Eq => Self::Eq,
            ast::Cmpop::NotEq => Self::NotEq,
            ast::Cmpop::Lt => Self::Lt,
            ast::Cmpop::LtE => Self::LtE,
            ast::Cmpop::Gt => Self::Gt,
            ast::Cmpop::GtE => Self::GtE,
            ast::Cmpop::Is => Self::Is,
            ast::Cmpop::IsNot => Self::IsNot,
            ast::Cmpop::In => Self::In,
            ast::Cmpop::NotIn => Self::NotIn,
        }
    }
}

#[derive(Debug)]
pub struct Alias<'a> {
    pub name: &'a str,
    pub asname: Option<&'a str>,
}

impl<'a> From<&'a ast::Alias> for Alias<'a> {
    fn from(alias: &'a ast::Alias) -> Self {
        Self {
            name: alias.name.as_str(),
            asname: alias.asname.as_deref(),
        }
    }
}

#[derive(Debug)]
pub struct Withitem<'a> {
    pub context_expr: Expr<'a>,
    pub optional_vars: Option<Expr<'a>>,
}

impl<'a> From<&'a ast::Withitem> for Withitem<'a> {
    fn from(withitem: &'a ast::Withitem) -> Self {
        Self {
            context_expr: (&withitem.context_expr).into(),
            optional_vars: withitem.optional_vars.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug)]
pub struct PatternMatchValue<'a> {
    pub value: Expr<'a>,
}

#[derive(Debug)]
pub struct PatternMatchSingleton<'a> {
    pub value: Constant<'a>,
}

#[derive(Debug)]
pub struct PatternMatchSequence<'a> {
    pub patterns: Vec<Pattern<'a>>,
}

#[derive(Debug)]
pub struct PatternMatchMapping<'a> {
    pub keys: Vec<Expr<'a>>,
    pub patterns: Vec<Pattern<'a>>,
    pub rest: Option<&'a str>,
}

#[derive(Debug)]
pub struct PatternMatchClass<'a> {
    pub cls: Expr<'a>,
    pub patterns: Vec<Pattern<'a>>,
    pub kwd_attrs: Vec<&'a str>,
    pub kwd_patterns: Vec<Pattern<'a>>,
}

#[derive(Debug)]
pub struct PatternMatchStar<'a> {
    pub name: Option<&'a str>,
}

#[derive(Debug)]
pub struct PatternMatchAs<'a> {
    pub pattern: Option<Box<Pattern<'a>>>,
    pub name: Option<&'a str>,
}

#[derive(Debug)]
pub struct PatternMatchOr<'a> {
    pub patterns: Vec<Pattern<'a>>,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
pub enum Pattern<'a> {
    MatchValue(PatternMatchValue<'a>),
    MatchSingleton(PatternMatchSingleton<'a>),
    MatchSequence(PatternMatchSequence<'a>),
    MatchMapping(PatternMatchMapping<'a>),
    MatchClass(PatternMatchClass<'a>),
    MatchStar(PatternMatchStar<'a>),
    MatchAs(PatternMatchAs<'a>),
    MatchOr(PatternMatchOr<'a>),
}

impl<'a> From<&'a ast::Pattern> for Pattern<'a> {
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

impl<'a> From<&'a Box<ast::Pattern>> for Box<Pattern<'a>> {
    fn from(pattern: &'a Box<ast::Pattern>) -> Self {
        Box::new((&**pattern).into())
    }
}

#[derive(Debug)]
pub struct MatchCase<'a> {
    pub pattern: Pattern<'a>,
    pub guard: Option<Expr<'a>>,
    pub body: Vec<Stmt<'a>>,
}

impl<'a> From<&'a ast::MatchCase> for MatchCase<'a> {
    fn from(match_case: &'a ast::MatchCase) -> Self {
        Self {
            pattern: (&match_case.pattern).into(),
            guard: match_case.guard.as_ref().map(Into::into),
            body: match_case.body.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug)]
pub struct Decorator<'a> {
    pub expression: Expr<'a>,
}

impl<'a> From<&'a ast::Decorator> for Decorator<'a> {
    fn from(decorator: &'a ast::Decorator) -> Self {
        Self {
            expression: (&decorator.expression).into(),
        }
    }
}

#[derive(Debug)]
pub enum Constant<'a> {
    None,
    Bool(&'a bool),
    Str(&'a str),
    Bytes(&'a [u8]),
    Int(&'a BigInt),
    Tuple(Vec<Constant<'a>>),
    Float(f64),
    Complex { real: f64, imag: f64 },
    Ellipsis,
}

impl<'a> From<&'a ast::Constant> for Constant<'a> {
    fn from(constant: &'a ast::Constant) -> Self {
        match constant {
            ast::Constant::None => Self::None,
            ast::Constant::Bool(value) => Self::Bool(value),
            ast::Constant::Str(value) => Self::Str(value),
            ast::Constant::Bytes(value) => Self::Bytes(value),
            ast::Constant::Int(value) => Self::Int(value),
            ast::Constant::Tuple(value) => Self::Tuple(value.iter().map(Into::into).collect()),
            ast::Constant::Float(value) => Self::Float(*value),
            ast::Constant::Complex { real, imag } => Self::Complex {
                real: *real,
                imag: *imag,
            },
            ast::Constant::Ellipsis => Self::Ellipsis,
        }
    }
}

#[derive(Debug)]
pub struct Arguments<'a> {
    pub posonlyargs: Vec<Arg<'a>>,
    pub args: Vec<Arg<'a>>,
    pub vararg: Option<Arg<'a>>,
    pub kwonlyargs: Vec<Arg<'a>>,
    pub kw_defaults: Vec<Expr<'a>>,
    pub kwarg: Option<Arg<'a>>,
    pub defaults: Vec<Expr<'a>>,
}

impl<'a> From<&'a ast::Arguments> for Arguments<'a> {
    fn from(arguments: &'a ast::Arguments) -> Self {
        Self {
            posonlyargs: arguments.posonlyargs.iter().map(Into::into).collect(),
            args: arguments.args.iter().map(Into::into).collect(),
            vararg: arguments.vararg.as_ref().map(Into::into),
            kwonlyargs: arguments.kwonlyargs.iter().map(Into::into).collect(),
            kw_defaults: arguments.kw_defaults.iter().map(Into::into).collect(),
            kwarg: arguments.kwarg.as_ref().map(Into::into),
            defaults: arguments.defaults.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<&'a Box<ast::Arguments>> for Arguments<'a> {
    fn from(arguments: &'a Box<ast::Arguments>) -> Self {
        (&**arguments).into()
    }
}

impl<'a> From<&'a Box<ast::Arg>> for Arg<'a> {
    fn from(arg: &'a Box<ast::Arg>) -> Self {
        (&**arg).into()
    }
}

#[derive(Debug)]
pub struct Arg<'a> {
    pub arg: &'a str,
    pub annotation: Option<Box<Expr<'a>>>,
}

impl<'a> From<&'a ast::Arg> for Arg<'a> {
    fn from(arg: &'a ast::Arg) -> Self {
        Self {
            arg: arg.arg.as_str(),
            annotation: arg.annotation.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug)]
pub struct Keyword<'a> {
    pub arg: Option<&'a str>,
    pub value: Expr<'a>,
}

impl<'a> From<&'a ast::Keyword> for Keyword<'a> {
    fn from(keyword: &'a ast::Keyword) -> Self {
        Self {
            arg: keyword.arg.as_ref().map(ast::Identifier::as_str),
            value: (&keyword.value).into(),
        }
    }
}

#[derive(Debug)]
pub struct Comprehension<'a> {
    pub target: Expr<'a>,
    pub iter: Expr<'a>,
    pub ifs: Vec<Expr<'a>>,
    pub is_async: bool,
}

impl<'a> From<&'a ast::Comprehension> for Comprehension<'a> {
    fn from(comprehension: &'a ast::Comprehension) -> Self {
        Self {
            target: (&comprehension.target).into(),
            iter: (&comprehension.iter).into(),
            ifs: comprehension.ifs.iter().map(Into::into).collect(),
            is_async: comprehension.is_async,
        }
    }
}

#[derive(Debug)]
pub struct ExcepthandlerExceptHandler<'a> {
    pub type_: Option<Box<Expr<'a>>>,
    pub name: Option<&'a str>,
    pub body: Vec<Stmt<'a>>,
}

#[derive(Debug)]
pub enum Excepthandler<'a> {
    ExceptHandler(ExcepthandlerExceptHandler<'a>),
}

impl<'a> From<&'a ast::Excepthandler> for Excepthandler<'a> {
    fn from(excepthandler: &'a ast::Excepthandler) -> Self {
        let ast::Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler {
            type_,
            name,
            body,
            ..
        }) = excepthandler;
        Self::ExceptHandler(ExcepthandlerExceptHandler {
            type_: type_.as_ref().map(Into::into),
            name: name.as_deref(),
            body: body.iter().map(Into::into).collect(),
        })
    }
}

#[derive(Debug)]
pub struct ExprBoolOp<'a> {
    pub op: Boolop,
    pub values: Vec<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprNamedExpr<'a> {
    pub target: Box<Expr<'a>>,
    pub value: Box<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprBinOp<'a> {
    pub left: Box<Expr<'a>>,
    pub op: Operator,
    pub right: Box<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprUnaryOp<'a> {
    pub op: Unaryop,
    pub operand: Box<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprLambda<'a> {
    pub args: Arguments<'a>,
    pub body: Box<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprIfExp<'a> {
    pub test: Box<Expr<'a>>,
    pub body: Box<Expr<'a>>,
    pub orelse: Box<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprDict<'a> {
    pub keys: Vec<Option<Expr<'a>>>,
    pub values: Vec<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprSet<'a> {
    pub elts: Vec<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprListComp<'a> {
    pub elt: Box<Expr<'a>>,
    pub generators: Vec<Comprehension<'a>>,
}

#[derive(Debug)]
pub struct ExprSetComp<'a> {
    pub elt: Box<Expr<'a>>,
    pub generators: Vec<Comprehension<'a>>,
}

#[derive(Debug)]
pub struct ExprDictComp<'a> {
    pub key: Box<Expr<'a>>,
    pub value: Box<Expr<'a>>,
    pub generators: Vec<Comprehension<'a>>,
}

#[derive(Debug)]
pub struct ExprGeneratorExp<'a> {
    pub elt: Box<Expr<'a>>,
    pub generators: Vec<Comprehension<'a>>,
}

#[derive(Debug)]
pub struct ExprAwait<'a> {
    pub value: Box<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprYield<'a> {
    pub value: Option<Box<Expr<'a>>>,
}

#[derive(Debug)]
pub struct ExprYieldFrom<'a> {
    pub value: Box<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprCompare<'a> {
    pub left: Box<Expr<'a>>,
    pub ops: Vec<Cmpop>,
    pub comparators: Vec<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprCall<'a> {
    pub func: Box<Expr<'a>>,
    pub args: Vec<Expr<'a>>,
    pub keywords: Vec<Keyword<'a>>,
}

#[derive(Debug)]
pub struct ExprFormattedValue<'a> {
    pub value: Box<Expr<'a>>,
    pub conversion: ast::ConversionFlag,
    pub format_spec: Option<Box<Expr<'a>>>,
}

#[derive(Debug)]
pub struct ExprJoinedStr<'a> {
    pub values: Vec<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprConstant<'a> {
    pub value: Constant<'a>,
    pub kind: Option<&'a str>,
}

#[derive(Debug)]
pub struct ExprAttribute<'a> {
    pub value: Box<Expr<'a>>,
    pub attr: &'a str,
}

#[derive(Debug)]
pub struct ExprSubscript<'a> {
    pub value: Box<Expr<'a>>,
    pub slice: Box<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprStarred<'a> {
    pub value: Box<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprName<'a> {
    pub id: &'a str,
}

#[derive(Debug)]
pub struct ExprList<'a> {
    pub elts: Vec<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprTuple<'a> {
    pub elts: Vec<Expr<'a>>,
}

#[derive(Debug)]
pub struct ExprSlice<'a> {
    pub lower: Option<Box<Expr<'a>>>,
    pub upper: Option<Box<Expr<'a>>>,
    pub step: Option<Box<Expr<'a>>>,
}

#[derive(Debug)]
pub struct ExprVerbatim {
    pub range: TextRange,
}

#[derive(Debug)]
pub enum Expr<'a> {
    Verbatim(ExprVerbatim),
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

impl<'a> Expr<'a> {
    pub fn verbatim(expr: &ast::Expr) -> Self {
        Self::Verbatim(ExprVerbatim {
            range: expr.range(),
        })
    }
}

impl<'a> From<&'a Box<ast::Expr>> for Box<Expr<'a>> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        Box::new((&**expr).into())
    }
}

impl<'a> From<&'a Box<ast::Expr>> for Expr<'a> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        (&**expr).into()
    }
}

impl<'a> From<&'a ast::Expr> for Expr<'a> {
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
                ctx: _,
                range: _range,
            }) => Self::Attribute(ExprAttribute {
                value: value.into(),
                attr: attr.as_str(),
            }),
            ast::Expr::Subscript(ast::ExprSubscript {
                value,
                slice,
                ctx: _,
                range: _range,
            }) => Self::Subscript(ExprSubscript {
                value: value.into(),
                slice: slice.into(),
            }),
            ast::Expr::Starred(ast::ExprStarred {
                value,
                ctx: _,
                range: _range,
            }) => Self::Starred(ExprStarred {
                value: value.into(),
            }),
            ast::Expr::Name(ast::ExprName {
                id,
                ctx: _,
                range: _range,
            }) => Self::Name(ExprName { id: id.as_str() }),
            ast::Expr::List(ast::ExprList {
                elts,
                ctx: _,
                range: _range,
            }) => Self::List(ExprList {
                elts: elts.iter().map(Into::into).collect(),
            }),
            ast::Expr::Tuple(ast::ExprTuple {
                elts,
                ctx: _,
                range: _range,
            }) => Self::Tuple(ExprTuple {
                elts: elts.iter().map(Into::into).collect(),
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

#[derive(Debug)]
pub struct StmtFunctionDef<'a> {
    pub name: &'a str,
    pub args: Arguments<'a>,
    pub body: Vec<Stmt<'a>>,
    pub decorator_list: Vec<Decorator<'a>>,
    pub returns: Option<Expr<'a>>,
}

#[derive(Debug)]
pub struct StmtAsyncFunctionDef<'a> {
    pub name: &'a str,
    pub args: Arguments<'a>,
    pub body: Vec<Stmt<'a>>,
    pub decorator_list: Vec<Decorator<'a>>,
    pub returns: Option<Expr<'a>>,
}

#[derive(Debug)]
pub struct StmtClassDef<'a> {
    pub name: &'a str,
    pub bases: Vec<Expr<'a>>,
    pub keywords: Vec<Keyword<'a>>,
    pub body: Vec<Stmt<'a>>,
    pub decorator_list: Vec<Decorator<'a>>,
}

#[derive(Debug)]
pub struct StmtReturn<'a> {
    pub value: Option<Expr<'a>>,
}

#[derive(Debug)]
pub struct StmtDelete<'a> {
    pub targets: Vec<Expr<'a>>,
}

#[derive(Debug)]
pub struct StmtAssign<'a> {
    pub targets: Vec<Expr<'a>>,
    pub value: Expr<'a>,
}

#[derive(Debug)]
pub struct StmtAugAssign<'a> {
    pub target: Expr<'a>,
    pub op: Operator,
    pub value: Expr<'a>,
}

#[derive(Debug)]
pub struct StmtAnnAssign<'a> {
    pub target: Expr<'a>,
    pub annotation: Expr<'a>,
    pub value: Option<Expr<'a>>,
    pub simple: bool,
}

#[derive(Debug)]
pub struct StmtFor<'a> {
    pub target: Expr<'a>,
    pub iter: Expr<'a>,
    pub body: Vec<Stmt<'a>>,
    pub orelse: Vec<Stmt<'a>>,
}

#[derive(Debug)]
pub struct StmtAsyncFor<'a> {
    pub target: Expr<'a>,
    pub iter: Expr<'a>,
    pub body: Vec<Stmt<'a>>,
    pub orelse: Vec<Stmt<'a>>,
}

#[derive(Debug)]
pub struct StmtWhile<'a> {
    pub test: Expr<'a>,
    pub body: Vec<Stmt<'a>>,
    pub orelse: Vec<Stmt<'a>>,
}

#[derive(Debug)]
pub struct StmtIf<'a> {
    pub test: Expr<'a>,
    pub body: Vec<Stmt<'a>>,
    pub orelse: Vec<Stmt<'a>>,
}

#[derive(Debug)]
pub struct StmtWith<'a> {
    pub items: Vec<Withitem<'a>>,
    pub body: Vec<Stmt<'a>>,
}

#[derive(Debug)]
pub struct StmtAsyncWith<'a> {
    pub items: Vec<Withitem<'a>>,
    pub body: Vec<Stmt<'a>>,
}

#[derive(Debug)]
pub struct StmtMatch<'a> {
    pub subject: Expr<'a>,
    pub cases: Vec<MatchCase<'a>>,
}

#[derive(Debug)]
pub struct StmtRaise<'a> {
    pub exc: Option<Expr<'a>>,
    pub cause: Option<Expr<'a>>,
}

#[derive(Debug)]
pub struct StmtTry<'a> {
    pub body: Vec<Stmt<'a>>,
    pub handlers: Vec<Excepthandler<'a>>,
    pub orelse: Vec<Stmt<'a>>,
    pub finalbody: Vec<Stmt<'a>>,
}

#[derive(Debug)]
pub struct StmtTryStar<'a> {
    pub body: Vec<Stmt<'a>>,
    pub handlers: Vec<Excepthandler<'a>>,
    pub orelse: Vec<Stmt<'a>>,
    pub finalbody: Vec<Stmt<'a>>,
}

#[derive(Debug)]
pub struct StmtAssert<'a> {
    pub test: Expr<'a>,
    pub msg: Option<Expr<'a>>,
}

#[derive(Debug)]
pub struct StmtImport<'a> {
    pub names: Vec<Alias<'a>>,
}

#[derive(Debug)]
pub struct StmtImportFrom<'a> {
    pub module: Option<&'a str>,
    pub names: Vec<Alias<'a>>,
    pub level: Option<ast::Int>,
}

#[derive(Debug)]
pub struct StmtGlobal<'a> {
    pub names: Vec<&'a str>,
}

#[derive(Debug)]
pub struct StmtNonlocal<'a> {
    pub names: Vec<&'a str>,
}

#[derive(Debug)]
pub struct StmtExpr<'a> {
    pub value: Expr<'a>,
}

#[derive(Debug)]
pub struct StmtVerbatim {
    pub range: TextRange,
}

#[derive(Debug)]
pub enum Stmt<'a> {
    Verbatim(StmtVerbatim),
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

impl<'a> From<&'a ast::Stmt> for Stmt<'a> {
    fn from(stmt: &'a ast::Stmt) -> Self {
        match stmt {
            ast::Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment: _,
                range: _range,
            }) => Self::FunctionDef(StmtFunctionDef {
                name: name.as_str(),
                args: args.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.as_ref().map(Into::into),
            }),
            ast::Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment: _,
                range: _range,
            }) => Self::AsyncFunctionDef(StmtAsyncFunctionDef {
                name: name.as_str(),
                args: args.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.as_ref().map(Into::into),
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
                type_comment: _,
                range: _range,
            }) => Self::Assign(StmtAssign {
                targets: targets.iter().map(Into::into).collect(),
                value: value.into(),
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
                type_comment: _,
                range: _range,
            }) => Self::For(StmtFor {
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            }),
            ast::Stmt::AsyncFor(ast::StmtAsyncFor {
                target,
                iter,
                body,
                orelse,
                type_comment: _,
                range: _range,
            }) => Self::AsyncFor(StmtAsyncFor {
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
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
                type_comment: _,
                range: _range,
            }) => Self::With(StmtWith {
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
            }),
            ast::Stmt::AsyncWith(ast::StmtAsyncWith {
                items,
                body,
                type_comment: _,
                range: _range,
            }) => Self::AsyncWith(StmtAsyncWith {
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
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
