//! Compare two AST nodes for equality, ignoring locations.

use rustpython_ast::{Arg, Arguments, Comprehension, Expr, ExprKind, Keyword};

/// Returns `true` if the two `Expr` are equal, ignoring locations.
pub fn expr(a: &Expr, b: &Expr) -> bool {
    match (&a.node, &b.node) {
        (
            ExprKind::BoolOp {
                op: op_a,
                values: values_a,
            },
            ExprKind::BoolOp {
                op: op_b,
                values: values_b,
            },
        ) => {
            op_a == op_b
                && values_a.len() == values_b.len()
                && values_a.iter().zip(values_b).all(|(a, b)| expr(a, b))
        }
        (
            ExprKind::NamedExpr {
                target: target_a,
                value: value_a,
            },
            ExprKind::NamedExpr {
                target: target_b,
                value: value_b,
            },
        ) => expr(target_a, target_b) && expr(value_a, value_b),
        (
            ExprKind::BinOp {
                left: left_a,
                op: op_a,
                right: right_a,
            },
            ExprKind::BinOp {
                left: left_b,
                op: op_b,
                right: right_b,
            },
        ) => op_a == op_b && expr(left_a, left_b) && expr(right_a, right_b),
        (
            ExprKind::UnaryOp {
                op: op_a,
                operand: operand_a,
            },
            ExprKind::UnaryOp {
                op: op_b,
                operand: operand_b,
            },
        ) => op_a == op_b && expr(operand_a, operand_b),
        (
            ExprKind::Lambda {
                args: args_a,
                body: body_a,
            },
            ExprKind::Lambda {
                args: args_b,
                body: body_b,
            },
        ) => expr(body_a, body_b) && arguments(args_a, args_b),
        (
            ExprKind::IfExp {
                test: test_a,
                body: body_a,
                orelse: orelse_a,
            },
            ExprKind::IfExp {
                test: test_b,
                body: body_b,
                orelse: orelse_b,
            },
        ) => expr(test_a, test_b) && expr(body_a, body_b) && expr(orelse_a, orelse_b),
        (
            ExprKind::Dict {
                keys: keys_a,
                values: values_a,
            },
            ExprKind::Dict {
                keys: keys_b,
                values: values_b,
            },
        ) => {
            keys_a.len() == keys_b.len()
                && values_a.len() == values_b.len()
                && keys_a.iter().zip(keys_b).all(|(a, b)| expr(a, b))
                && values_a.iter().zip(values_b).all(|(a, b)| expr(a, b))
        }
        (ExprKind::Set { elts: elts_a }, ExprKind::Set { elts: elts_b }) => {
            elts_a.len() == elts_b.len() && elts_a.iter().zip(elts_b).all(|(a, b)| expr(a, b))
        }
        (
            ExprKind::ListComp {
                elt: elt_a,
                generators: generators_a,
            },
            ExprKind::ListComp {
                elt: elt_b,
                generators: generators_b,
            },
        ) => {
            expr(elt_a, elt_b)
                && generators_a.len() == generators_b.len()
                && generators_a
                    .iter()
                    .zip(generators_b)
                    .all(|(a, b)| comprehension(a, b))
        }
        (
            ExprKind::SetComp {
                elt: elt_a,
                generators: generators_a,
            },
            ExprKind::SetComp {
                elt: elt_b,
                generators: generators_b,
            },
        ) => {
            expr(elt_a, elt_b)
                && generators_a.len() == generators_b.len()
                && generators_a
                    .iter()
                    .zip(generators_b)
                    .all(|(a, b)| comprehension(a, b))
        }
        (
            ExprKind::DictComp {
                key: key_a,
                value: value_a,
                generators: generators_a,
            },
            ExprKind::DictComp {
                key: key_b,
                value: value_b,
                generators: generators_b,
            },
        ) => {
            expr(key_a, key_b)
                && expr(value_a, value_b)
                && generators_a.len() == generators_b.len()
                && generators_a
                    .iter()
                    .zip(generators_b)
                    .all(|(a, b)| comprehension(a, b))
        }
        (
            ExprKind::GeneratorExp {
                elt: elt_a,
                generators: generators_a,
            },
            ExprKind::GeneratorExp {
                elt: elt_b,
                generators: generators_b,
            },
        ) => {
            expr(elt_a, elt_b)
                && generators_a.len() == generators_b.len()
                && generators_a
                    .iter()
                    .zip(generators_b)
                    .all(|(a, b)| comprehension(a, b))
        }
        (ExprKind::Await { value: value_a }, ExprKind::Await { value: value_b }) => {
            expr(value_a, value_b)
        }
        (ExprKind::Yield { value: value_a }, ExprKind::Yield { value: value_b }) => {
            option_expr(value_a.as_deref(), value_b.as_deref())
        }
        (ExprKind::YieldFrom { value: value_a }, ExprKind::YieldFrom { value: value_b }) => {
            expr(value_a, value_b)
        }
        (
            ExprKind::Compare {
                left: left_a,
                ops: ops_a,
                comparators: comparators_a,
            },
            ExprKind::Compare {
                left: left_b,
                ops: ops_b,
                comparators: comparators_b,
            },
        ) => {
            expr(left_a, left_b)
                && ops_a == ops_b
                && comparators_a.len() == comparators_b.len()
                && comparators_a
                    .iter()
                    .zip(comparators_b)
                    .all(|(a, b)| expr(a, b))
        }
        (
            ExprKind::Call {
                func: func_a,
                args: args_a,
                keywords: keywords_a,
            },
            ExprKind::Call {
                func: func_b,
                args: args_b,
                keywords: keywords_b,
            },
        ) => {
            expr(func_a, func_b)
                && args_a.len() == args_b.len()
                && args_a.iter().zip(args_b).all(|(a, b)| expr(a, b))
                && keywords_a.len() == keywords_b.len()
                && keywords_a
                    .iter()
                    .zip(keywords_b)
                    .all(|(a, b)| keyword(a, b))
        }
        (
            ExprKind::FormattedValue {
                value: value_a,
                conversion: conversion_a,
                format_spec: format_spec_a,
            },
            ExprKind::FormattedValue {
                value: value_b,
                conversion: conversion_b,
                format_spec: format_spec_b,
            },
        ) => {
            expr(value_a, value_b)
                && conversion_a == conversion_b
                && option_expr(format_spec_a.as_deref(), format_spec_b.as_deref())
        }
        (ExprKind::JoinedStr { values: values_a }, ExprKind::JoinedStr { values: values_b }) => {
            values_a.len() == values_b.len()
                && values_a.iter().zip(values_b).all(|(a, b)| expr(a, b))
        }
        (
            ExprKind::Constant {
                value: value_a,
                kind: kind_a,
            },
            ExprKind::Constant {
                value: value_b,
                kind: kind_b,
            },
        ) => value_a == value_b && kind_a == kind_b,
        (
            ExprKind::Attribute {
                value: value_a,
                attr: attr_a,
                ctx: ctx_a,
            },
            ExprKind::Attribute {
                value: value_b,
                attr: attr_b,
                ctx: ctx_b,
            },
        ) => attr_a == attr_b && ctx_a == ctx_b && expr(value_a, value_b),
        (
            ExprKind::Subscript {
                value: value_a,
                slice: slice_a,
                ctx: ctx_a,
            },
            ExprKind::Subscript {
                value: value_b,
                slice: slice_b,
                ctx: ctx_b,
            },
        ) => ctx_a == ctx_b && expr(value_a, value_b) && expr(slice_a, slice_b),
        (
            ExprKind::Starred {
                value: value_a,
                ctx: ctx_a,
            },
            ExprKind::Starred {
                value: value_b,
                ctx: ctx_b,
            },
        ) => ctx_a == ctx_b && expr(value_a, value_b),
        (
            ExprKind::Name {
                id: id_a,
                ctx: ctx_a,
            },
            ExprKind::Name {
                id: id_b,
                ctx: ctx_b,
            },
        ) => id_a == id_b && ctx_a == ctx_b,
        (
            ExprKind::List {
                elts: elts_a,
                ctx: ctx_a,
            },
            ExprKind::List {
                elts: elts_b,
                ctx: ctx_b,
            },
        ) => {
            ctx_a == ctx_b
                && elts_a.len() == elts_b.len()
                && elts_a.iter().zip(elts_b).all(|(a, b)| expr(a, b))
        }
        (
            ExprKind::Tuple {
                elts: elts_a,
                ctx: ctx_a,
            },
            ExprKind::Tuple {
                elts: elts_b,
                ctx: ctx_b,
            },
        ) => {
            ctx_a == ctx_b
                && elts_a.len() == elts_b.len()
                && elts_a.iter().zip(elts_b).all(|(a, b)| expr(a, b))
        }
        (
            ExprKind::Slice {
                lower: lower_a,
                upper: upper_a,
                step: step_a,
            },
            ExprKind::Slice {
                lower: lower_b,
                upper: upper_b,
                step: step_b,
            },
        ) => {
            option_expr(lower_a.as_deref(), lower_b.as_deref())
                && option_expr(upper_a.as_deref(), upper_b.as_deref())
                && option_expr(step_a.as_deref(), step_b.as_deref())
        }
        _ => false,
    }
}

