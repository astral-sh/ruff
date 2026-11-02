//! Constructs expressions for lint fixes. Synthesized nodes use empty source ranges.

use ruff_python_ast::kind;
use ruff_python_ast::{
    AstNodeId, BuildId, Builder, CmpOp, ConversionFlag, Expr, ExprCompare, ExprContext, ExprKind,
    FStringFlags, ListId, Operator, StringLiteralFlags, UnaryOp, build,
};
use ruff_text_size::TextRange;

pub(crate) fn string_literal(
    builder: &mut Builder,
    text: &str,
    flags: StringLiteralFlags,
) -> BuildId<kind::Expr> {
    let part = builder.add(build::StringLiteral {
        range: TextRange::default(),
        value: text,
        flags,
    });
    let value = builder.list([part]);
    builder
        .add(build::ExprStringLiteral {
            range: TextRange::default(),
            value,
        })
        .into()
}

pub(crate) fn literal_element(
    builder: &mut Builder,
    value: &str,
    range: TextRange,
) -> BuildId<kind::InterpolatedStringElement> {
    builder
        .add(build::InterpolatedStringLiteralElement { range, value })
        .into()
}

pub(crate) fn interpolation(
    builder: &mut Builder,
    expr: Expr<'_>,
) -> BuildId<kind::InterpolatedStringElement> {
    let expression = builder.copy_node(expr);
    builder
        .add(build::InterpolatedElement {
            range: TextRange::default(),
            expression,
            debug_text: None,
            conversion: ConversionFlag::None,
            format_spec: None,
        })
        .into()
}

pub(crate) fn f_string(
    builder: &mut Builder,
    elements: ListId<AstNodeId<kind::InterpolatedStringElement>>,
    flags: FStringFlags,
) -> BuildId<kind::Expr> {
    let part = builder.add(build::FString {
        range: TextRange::default(),
        elements,
        flags,
    });
    let value = builder.list([ruff_python_ast::FStringPartId::FString(part)]);
    builder
        .add(build::ExprFString {
            range: TextRange::default(),
            value,
        })
        .into()
}

pub(crate) fn identifier(builder: &mut Builder, spelling: &str) -> BuildId<kind::Identifier> {
    let id = builder.intern_name(spelling);
    builder.add(build::Identifier {
        range: TextRange::default(),
        id,
    })
}

pub(crate) fn name(builder: &mut Builder, spelling: &str) -> BuildId<kind::Expr> {
    let id = builder.intern_name(spelling);
    builder
        .add(build::ExprName {
            range: TextRange::default(),
            id,
            ctx: ExprContext::Load,
        })
        .into()
}

pub(crate) fn call_name(
    builder: &mut Builder,
    spelling: &str,
    args: ListId<AstNodeId<kind::Expr>>,
) -> BuildId<kind::Expr> {
    let func = name(builder, spelling);
    call(builder, func, args)
}

pub(crate) fn attribute(
    builder: &mut Builder,
    value: BuildId<kind::Expr>,
    spelling: &str,
    ctx: ExprContext,
) -> BuildId<kind::Expr> {
    let range = TextRange::default();
    let attr = identifier(builder, spelling);
    builder
        .add(build::ExprAttribute {
            range,
            value,
            attr,
            ctx,
        })
        .into()
}

pub(crate) fn call_method(
    builder: &mut Builder,
    object: BuildId<kind::Expr>,
    method: &str,
    args: ListId<AstNodeId<kind::Expr>>,
) -> BuildId<kind::Expr> {
    let func = attribute(builder, object, method, ExprContext::Load);
    call(builder, func, args)
}

fn call(
    builder: &mut Builder,
    func: BuildId<kind::Expr>,
    args: ListId<AstNodeId<kind::Expr>>,
) -> BuildId<kind::Expr> {
    call_with_keywords(builder, func, args, ListId::default())
}

pub(crate) fn call_with_keywords(
    builder: &mut Builder,
    func: BuildId<kind::Expr>,
    args: ListId<AstNodeId<kind::Expr>>,
    keywords: ListId<AstNodeId<kind::Keyword>>,
) -> BuildId<kind::Expr> {
    let range = TextRange::default();
    let arguments = builder.add(build::Arguments {
        range,
        args,
        keywords,
    });
    builder
        .add(build::ExprCall {
            range,
            func,
            arguments,
        })
        .into()
}

pub(crate) fn not_expr(builder: &mut Builder, operand: Expr<'_>) -> BuildId<kind::Expr> {
    let operand = builder.copy_node(operand);
    builder
        .add(build::ExprUnaryOp {
            range: TextRange::default(),
            op: UnaryOp::Not,
            operand,
        })
        .into()
}

pub(crate) fn assignment(
    builder: &mut Builder,
    target: Expr<'_>,
    value: BuildId<kind::Expr>,
) -> BuildId<kind::Stmt> {
    let targets = builder.copy_nodes([target]);
    builder
        .add(build::StmtAssign {
            range: TextRange::default(),
            targets,
            value,
        })
        .into()
}

pub(crate) fn expression_statement(
    builder: &mut Builder,
    value: BuildId<kind::Expr>,
) -> BuildId<kind::Stmt> {
    builder
        .add(build::StmtExpr {
            range: TextRange::default(),
            value,
        })
        .into()
}

