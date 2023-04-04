//! An equivalent object hierarchy to the [`Expr`] hierarchy, but with the
//! ability to compare expressions for equality (via [`Eq`] and [`Hash`]).

use num_bigint::BigInt;
use rustpython_parser::ast::{
    Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, Excepthandler,
    ExcepthandlerKind, Expr, ExprContext, ExprKind, Keyword, MatchCase, Operator, Pattern,
    PatternKind, Stmt, StmtKind, Unaryop, Withitem,
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
            name: &alias.node.name,
            asname: alias.node.asname.as_deref(),
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
        match &pattern.node {
            PatternKind::MatchValue { value } => Self::MatchValue {
                value: value.into(),
            },
            PatternKind::MatchSingleton { value } => Self::MatchSingleton {
                value: value.into(),
            },
            PatternKind::MatchSequence { patterns } => Self::MatchSequence {
                patterns: patterns.iter().map(Into::into).collect(),
            },
            PatternKind::MatchMapping {
                keys,
                patterns,
                rest,
            } => Self::MatchMapping {
                keys: keys.iter().map(Into::into).collect(),
                patterns: patterns.iter().map(Into::into).collect(),
                rest: rest.as_deref(),
            },
            PatternKind::MatchClass {
                cls,
                patterns,
                kwd_attrs,
                kwd_patterns,
            } => Self::MatchClass {
                cls: cls.into(),
                patterns: patterns.iter().map(Into::into).collect(),
                kwd_attrs: kwd_attrs.iter().map(String::as_str).collect(),
                kwd_patterns: kwd_patterns.iter().map(Into::into).collect(),
            },
            PatternKind::MatchStar { name } => Self::MatchStar {
                name: name.as_deref(),
            },
            PatternKind::MatchAs { pattern, name } => Self::MatchAs {
                pattern: pattern.as_ref().map(Into::into),
                name: name.as_deref(),
            },
            PatternKind::MatchOr { patterns } => Self::MatchOr {
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
            arg: &arg.node.arg,
            annotation: arg.node.annotation.as_ref().map(Into::into),
            type_comment: arg.node.type_comment.as_deref(),
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
            arg: keyword.node.arg.as_deref(),
            value: (&keyword.node.value).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableComprehension<'a> {
    pub target: ComparableExpr<'a>,
    pub iter: ComparableExpr<'a>,
    pub ifs: Vec<ComparableExpr<'a>>,
    pub is_async: usize,
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
        let ExcepthandlerKind::ExceptHandler { type_, name, body } = &excepthandler.node;
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
        conversion: usize,
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
        match &expr.node {
            ExprKind::BoolOp { op, values } => Self::BoolOp {
                op: op.into(),
                values: values.iter().map(Into::into).collect(),
            },
            ExprKind::NamedExpr { target, value } => Self::NamedExpr {
                target: target.into(),
                value: value.into(),
            },
            ExprKind::BinOp { left, op, right } => Self::BinOp {
                left: left.into(),
                op: op.into(),
                right: right.into(),
            },
            ExprKind::UnaryOp { op, operand } => Self::UnaryOp {
                op: op.into(),
                operand: operand.into(),
            },
            ExprKind::Lambda { args, body } => Self::Lambda {
                args: (&**args).into(),
                body: body.into(),
            },
            ExprKind::IfExp { test, body, orelse } => Self::IfExp {
                test: test.into(),
                body: body.into(),
                orelse: orelse.into(),
            },
            ExprKind::Dict { keys, values } => Self::Dict {
                keys: keys
                    .iter()
                    .map(|expr| expr.as_ref().map(Into::into))
                    .collect(),
                values: values.iter().map(Into::into).collect(),
            },
            ExprKind::Set { elts } => Self::Set {
                elts: elts.iter().map(Into::into).collect(),
            },
            ExprKind::ListComp { elt, generators } => Self::ListComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            },
            ExprKind::SetComp { elt, generators } => Self::SetComp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            },
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => Self::DictComp {
                key: key.into(),
                value: value.into(),
                generators: generators.iter().map(Into::into).collect(),
            },
            ExprKind::GeneratorExp { elt, generators } => Self::GeneratorExp {
                elt: elt.into(),
                generators: generators.iter().map(Into::into).collect(),
            },
            ExprKind::Await { value } => Self::Await {
                value: value.into(),
            },
            ExprKind::Yield { value } => Self::Yield {
                value: value.as_ref().map(Into::into),
            },
            ExprKind::YieldFrom { value } => Self::YieldFrom {
                value: value.into(),
            },
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => Self::Compare {
                left: left.into(),
                ops: ops.iter().map(Into::into).collect(),
                comparators: comparators.iter().map(Into::into).collect(),
            },
            ExprKind::Call {
                func,
                args,
                keywords,
            } => Self::Call {
                func: func.into(),
                args: args.iter().map(Into::into).collect(),
                keywords: keywords.iter().map(Into::into).collect(),
            },
            ExprKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => Self::FormattedValue {
                value: value.into(),
                conversion: *conversion,
                format_spec: format_spec.as_ref().map(Into::into),
            },
            ExprKind::JoinedStr { values } => Self::JoinedStr {
                values: values.iter().map(Into::into).collect(),
            },
            ExprKind::Constant { value, kind } => Self::Constant {
                value: value.into(),
                kind: kind.as_ref().map(String::as_str),
            },
            ExprKind::Attribute { value, attr, ctx } => Self::Attribute {
                value: value.into(),
                attr,
                ctx: ctx.into(),
            },
            ExprKind::Subscript { value, slice, ctx } => Self::Subscript {
                value: value.into(),
                slice: slice.into(),
                ctx: ctx.into(),
            },
            ExprKind::Starred { value, ctx } => Self::Starred {
                value: value.into(),
                ctx: ctx.into(),
            },
            ExprKind::Name { id, ctx } => Self::Name {
                id,
                ctx: ctx.into(),
            },
            ExprKind::List { elts, ctx } => Self::List {
                elts: elts.iter().map(Into::into).collect(),
                ctx: ctx.into(),
            },
            ExprKind::Tuple { elts, ctx } => Self::Tuple {
                elts: elts.iter().map(Into::into).collect(),
                ctx: ctx.into(),
            },
            ExprKind::Slice { lower, upper, step } => Self::Slice {
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
        decorator_list: Vec<ComparableExpr<'a>>,
        returns: Option<ComparableExpr<'a>>,
        type_comment: Option<&'a str>,
    },
    AsyncFunctionDef {
        name: &'a str,
        args: ComparableArguments<'a>,
        body: Vec<ComparableStmt<'a>>,
        decorator_list: Vec<ComparableExpr<'a>>,
        returns: Option<ComparableExpr<'a>>,
        type_comment: Option<&'a str>,
    },
    ClassDef {
        name: &'a str,
        bases: Vec<ComparableExpr<'a>>,
        keywords: Vec<ComparableKeyword<'a>>,
        body: Vec<ComparableStmt<'a>>,
        decorator_list: Vec<ComparableExpr<'a>>,
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
        simple: usize,
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
        level: Option<usize>,
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
        match &stmt.node {
            StmtKind::FunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
            } => Self::FunctionDef {
                name,
                args: args.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.as_ref().map(Into::into),
                type_comment: type_comment.as_ref().map(std::string::String::as_str),
            },
            StmtKind::AsyncFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                type_comment,
            } => Self::AsyncFunctionDef {
                name,
                args: args.into(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
                returns: returns.as_ref().map(Into::into),
                type_comment: type_comment.as_ref().map(std::string::String::as_str),
            },
            StmtKind::ClassDef {
                name,
                bases,
                keywords,
                body,
                decorator_list,
            } => Self::ClassDef {
                name,
                bases: bases.iter().map(Into::into).collect(),
                keywords: keywords.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
                decorator_list: decorator_list.iter().map(Into::into).collect(),
            },
            StmtKind::Return { value } => Self::Return {
                value: value.as_ref().map(Into::into),
            },
            StmtKind::Delete { targets } => Self::Delete {
                targets: targets.iter().map(Into::into).collect(),
            },
            StmtKind::Assign {
                targets,
                value,
                type_comment,
            } => Self::Assign {
                targets: targets.iter().map(Into::into).collect(),
                value: value.into(),
                type_comment: type_comment.as_ref().map(std::string::String::as_str),
            },
            StmtKind::AugAssign { target, op, value } => Self::AugAssign {
                target: target.into(),
                op: op.into(),
                value: value.into(),
            },
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
                simple,
            } => Self::AnnAssign {
                target: target.into(),
                annotation: annotation.into(),
                value: value.as_ref().map(Into::into),
                simple: *simple,
            },
            StmtKind::For {
                target,
                iter,
                body,
                orelse,
                type_comment,
            } => Self::For {
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            StmtKind::AsyncFor {
                target,
                iter,
                body,
                orelse,
                type_comment,
            } => Self::AsyncFor {
                target: target.into(),
                iter: iter.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            StmtKind::While { test, body, orelse } => Self::While {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            },
            StmtKind::If { test, body, orelse } => Self::If {
                test: test.into(),
                body: body.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
            },
            StmtKind::With {
                items,
                body,
                type_comment,
            } => Self::With {
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            StmtKind::AsyncWith {
                items,
                body,
                type_comment,
            } => Self::AsyncWith {
                items: items.iter().map(Into::into).collect(),
                body: body.iter().map(Into::into).collect(),
                type_comment: type_comment.as_ref().map(String::as_str),
            },
            StmtKind::Match { subject, cases } => Self::Match {
                subject: subject.into(),
                cases: cases.iter().map(Into::into).collect(),
            },
            StmtKind::Raise { exc, cause } => Self::Raise {
                exc: exc.as_ref().map(Into::into),
                cause: cause.as_ref().map(Into::into),
            },
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => Self::Try {
                body: body.iter().map(Into::into).collect(),
                handlers: handlers.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                finalbody: finalbody.iter().map(Into::into).collect(),
            },
            StmtKind::TryStar {
                body,
                handlers,
                orelse,
                finalbody,
            } => Self::TryStar {
                body: body.iter().map(Into::into).collect(),
                handlers: handlers.iter().map(Into::into).collect(),
                orelse: orelse.iter().map(Into::into).collect(),
                finalbody: finalbody.iter().map(Into::into).collect(),
            },
            StmtKind::Assert { test, msg } => Self::Assert {
                test: test.into(),
                msg: msg.as_ref().map(Into::into),
            },
            StmtKind::Import { names } => Self::Import {
                names: names.iter().map(Into::into).collect(),
            },
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => Self::ImportFrom {
                module: module.as_ref().map(String::as_str),
                names: names.iter().map(Into::into).collect(),
                level: *level,
            },
            StmtKind::Global { names } => Self::Global {
                names: names.iter().map(String::as_str).collect(),
            },
            StmtKind::Nonlocal { names } => Self::Nonlocal {
                names: names.iter().map(String::as_str).collect(),
            },
            StmtKind::Expr { value } => Self::Expr {
                value: value.into(),
            },
            StmtKind::Pass => Self::Pass,
            StmtKind::Break => Self::Break,
            StmtKind::Continue => Self::Continue,
        }
    }
}
