use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Expr, Keyword};

fn relocate_keyword(keyword: &mut Keyword, location: TextRange) {
    relocate_expr(&mut keyword.value, location);
}

/// Change an expression's location (recursively) to match a desired, fixed
/// location.
pub fn relocate_expr(expr: &mut Expr, location: TextRange) {
    match expr {
        Expr::BoolOp(ast::ExprBoolOp { values, range, .. }) => {
            *range = location;
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        Expr::NamedExpr(ast::ExprNamedExpr {
            target,
            value,
            range,
        }) => {
            *range = location;
            relocate_expr(target, location);
            relocate_expr(value, location);
        }
        Expr::BinOp(ast::ExprBinOp {
            left, right, range, ..
        }) => {
            *range = location;
            relocate_expr(left, location);
            relocate_expr(right, location);
        }
        Expr::UnaryOp(ast::ExprUnaryOp { operand, range, .. }) => {
            *range = location;
            relocate_expr(operand, location);
        }
        Expr::Lambda(ast::ExprLambda { body, range, .. }) => {
            *range = location;
            relocate_expr(body, location);
        }
        Expr::IfExp(ast::ExprIfExp {
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
        Expr::Dict(ast::ExprDict {
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
        Expr::Set(ast::ExprSet { elts, range }) => {
            *range = location;
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        Expr::ListComp(ast::ExprListComp { elt, range, .. }) => {
            *range = location;
            relocate_expr(elt, location);
        }
        Expr::SetComp(ast::ExprSetComp { elt, range, .. }) => {
            *range = location;
            relocate_expr(elt, location);
        }
        Expr::DictComp(ast::ExprDictComp {
            key, value, range, ..
        }) => {
            *range = location;
            relocate_expr(key, location);
            relocate_expr(value, location);
        }
        Expr::GeneratorExp(ast::ExprGeneratorExp { elt, range, .. }) => {
            *range = location;
            relocate_expr(elt, location);
        }
        Expr::Await(ast::ExprAwait { value, range }) => {
            *range = location;
            relocate_expr(value, location);
        }
        Expr::Yield(ast::ExprYield { value, range }) => {
            *range = location;
            if let Some(expr) = value {
                relocate_expr(expr, location);
            }
        }
        Expr::YieldFrom(ast::ExprYieldFrom { value, range }) => {
            *range = location;
            relocate_expr(value, location);
        }
        Expr::Compare(ast::ExprCompare {
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
        Expr::Call(ast::ExprCall {
            func,
            args,
            keywords,
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
        Expr::FormattedValue(ast::ExprFormattedValue {
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
        Expr::JoinedStr(ast::ExprJoinedStr { values, range }) => {
            *range = location;
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        Expr::Constant(ast::ExprConstant { range, .. }) => {
            *range = location;
        }
        Expr::Attribute(ast::ExprAttribute { value, range, .. }) => {
            *range = location;
            relocate_expr(value, location);
        }
        Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            range,
            ..
        }) => {
            *range = location;
            relocate_expr(value, location);
            relocate_expr(slice, location);
        }
        Expr::Starred(ast::ExprStarred { value, range, .. }) => {
            *range = location;
            relocate_expr(value, location);
        }
        Expr::Name(ast::ExprName { range, .. }) => {
            *range = location;
        }
        Expr::List(ast::ExprList { elts, range, .. }) => {
            *range = location;
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        Expr::Tuple(ast::ExprTuple { elts, range, .. }) => {
            *range = location;
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        Expr::Slice(ast::ExprSlice {
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
    }
}
