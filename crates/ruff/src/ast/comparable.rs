//! An equivalent object hierarchy to the [`Expr`] hierarchy, but with the
//! ability to compare expressions for equality (via [`Eq`] and [`Hash`]).

use num_bigint::BigInt;
use rustpython_parser::ast::{
    Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, Expr, ExprContext, ExprKind, Keyword,
    Operator, Unaryop,
};

#[derive(Debug, PartialEq, Eq, Hash)]
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

#[derive(Debug, PartialEq, Eq, Hash)]
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

#[derive(Debug, PartialEq, Eq, Hash)]
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

#[derive(Debug, PartialEq, Eq, Hash)]
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

#[derive(Debug, PartialEq, Eq, Hash)]
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
            Constant::Tuple(value) => {
                Self::Tuple(value.iter().map(std::convert::Into::into).collect())
            }
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
            posonlyargs: arguments
                .posonlyargs
                .iter()
                .map(std::convert::Into::into)
                .collect(),
            args: arguments
                .args
                .iter()
                .map(std::convert::Into::into)
                .collect(),
            vararg: arguments.vararg.as_ref().map(std::convert::Into::into),
            kwonlyargs: arguments
                .kwonlyargs
                .iter()
                .map(std::convert::Into::into)
                .collect(),
            kw_defaults: arguments
                .kw_defaults
                .iter()
                .map(std::convert::Into::into)
                .collect(),
            kwarg: arguments.vararg.as_ref().map(std::convert::Into::into),
            defaults: arguments
                .defaults
                .iter()
                .map(std::convert::Into::into)
                .collect(),
        }
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
            annotation: arg.node.annotation.as_ref().map(std::convert::Into::into),
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
    pub is_async: &'a usize,
}

impl<'a> From<&'a Comprehension> for ComparableComprehension<'a> {
    fn from(comprehension: &'a Comprehension) -> Self {
        Self {
            target: (&comprehension.target).into(),
            iter: (&comprehension.iter).into(),
            ifs: comprehension
                .ifs
                .iter()
                .map(std::convert::Into::into)
                .collect(),
            is_async: &comprehension.is_async,
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
        conversion: &'a usize,
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
                values: values.iter().map(std::convert::Into::into).collect(),
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
                    .map(|expr| expr.as_ref().map(std::convert::Into::into))
                    .collect(),
                values: values.iter().map(std::convert::Into::into).collect(),
            },
            ExprKind::Set { elts } => Self::Set {
                elts: elts.iter().map(std::convert::Into::into).collect(),
            },
            ExprKind::ListComp { elt, generators } => Self::ListComp {
                elt: elt.into(),
                generators: generators.iter().map(std::convert::Into::into).collect(),
            },
            ExprKind::SetComp { elt, generators } => Self::SetComp {
                elt: elt.into(),
                generators: generators.iter().map(std::convert::Into::into).collect(),
            },
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => Self::DictComp {
                key: key.into(),
                value: value.into(),
                generators: generators.iter().map(std::convert::Into::into).collect(),
            },
            ExprKind::GeneratorExp { elt, generators } => Self::GeneratorExp {
                elt: elt.into(),
                generators: generators.iter().map(std::convert::Into::into).collect(),
            },
            ExprKind::Await { value } => Self::Await {
                value: value.into(),
            },
            ExprKind::Yield { value } => Self::Yield {
                value: value.as_ref().map(std::convert::Into::into),
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
                ops: ops.iter().map(std::convert::Into::into).collect(),
                comparators: comparators.iter().map(std::convert::Into::into).collect(),
            },
            ExprKind::Call {
                func,
                args,
                keywords,
            } => Self::Call {
                func: func.into(),
                args: args.iter().map(std::convert::Into::into).collect(),
                keywords: keywords.iter().map(std::convert::Into::into).collect(),
            },
            ExprKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => Self::FormattedValue {
                value: value.into(),
                conversion,
                format_spec: format_spec.as_ref().map(std::convert::Into::into),
            },
            ExprKind::JoinedStr { values } => Self::JoinedStr {
                values: values.iter().map(std::convert::Into::into).collect(),
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
                elts: elts.iter().map(std::convert::Into::into).collect(),
                ctx: ctx.into(),
            },
            ExprKind::Tuple { elts, ctx } => Self::Tuple {
                elts: elts.iter().map(std::convert::Into::into).collect(),
                ctx: ctx.into(),
            },
            ExprKind::Slice { lower, upper, step } => Self::Slice {
                lower: lower.as_ref().map(std::convert::Into::into),
                upper: upper.as_ref().map(std::convert::Into::into),
                step: step.as_ref().map(std::convert::Into::into),
            },
        }
    }
}
