use crate::{nodes, Arguments, Expr, Keyword};
use ruff_text_size::TextRange;

fn relocate_keyword(keyword: &mut Keyword, location: TextRange) {
    relocate_expr(&mut keyword.value, location);
}

/// Change an expression's location (recursively) to match a desired, fixed
/// location.
pub fn relocate_expr(expr: &mut Expr, location: TextRange) {
    match expr {
        Expr::BoolOp(nodes::ExprBoolOp { values, range, .. }) => {
            *range = location;
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        Expr::NamedExpr(nodes::ExprNamedExpr {
            target,
            value,
            range,
        }) => {
            *range = location;
            relocate_expr(target, location);
            relocate_expr(value, location);
        }
        Expr::BinOp(nodes::ExprBinOp {
            left, right, range, ..
        }) => {
            *range = location;
            relocate_expr(left, location);
            relocate_expr(right, location);
        }
        Expr::UnaryOp(nodes::ExprUnaryOp { operand, range, .. }) => {
            *range = location;
            relocate_expr(operand, location);
        }
        Expr::Lambda(nodes::ExprLambda { body, range, .. }) => {
            *range = location;
            relocate_expr(body, location);
        }
        Expr::IfExp(nodes::ExprIfExp {
            test,
            body,
            orelse,
            range,
        }) => {
            *range = location;
            relocate_expr(test, location);
            relocate_expr(body, location);
            relocate_expr(orelse, location);
        }
        Expr::Dict(nodes::ExprDict {
            keys,
            values,
            range,
        }) => {
            *range = location;
            for expr in keys.iter_mut().flatten() {
                relocate_expr(expr, location);
            }
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        Expr::Set(nodes::ExprSet { elts, range }) => {
            *range = location;
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        Expr::ListComp(nodes::ExprListComp { elt, range, .. }) => {
            *range = location;
            relocate_expr(elt, location);
        }
        Expr::SetComp(nodes::ExprSetComp { elt, range, .. }) => {
            *range = location;
            relocate_expr(elt, location);
        }
        Expr::DictComp(nodes::ExprDictComp {
            key, value, range, ..
        }) => {
            *range = location;
            relocate_expr(key, location);
            relocate_expr(value, location);
        }
        Expr::GeneratorExp(nodes::ExprGeneratorExp { elt, range, .. }) => {
            *range = location;
            relocate_expr(elt, location);
        }
        Expr::Await(nodes::ExprAwait { value, range }) => {
            *range = location;
            relocate_expr(value, location);
        }
        Expr::Yield(nodes::ExprYield { value, range }) => {
            *range = location;
            if let Some(expr) = value {
                relocate_expr(expr, location);
            }
        }
        Expr::YieldFrom(nodes::ExprYieldFrom { value, range }) => {
            *range = location;
            relocate_expr(value, location);
        }
        Expr::Compare(nodes::ExprCompare {
            left,
            comparators,
            range,
            ..
        }) => {
            *range = location;
            relocate_expr(left, location);
            for expr in comparators {
                relocate_expr(expr, location);
            }
        }
        Expr::Call(nodes::ExprCall {
            func,
            arguments: Arguments { args, keywords, .. },
            range,
        }) => {
            *range = location;
            relocate_expr(func, location);
            for expr in args {
                relocate_expr(expr, location);
            }
            for keyword in keywords {
                relocate_keyword(keyword, location);
            }
        }
        Expr::FormattedValue(nodes::ExprFormattedValue {
            value,
            format_spec,
            range,
            ..
        }) => {
            *range = location;
            relocate_expr(value, location);
            if let Some(expr) = format_spec {
                relocate_expr(expr, location);
            }
        }
        Expr::FString(nodes::ExprFString { values, range, .. }) => {
            *range = location;
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        Expr::Constant(nodes::ExprConstant { range, .. }) => {
            *range = location;
        }
        Expr::Attribute(nodes::ExprAttribute { value, range, .. }) => {
            *range = location;
            relocate_expr(value, location);
        }
        Expr::Subscript(nodes::ExprSubscript {
            value,
            slice,
            range,
            ..
        }) => {
            *range = location;
            relocate_expr(value, location);
            relocate_expr(slice, location);
        }
        Expr::Starred(nodes::ExprStarred { value, range, .. }) => {
            *range = location;
            relocate_expr(value, location);
        }
        Expr::Name(nodes::ExprName { range, .. }) => {
            *range = location;
        }
        Expr::List(nodes::ExprList { elts, range, .. }) => {
            *range = location;
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        Expr::Tuple(nodes::ExprTuple { elts, range, .. }) => {
            *range = location;
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        Expr::Slice(nodes::ExprSlice {
            lower,
            upper,
            step,
            range,
        }) => {
            *range = location;
            if let Some(expr) = lower {
                relocate_expr(expr, location);
            }
            if let Some(expr) = upper {
                relocate_expr(expr, location);
            }
            if let Some(expr) = step {
                relocate_expr(expr, location);
            }
        }
        Expr::IpyEscapeCommand(nodes::ExprIpyEscapeCommand { range, .. }) => {
            *range = location;
        }
    }
}
