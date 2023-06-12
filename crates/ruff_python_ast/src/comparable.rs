//! An equivalent object hierarchy to the [`Expr`] hierarchy, but with the
//! ability to compare expressions for equality (via [`Eq`] and [`Hash`]).

use num_bigint::BigInt;
use rustpython_ast::Decorator;
use rustpython_parser::ast::{
    self, Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, ConversionFlag,
    Excepthandler, Expr, ExprContext, Identifier, Int, Keyword, MatchCase, Operator, Pattern, Stmt,
    Unaryop, Withitem,
};

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum ComparableExprContext {
    Load,
    Store,
    Del,
}

impl From<&ExprContext> for ComparableExprContext {
    fn from(ctx: &ExprContext) -> Self {
        match ctx {
            ExprContext::Load => Self::Load,
            ExprContext::Store => Self::Store,
            ExprContext::Del => Self::Del,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum ComparableBoolop {
    And,
    Or,
}

impl From<&Boolop> for ComparableBoolop {
    fn from(op: &Boolop) -> Self {
        match op {
            Boolop::And => Self::And,
            Boolop::Or => Self::Or,
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

impl From<&Operator> for ComparableOperator {
    fn from(op: &Operator) -> Self {
        match op {
            Operator::Add => Self::Add,
            Operator::Sub => Self::Sub,
            Operator::Mult => Self::Mult,
            Operator::MatMult => Self::MatMult,
            Operator::Div => Self::Div,
            Operator::Mod => Self::Mod,
            Operator::Pow => Self::Pow,
            Operator::LShift => Self::LShift,
            Operator::RShift => Self::RShift,
            Operator::BitOr => Self::BitOr,
            Operator::BitXor => Self::BitXor,
            Operator::BitAnd => Self::BitAnd,
            Operator::FloorDiv => Self::FloorDiv,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum ComparableUnaryop {
    Invert,
    Not,
    UAdd,
    USub,
}

impl From<&Unaryop> for ComparableUnaryop {
    fn from(op: &Unaryop) -> Self {
        match op {
            Unaryop::Invert => Self::Invert,
            Unaryop::Not => Self::Not,
            Unaryop::UAdd => Self::UAdd,
            Unaryop::USub => Self::USub,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum ComparableCmpop {
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

impl From<&Cmpop> for ComparableCmpop {
    fn from(op: &Cmpop) -> Self {
        match op {
            Cmpop::Eq => Self::Eq,
            Cmpop::NotEq => Self::NotEq,
            Cmpop::Lt => Self::Lt,
            Cmpop::LtE => Self::LtE,
            Cmpop::Gt => Self::Gt,
            Cmpop::GtE => Self::GtE,
            Cmpop::Is => Self::Is,
            Cmpop::IsNot => Self::IsNot,
            Cmpop::In => Self::In,
            Cmpop::NotIn => Self::NotIn,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableAlias<'a> {
    pub name: &'a str,
    pub asname: Option<&'a str>,
}

impl<'a> From<&'a Alias> for ComparableAlias<'a> {
    fn from(alias: &'a Alias) -> Self {
        Self {
            name: alias.name.as_str(),
            asname: alias.asname.as_deref(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableWithitem<'a> {
    pub context_expr: ComparableExpr<'a>,
    pub optional_vars: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a Withitem> for ComparableWithitem<'a> {
    fn from(withitem: &'a Withitem) -> Self {
        Self {
            context_expr: (&withitem.context_expr).into(),
            optional_vars: withitem.optional_vars.as_ref().map(Into::into),
        }
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparablePattern<'a> {
    MatchValue {
        value: ComparableExpr<'a>,
    },
    MatchSingleton {
        value: ComparableConstant<'a>,
    },
    MatchSequence {
        patterns: Vec<ComparablePattern<'a>>,
    },
    MatchMapping {
        keys: Vec<ComparableExpr<'a>>,
        patterns: Vec<ComparablePattern<'a>>,
        rest: Option<&'a str>,
    },
    MatchClass {
        cls: ComparableExpr<'a>,
        patterns: Vec<ComparablePattern<'a>>,
        kwd_attrs: Vec<&'a str>,
        kwd_patterns: Vec<ComparablePattern<'a>>,
    },
    MatchStar {
        name: Option<&'a str>,
    },
    MatchAs {
        pattern: Option<Box<ComparablePattern<'a>>>,
        name: Option<&'a str>,
    },
    MatchOr {
        patterns: Vec<ComparablePattern<'a>>,
    },
}

impl<'a> From<&'a Pattern> for ComparablePattern<'a> {
    fn from(pattern: &'a Pattern) -> Self {
        match pattern {
            Pattern::MatchValue(ast::PatternMatchValue { value, .. }) => Self::MatchValue {
                value: value.into(),
            },
            Pattern::MatchSingleton(ast::PatternMatchSingleton { value, .. }) => {
                Self::MatchSingleton {
                    value: value.into(),
                }
            }
            Pattern::MatchSequence(ast::PatternMatchSequence { patterns, .. }) => {
                Self::MatchSequence {
                    patterns: patterns.iter().map(Into::into).collect(),
                }
            }
            Pattern::MatchMapping(ast::PatternMatchMapping {
                keys,
                patterns,
                rest,
                ..
            }) => Self::MatchMapping {
                keys: keys.iter().map(Into::into).collect(),
                patterns: patterns.iter().map(Into::into).collect(),
                rest: rest.as_deref(),
            },
            Pattern::MatchClass(ast::PatternMatchClass {
                cls,
                patterns,
                kwd_attrs,
                kwd_patterns,
                ..
            }) => Self::MatchClass {
                cls: cls.into(),
                patterns: patterns.iter().map(Into::into).collect(),
                kwd_attrs: kwd_attrs.iter().map(Identifier::as_str).collect(),
                kwd_patterns: kwd_patterns.iter().map(Into::into).collect(),
            },
            Pattern::MatchStar(ast::PatternMatchStar { name, .. }) => Self::MatchStar {
                name: name.as_deref(),
            },
            Pattern::MatchAs(ast::PatternMatchAs { pattern, name, .. }) => Self::MatchAs {
                pattern: pattern.as_ref().map(Into::into),
                name: name.as_deref(),
            },
            Pattern::MatchOr(ast::PatternMatchOr { patterns, .. }) => Self::MatchOr {
                patterns: patterns.iter().map(Into::into).collect(),
            },
        }
    }
}

impl<'a> From<&'a Box<Pattern>> for Box<ComparablePattern<'a>> {
    fn from(pattern: &'a Box<Pattern>) -> Self {
        Box::new((&**pattern).into())
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableMatchCase<'a> {
    pub pattern: ComparablePattern<'a>,
    pub guard: Option<ComparableExpr<'a>>,
    pub body: Vec<ComparableStmt<'a>>,
}

impl<'a> From<&'a MatchCase> for ComparableMatchCase<'a> {
    fn from(match_case: &'a MatchCase) -> Self {
        Self {
            pattern: (&match_case.pattern).into(),
            guard: match_case.guard.as_ref().map(Into::into),
            body: match_case.body.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableDecorator<'a> {
    pub expression: ComparableExpr<'a>,
}

impl<'a> From<&'a Decorator> for ComparableDecorator<'a> {
    fn from(decorator: &'a Decorator) -> Self {
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

impl<'a> From<&'a Constant> for ComparableConstant<'a> {
    fn from(constant: &'a Constant) -> Self {
        match constant {
            Constant::None => Self::None,
            Constant::Bool(value) => Self::Bool(value),
            Constant::Str(value) => Self::Str(value),
            Constant::Bytes(value) => Self::Bytes(value),
            Constant::Int(value) => Self::Int(value),
            Constant::Tuple(value) => Self::Tuple(value.iter().map(Into::into).collect()),
            Constant::Float(value) => Self::Float(value.to_bits()),
            Constant::Complex { real, imag } => Self::Complex {
                real: real.to_bits(),
                imag: imag.to_bits(),
            },
            Constant::Ellipsis => Self::Ellipsis,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableArguments<'a> {
    pub posonlyargs: Vec<ComparableArg<'a>>,
    pub args: Vec<ComparableArg<'a>>,
    pub vararg: Option<ComparableArg<'a>>,
    pub kwonlyargs: Vec<ComparableArg<'a>>,
    pub kw_defaults: Vec<ComparableExpr<'a>>,
    pub kwarg: Option<ComparableArg<'a>>,
    pub defaults: Vec<ComparableExpr<'a>>,
}

impl<'a> From<&'a Arguments> for ComparableArguments<'a> {
    fn from(arguments: &'a Arguments) -> Self {
        Self {
            posonlyargs: arguments.posonlyargs.iter().map(Into::into).collect(),
            args: arguments.args.iter().map(Into::into).collect(),
            vararg: arguments.vararg.as_ref().map(Into::into),
            kwonlyargs: arguments.kwonlyargs.iter().map(Into::into).collect(),
            kw_defaults: arguments.kw_defaults.iter().map(Into::into).collect(),
            kwarg: arguments.vararg.as_ref().map(Into::into),
            defaults: arguments.defaults.iter().map(Into::into).collect(),
        }
    }
}

impl<'a> From<&'a Box<Arguments>> for ComparableArguments<'a> {
    fn from(arguments: &'a Box<Arguments>) -> Self {
        (&**arguments).into()
    }
}

impl<'a> From<&'a Box<Arg>> for ComparableArg<'a> {
    fn from(arg: &'a Box<Arg>) -> Self {
        (&**arg).into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableArg<'a> {
    pub arg: &'a str,
    pub annotation: Option<Box<ComparableExpr<'a>>>,
    pub type_comment: Option<&'a str>,
}

impl<'a> From<&'a Arg> for ComparableArg<'a> {
    fn from(arg: &'a Arg) -> Self {
        Self {
            arg: arg.arg.as_str(),
            annotation: arg.annotation.as_ref().map(Into::into),
            type_comment: arg.type_comment.as_deref(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableKeyword<'a> {
    pub arg: Option<&'a str>,
    pub value: ComparableExpr<'a>,
}

impl<'a> From<&'a Keyword> for ComparableKeyword<'a> {
    fn from(keyword: &'a Keyword) -> Self {
        Self {
            arg: keyword.arg.as_ref().map(Identifier::as_str),
            value: (&keyword.value).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableComprehension<'a> {
    pub target: ComparableExpr<'a>,
    pub iter: ComparableExpr<'a>,
    pub ifs: Vec<ComparableExpr<'a>>,
    pub is_async: bool,
}

impl<'a> From<&'a Comprehension> for ComparableComprehension<'a> {
    fn from(comprehension: &'a Comprehension) -> Self {
        Self {
            target: (&comprehension.target).into(),
            iter: (&comprehension.iter).into(),
            ifs: comprehension.ifs.iter().map(Into::into).collect(),
            is_async: comprehension.is_async,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableExcepthandler<'a> {
    ExceptHandler {
        type_: Option<ComparableExpr<'a>>,
        name: Option<&'a str>,
        body: Vec<ComparableStmt<'a>>,
    },
}

impl<'a> From<&'a Excepthandler> for ComparableExcepthandler<'a> {
    fn from(excepthandler: &'a Excepthandler) -> Self {
        let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler {
            type_, name, body, ..
        }) = excepthandler;
        Self::ExceptHandler {
            type_: type_.as_ref().map(Into::into),
            name: name.as_deref(),
            body: body.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableExpr<'a> {
    BoolOp {
        op: ComparableBoolop,
        values: Vec<ComparableExpr<'a>>,
    },
    NamedExpr {
        target: Box<ComparableExpr<'a>>,
        value: Box<ComparableExpr<'a>>,
    },
    BinOp {
        left: Box<ComparableExpr<'a>>,
        op: ComparableOperator,
        right: Box<ComparableExpr<'a>>,
    },
    UnaryOp {
        op: ComparableUnaryop,
        operand: Box<ComparableExpr<'a>>,
    },
    Lambda {
        args: ComparableArguments<'a>,
        body: Box<ComparableExpr<'a>>,
    },
    IfExp {
        test: Box<ComparableExpr<'a>>,
        body: Box<ComparableExpr<'a>>,
        orelse: Box<ComparableExpr<'a>>,
    },
    Dict {
        keys: Vec<Option<ComparableExpr<'a>>>,
        values: Vec<ComparableExpr<'a>>,
    },
    Set {
        elts: Vec<ComparableExpr<'a>>,
    },
    ListComp {
        elt: Box<ComparableExpr<'a>>,
        generators: Vec<ComparableComprehension<'a>>,
    },
    SetComp {
        elt: Box<ComparableExpr<'a>>,
        generators: Vec<ComparableComprehension<'a>>,
    },
    DictComp {
        key: Box<ComparableExpr<'a>>,
        value: Box<ComparableExpr<'a>>,
        generators: Vec<ComparableComprehension<'a>>,
    },
    GeneratorExp {
        elt: Box<ComparableExpr<'a>>,
        generators: Vec<ComparableComprehension<'a>>,
    },
    Await {
        value: Box<ComparableExpr<'a>>,
    },
    Yield {
        value: Option<Box<ComparableExpr<'a>>>,
    },
    YieldFrom {
        value: Box<ComparableExpr<'a>>,
    },
    Compare {
        left: Box<ComparableExpr<'a>>,
        ops: Vec<ComparableCmpop>,
        comparators: Vec<ComparableExpr<'a>>,
    },
    Call {
        func: Box<ComparableExpr<'a>>,
        args: Vec<ComparableExpr<'a>>,
        keywords: Vec<ComparableKeyword<'a>>,
    },
    FormattedValue {
        value: Box<ComparableExpr<'a>>,
        conversion: ConversionFlag,
        format_spec: Option<Box<ComparableExpr<'a>>>,
    },
    JoinedStr {
        values: Vec<ComparableExpr<'a>>,
    },
    Constant {
        value: ComparableConstant<'a>,
        kind: Option<&'a str>,
    },
    Attribute {
        value: Box<ComparableExpr<'a>>,
        attr: &'a str,
        ctx: ComparableExprContext,
    },
    Subscript {
        value: Box<ComparableExpr<'a>>,
        slice: Box<ComparableExpr<'a>>,
        ctx: ComparableExprContext,
    },
    Starred {
        value: Box<ComparableExpr<'a>>,
        ctx: ComparableExprContext,
    },
    Name {
        id: &'a str,
        ctx: ComparableExprContext,
    },
    List {
        elts: Vec<ComparableExpr<'a>>,
        ctx: ComparableExprContext,
    },
    Tuple {
        elts: Vec<ComparableExpr<'a>>,
        ctx: ComparableExprContext,
    },
    Slice {
        lower: Option<Box<ComparableExpr<'a>>>,
        upper: Option<Box<ComparableExpr<'a>>>,
        step: Option<Box<ComparableExpr<'a>>>,
    },
}

impl<'a> From<&'a Box<Expr>> for Box<ComparableExpr<'a>> {
    fn from(expr: &'a Box<Expr>) -> Self {
        Box::new((&**expr).into())
    }
}

impl<'a> From<&'a Box<Expr>> for ComparableExpr<'a> {
    fn from(expr: &'a Box<Expr>) -> Self {
        (&**expr).into()
    }
}

impl<'a> From<&'a Expr> for ComparableExpr<'a> {
    fn from(expr: &'a Expr) -> Self {
        match expr {
            Expr::BoolOp(ast::ExprBoolOp {
                op,
                values,
                range: _range,
            }) => Self::BoolOp {
                op: op.into(),
                values: values.iter().map(Into::into).collect(),
            },
            Expr::NamedExpr(ast::ExprNamedExpr {
                target,
                value,
                range: _range,
            }) => Self::NamedExpr {
                target: target.into(),
                value: value.into(),
            },
            Expr::BinOp(ast::ExprBinOp {
                left,
                op,
                right,
                range: _range,
            }) => Self::BinOp {
                left: left.into(),
                op: op.into(),
                right: right.into(),
            },
            Expr::UnaryOp(ast::ExprUnaryOp {
                op,
                operand,
                range: _range,
            }) => Self::UnaryOp {
                op: op.into(),
                operand: operand.into(),
            },
            Expr::Lambda(ast::ExprLambda {
                args,
                body,
                range: _range,
            }) => Self::Lambda {
                args: (&**args).into(),
                body: body.into(),
            },
            Expr::IfExp(ast::ExprIfExp {
                test,
                body,
                orelse,
                range: _range,
            }) => Self::IfExp {
                test: test.into(),
                body: body.into(),
                orelse: orelse.into(),
            },
            Expr::Dict(ast::ExprDict {
                keys,
                values,
                range: _range,
            }) => Self::Dict {
                keys: keys
                    .iter()
                    .map(|expr| expr.as_ref().map(Into::into))
                    .collect(),
                values: values.iter().map(Into::into).collect(),
            },
            Expr::Set(ast::ExprSet {
                elts,
                range: _range,
            }) => Self::Set {
                elts: elts.iter().map(Into::into).collect(),
            },
            Expr::ListComp(ast::ExprListComp {
                elt,
                generators,
                range: _range,
            }) => Self::ListComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            },
            Expr::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range: _range,
            }) => Self::SetComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            },
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _range,
            }) => Self::DictComp {
                key: key.into(),
                value: value.into(),
                generators: generators.iter().map(Into::into).collect(),
            },
            Expr::GeneratorExp(ast::ExprGeneratorExp {
                elt,
                generators,
                range: _range,
            }) => Self::GeneratorExp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            },
            Expr::Await(ast::ExprAwait {
                value,
                range: _range,
            }) => Self::Await {
                value: value.into(),
            },
            Expr::Yield(ast::ExprYield {
                value,
                range: _range,
            }) => Self::Yield {
                value: value.as_ref().map(Into::into),
            },
            Expr::YieldFrom(ast::ExprYieldFrom {
                value,
                range: _range,
            }) => Self::YieldFrom {
                value: value.into(),
            },
            Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                range: _range,
            }) => Self::Compare {
                left: left.into(),
                ops: ops.iter().map(Into::into).collect(),
                comparators: comparators.iter().map(Into::into).collect(),
            },
            Expr::Call(ast::ExprCall {
                func,
                args,
                keywords,
                range: _range,
            }) => Self::Call {
                func: func.into(),
                args: args.iter().map(Into::into).collect(),
                keywords: keywords.iter().map(Into::into).collect(),
            },
            Expr::FormattedValue(ast::ExprFormattedValue {
                value,
                conversion,
                format_spec,
                range: _range,
            }) => Self::FormattedValue {
                value: value.into(),
                conversion: *conversion,
                format_spec: format_spec.as_ref().map(Into::into),
            },
            Expr::JoinedStr(ast::ExprJoinedStr {
                values,
                range: _range,
            }) => Self::JoinedStr {
                values: values.iter().map(Into::into).collect(),
            },
            Expr::Constant(ast::ExprConstant {
                value,
                kind,
                range: _range,
            }) => Self::Constant {
                value: value.into(),
                kind: kind.as_ref().map(String::as_str),
            },
            Expr::Attribute(ast::ExprAttribute {
                value,
                attr,
                ctx,
                range: _range,
            }) => Self::Attribute {
                value: value.into(),
                attr: attr.as_str(),
                ctx: ctx.into(),
            },
            Expr::Subscript(ast::ExprSubscript {
                value,
                slice,
                ctx,
                range: _range,
            }) => Self::Subscript {
                value: value.into(),
                slice: slice.into(),
                ctx: ctx.into(),
            },
            Expr::Starred(ast::ExprStarred {
                value,
                ctx,
                range: _range,
            }) => Self::Starred {
                value: value.into(),
                ctx: ctx.into(),
            },
            Expr::Name(ast::ExprName {
                id,
                ctx,
                range: _range,
            }) => Self::Name {
                id: id.as_str(),
                ctx: ctx.into(),
            },
            Expr::List(ast::ExprList {
                elts,
                ctx,
                range: _range,
            }) => Self::List {
                elts: elts.iter().map(Into::into).collect(),
                ctx: ctx.into(),
            },
            Expr::Tuple(ast::ExprTuple {
                elts,
                ctx,
                range: _range,
            }) => Self::Tuple {
                elts: elts.iter().map(Into::into).collect(),
                ctx: ctx.into(),
            },
            Expr::Slice(ast::ExprSlice {
                lower,
                upper,
                step,
                range: _range,
            }) => Self::Slice {
                lower: lower.as_ref().map(Into::into),
                upper: upper.as_ref().map(Into::into),
                step: step.as_ref().map(Into::into),
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ComparableStmt<'a> {
    FunctionDef {
        name: &'a str,
        args: ComparableArguments<'a>,
        body: Vec<ComparableStmt<'a>>,
        decorator_list: Vec<ComparableDecorator<'a>>,
        returns: Option<ComparableExpr<'a>>,
        type_comment: Option<&'a str>,
    },
    AsyncFunctionDef {
        name: &'a str,
        args: ComparableArguments<'a>,
        body: Vec<ComparableStmt<'a>>,
        decorator_list: Vec<ComparableDecorator<'a>>,
        returns: Option<ComparableExpr<'a>>,
        type_comment: Option<&'a str>,
    },
    ClassDef {
        name: &'a str,
        bases: Vec<ComparableExpr<'a>>,
        keywords: Vec<ComparableKeyword<'a>>,
        body: Vec<ComparableStmt<'a>>,
        decorator_list: Vec<ComparableDecorator<'a>>,
    },
    Return {
        value: Option<ComparableExpr<'a>>,
    },
    Delete {
        targets: Vec<ComparableExpr<'a>>,
    },
    Assign {
        targets: Vec<ComparableExpr<'a>>,
        value: ComparableExpr<'a>,
        type_comment: Option<&'a str>,
    },
    AugAssign {
        target: ComparableExpr<'a>,
        op: ComparableOperator,
        value: ComparableExpr<'a>,
    },
    AnnAssign {
        target: ComparableExpr<'a>,
        annotation: ComparableExpr<'a>,
        value: Option<ComparableExpr<'a>>,
        simple: bool,
    },
    For {
        target: ComparableExpr<'a>,
        iter: ComparableExpr<'a>,
        body: Vec<ComparableStmt<'a>>,
        orelse: Vec<ComparableStmt<'a>>,
        type_comment: Option<&'a str>,
    },
    AsyncFor {
        target: ComparableExpr<'a>,
        iter: ComparableExpr<'a>,
        body: Vec<ComparableStmt<'a>>,
        orelse: Vec<ComparableStmt<'a>>,
        type_comment: Option<&'a str>,
    },
    While {
        test: ComparableExpr<'a>,
        body: Vec<ComparableStmt<'a>>,
        orelse: Vec<ComparableStmt<'a>>,
    },
    If {
        test: ComparableExpr<'a>,
        body: Vec<ComparableStmt<'a>>,
        orelse: Vec<ComparableStmt<'a>>,
    },
    With {
        items: Vec<ComparableWithitem<'a>>,
        body: Vec<ComparableStmt<'a>>,
        type_comment: Option<&'a str>,
    },
    AsyncWith {
        items: Vec<ComparableWithitem<'a>>,
        body: Vec<ComparableStmt<'a>>,
        type_comment: Option<&'a str>,
    },
    Match {
        subject: ComparableExpr<'a>,
        cases: Vec<ComparableMatchCase<'a>>,
    },
    Raise {
        exc: Option<ComparableExpr<'a>>,
        cause: Option<ComparableExpr<'a>>,
    },
    Try {
        body: Vec<ComparableStmt<'a>>,
        handlers: Vec<ComparableExcepthandler<'a>>,
        orelse: Vec<ComparableStmt<'a>>,
        finalbody: Vec<ComparableStmt<'a>>,
    },
    TryStar {
        body: Vec<ComparableStmt<'a>>,
        handlers: Vec<ComparableExcepthandler<'a>>,
        orelse: Vec<ComparableStmt<'a>>,
        finalbody: Vec<ComparableStmt<'a>>,
    },
    Assert {
        test: ComparableExpr<'a>,
        msg: Option<ComparableExpr<'a>>,
    },
    Import {
        names: Vec<ComparableAlias<'a>>,
    },
    ImportFrom {
        module: Option<&'a str>,
        names: Vec<ComparableAlias<'a>>,
        level: Option<Int>,
    },
    Global {
        names: Vec<&'a str>,
    },
    Nonlocal {
        names: Vec<&'a str>,
    },
    Expr {
        value: ComparableExpr<'a>,
    },
    Pass,
    Break,
    Continue,
}

impl<'a> From<&'a Stmt> for ComparableStmt<'a> {
    fn from(stmt: &'a Stmt) -> Self {
        match stmt {
            Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
                range: _range,
            }) => Self::FunctionDef {
                name: name.as_str(),
                args: args.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.as_ref().map(Into::into),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
                range: _range,
            }) => Self::AsyncFunctionDef {
                name: name.as_str(),
                args: args.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.as_ref().map(Into::into),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            Stmt::ClassDef(ast::StmtClassDef {
                name,
                bases,
                keywords,
                body,
                decorator_list,
                range: _range,
            }) => Self::ClassDef {
                name: name.as_str(),
                bases: bases.iter().map(Into::into).collect(),
                keywords: keywords.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
            },
            Stmt::Return(ast::StmtReturn {
                value,
                range: _range,
            }) => Self::Return {
                value: value.as_ref().map(Into::into),
            },
            Stmt::Delete(ast::StmtDelete {
                targets,
                range: _range,
            }) => Self::Delete {
                targets: targets.iter().map(Into::into).collect(),
            },
            Stmt::Assign(ast::StmtAssign {
                targets,
                value,
                type_comment,
                range: _range,
            }) => Self::Assign {
                targets: targets.iter().map(Into::into).collect(),
                value: value.into(),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            Stmt::AugAssign(ast::StmtAugAssign {
                target,
                op,
                value,
                range: _range,
            }) => Self::AugAssign {
                target: target.into(),
                op: op.into(),
                value: value.into(),
            },
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                annotation,
                value,
                simple,
                range: _range,
            }) => Self::AnnAssign {
                target: target.into(),
                annotation: annotation.into(),
                value: value.as_ref().map(Into::into),
                simple: *simple,
            },
            Stmt::For(ast::StmtFor {
                target,
                iter,
                body,
                orelse,
                type_comment,
                range: _range,
            }) => Self::For {
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            Stmt::AsyncFor(ast::StmtAsyncFor {
                target,
                iter,
                body,
                orelse,
                type_comment,
                range: _range,
            }) => Self::AsyncFor {
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range: _range,
            }) => Self::While {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            },
            Stmt::If(ast::StmtIf {
                test,
                body,
                orelse,
                range: _range,
            }) => Self::If {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            },
            Stmt::With(ast::StmtWith {
                items,
                body,
                type_comment,
                range: _range,
            }) => Self::With {
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            Stmt::AsyncWith(ast::StmtAsyncWith {
                items,
                body,
                type_comment,
                range: _range,
            }) => Self::AsyncWith {
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            Stmt::Match(ast::StmtMatch {
                subject,
                cases,
                range: _range,
            }) => Self::Match {
                subject: subject.into(),
                cases: cases.iter().map(Into::into).collect(),
            },
            Stmt::Raise(ast::StmtRaise {
                exc,
                cause,
                range: _range,
            }) => Self::Raise {
                exc: exc.as_ref().map(Into::into),
                cause: cause.as_ref().map(Into::into),
            },
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                range: _range,
            }) => Self::Try {
                body: body.iter().map(Into::into).collect(),
                handlers: handlers.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                finalbody: finalbody.iter().map(Into::into).collect(),
            },
            Stmt::TryStar(ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                range: _range,
            }) => Self::TryStar {
                body: body.iter().map(Into::into).collect(),
                handlers: handlers.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                finalbody: finalbody.iter().map(Into::into).collect(),
            },
            Stmt::Assert(ast::StmtAssert {
                test,
                msg,
                range: _range,
            }) => Self::Assert {
                test: test.into(),
                msg: msg.as_ref().map(Into::into),
            },
            Stmt::Import(ast::StmtImport {
                names,
                range: _range,
            }) => Self::Import {
                names: names.iter().map(Into::into).collect(),
            },
            Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                range: _range,
            }) => Self::ImportFrom {
                module: module.as_deref(),
                names: names.iter().map(Into::into).collect(),
                level: *level,
            },
            Stmt::Global(ast::StmtGlobal {
                names,
                range: _range,
            }) => Self::Global {
                names: names.iter().map(Identifier::as_str).collect(),
            },
            Stmt::Nonlocal(ast::StmtNonlocal {
                names,
                range: _range,
            }) => Self::Nonlocal {
                names: names.iter().map(Identifier::as_str).collect(),
            },
            Stmt::Expr(ast::StmtExpr {
                value,
                range: _range,
            }) => Self::Expr {
                value: value.into(),
            },
            Stmt::Pass(_) => Self::Pass,
            Stmt::Break(_) => Self::Break,
            Stmt::Continue(_) => Self::Continue,
        }
    }
}