fn option_expr(a: Option<&Expr>, b: Option<&Expr>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => expr(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn arguments(a: &Arguments, b: &Arguments) -> bool {
    a.posonlyargs.len() == b.posonlyargs.len()
        && a.posonlyargs
            .iter()
            .zip(b.posonlyargs.iter())
            .all(|(a, b)| arg(a, b))
        && a.args.len() == b.args.len()
        && a.args.iter().zip(b.args.iter()).all(|(a, b)| arg(a, b))
        && option_arg(a.vararg.as_deref(), b.vararg.as_deref())
        && a.kwonlyargs.len() == b.kwonlyargs.len()
        && a.kwonlyargs
            .iter()
            .zip(b.kwonlyargs.iter())
            .all(|(a, b)| arg(a, b))
        && a.kw_defaults.len() == b.kw_defaults.len()
        && a.kw_defaults
            .iter()
            .zip(b.kw_defaults.iter())
            .all(|(a, b)| expr(a, b))
        && option_arg(a.kwarg.as_deref(), b.kwarg.as_deref())
        && a.defaults.len() == b.defaults.len()
        && a.defaults
            .iter()
            .zip(b.defaults.iter())
            .all(|(a, b)| expr(a, b))
}

fn arg(a: &Arg, b: &Arg) -> bool {
    a.node.arg == b.node.arg
        && option_expr(a.node.annotation.as_deref(), b.node.annotation.as_deref())
}

fn option_arg(a: Option<&Arg>, b: Option<&Arg>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => arg(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn keyword(a: &Keyword, b: &Keyword) -> bool {
    a.node.arg == b.node.arg && expr(&a.node.value, &b.node.value)
}

fn comprehension(a: &Comprehension, b: &Comprehension) -> bool {
    expr(&a.iter, &b.iter)
        && expr(&a.target, &b.target)
        && a.ifs.len() == b.ifs.len()
        && a.ifs.iter().zip(b.ifs.iter()).all(|(a, b)| expr(a, b))
}