pub(crate) fn compare(
    builder: &mut Builder,
    left: BuildId<kind::Expr>,
    op: CmpOp,
    right: BuildId<kind::Expr>,
) -> BuildId<kind::Expr> {
    let operands = builder.list([left, right]);
    let ops = builder.list([op]);
    builder
        .add(build::ExprCompare {
            range: TextRange::default(),
            operands,
            ops,
        })
        .into()
}

pub(crate) fn starred(builder: &mut Builder, value: BuildId<kind::Expr>) -> BuildId<kind::Expr> {
    builder
        .add(build::ExprStarred {
            range: TextRange::default(),
            ctx: ExprContext::Load,
            value,
        })
        .into()
}

pub(crate) fn list(
    builder: &mut Builder,
    elts: ListId<AstNodeId<kind::Expr>>,
) -> BuildId<kind::Expr> {
    builder
        .add(build::ExprList {
            range: TextRange::default(),
            ctx: ExprContext::Load,
            elts,
        })
        .into()
}

pub(crate) fn tuple(
    builder: &mut Builder,
    elts: ListId<AstNodeId<kind::Expr>>,
) -> BuildId<kind::Expr> {
    builder
        .add(build::ExprTuple {
            range: TextRange::default(),
            elts,
            ctx: ExprContext::Load,
            parenthesized: true,
        })
        .into()
}

pub(crate) fn subscript(
    builder: &mut Builder,
    value: BuildId<kind::Expr>,
    slice: BuildId<kind::Expr>,
) -> BuildId<kind::Expr> {
    builder
        .add(build::ExprSubscript {
            range: TextRange::default(),
            ctx: ExprContext::Load,
            value,
            slice,
        })
        .into()
}

/// Constructs a subscription with comma-separated items, including the trailing comma for one item.
pub(crate) fn subscript_tuple(
    builder: &mut Builder,
    value: BuildId<kind::Expr>,
    elts: ListId<AstNodeId<kind::Expr>>,
) -> BuildId<kind::Expr> {
    let slice = builder.add(build::ExprTuple {
        range: TextRange::default(),
        elts,
        ctx: ExprContext::Load,
        parenthesized: false,
    });
    subscript(builder, value, slice.into())
}

/// A single member uses a scalar subscript, as in `Literal[1]`.
pub(crate) fn subscript_members<'a>(
    builder: &mut Builder,
    value: BuildId<kind::Expr>,
    mut members: impl ExactSizeIterator<Item = Expr<'a>>,
) -> BuildId<kind::Expr> {
    if members.len() == 1
        && let Some(member) = members.next()
    {
        let slice = builder.copy_node(member);
        subscript(builder, value, slice)
    } else {
        let elts = builder.copy_nodes(members);
        subscript_tuple(builder, value, elts)
    }
}

pub(crate) fn none(builder: &mut Builder) -> BuildId<kind::Expr> {
    builder
        .add(build::ExprNoneLiteral {
            range: TextRange::default(),
        })
        .into()
}

pub(crate) fn bit_or(
    builder: &mut Builder,
    left: BuildId<kind::Expr>,
    right: BuildId<kind::Expr>,
) -> BuildId<kind::Expr> {
    builder
        .add(build::ExprBinOp {
            range: TextRange::default(),
            op: Operator::BitOr,
            left,
            right,
        })
        .into()
}

pub(crate) fn pep_604_optional(
    builder: &mut Builder,
    value: BuildId<kind::Expr>,
) -> BuildId<kind::Expr> {
    let none = none(builder);
    bit_or(builder, value, none)
}

/// Converts nested tuples of types to unions, as in `isinstance(x, (A, (B, C)))`.
pub(crate) fn pep_604_union<'a>(
    builder: &mut Builder,
    members: impl IntoIterator<Item = Expr<'a>>,
) -> BuildId<kind::Expr> {
    let mut members = members.into_iter();
    let Some(first) = members.next() else {
        return tuple(builder, ListId::default());
    };
    let first = copy_union_member(builder, first);
    extend_union(builder, first, members)
}

pub(crate) fn extend_union<'a>(
    builder: &mut Builder,
    first: BuildId<kind::Expr>,
    members: impl IntoIterator<Item = Expr<'a>>,
) -> BuildId<kind::Expr> {
    members.into_iter().fold(first, |left, member| {
        let right = copy_union_member(builder, member);
        bit_or(builder, left, right)
    })
}

fn copy_union_member(builder: &mut Builder, member: Expr<'_>) -> BuildId<kind::Expr> {
    match member.classify() {
        ExprKind::Tuple(tuple) => pep_604_union(builder, tuple.elts()),
        _ => builder.copy_node(member),
    }
}

/// Rebuilds a single comparison with the given operator.
pub(crate) fn with_comparison_op(
    builder: &mut Builder,
    comparison: ExprCompare<'_>,
    op: CmpOp,
) -> BuildId<kind::Expr> {
    let operands = builder.copy_nodes(comparison.operands());
    let ops = builder.list([op]);
    builder
        .add(build::ExprCompare {
            range: TextRange::default(),
            operands,
            ops,
        })
        .into()
}
