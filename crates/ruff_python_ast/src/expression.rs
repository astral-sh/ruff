use crate::node::AnyNodeRef;
use crate::visitor::preorder::PreorderVisitor;
use ruff_text_size::TextRange;
use rustpython_ast::{Expr, Ranged};
use rustpython_parser::ast;

/// Unowned pendant to [`ast::Expr`] that stores a reference instead of a owned value.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ExpressionRef<'a> {
    BoolOp(&'a ast::ExprBoolOp),
    NamedExpr(&'a ast::ExprNamedExpr),
    BinOp(&'a ast::ExprBinOp),
    UnaryOp(&'a ast::ExprUnaryOp),
    Lambda(&'a ast::ExprLambda),
    IfExp(&'a ast::ExprIfExp),
    Dict(&'a ast::ExprDict),
    Set(&'a ast::ExprSet),
    ListComp(&'a ast::ExprListComp),
    SetComp(&'a ast::ExprSetComp),
    DictComp(&'a ast::ExprDictComp),
    GeneratorExp(&'a ast::ExprGeneratorExp),
    Await(&'a ast::ExprAwait),
    Yield(&'a ast::ExprYield),
    YieldFrom(&'a ast::ExprYieldFrom),
    Compare(&'a ast::ExprCompare),
    Call(&'a ast::ExprCall),
    FormattedValue(&'a ast::ExprFormattedValue),
    JoinedStr(&'a ast::ExprJoinedStr),
    Constant(&'a ast::ExprConstant),
    Attribute(&'a ast::ExprAttribute),
    Subscript(&'a ast::ExprSubscript),
    Starred(&'a ast::ExprStarred),
    Name(&'a ast::ExprName),
    List(&'a ast::ExprList),
    Tuple(&'a ast::ExprTuple),
    Slice(&'a ast::ExprSlice),
}

impl<'a> From<&'a Box<Expr>> for ExpressionRef<'a> {
    fn from(value: &'a Box<Expr>) -> Self {
        ExpressionRef::from(value.as_ref())
    }
}

impl<'a> From<&'a Expr> for ExpressionRef<'a> {
    fn from(value: &'a Expr) -> Self {
        match value {
            Expr::BoolOp(value) => ExpressionRef::BoolOp(value),
            Expr::NamedExpr(value) => ExpressionRef::NamedExpr(value),
            Expr::BinOp(value) => ExpressionRef::BinOp(value),
            Expr::UnaryOp(value) => ExpressionRef::UnaryOp(value),
            Expr::Lambda(value) => ExpressionRef::Lambda(value),
            Expr::IfExp(value) => ExpressionRef::IfExp(value),
            Expr::Dict(value) => ExpressionRef::Dict(value),
            Expr::Set(value) => ExpressionRef::Set(value),
            Expr::ListComp(value) => ExpressionRef::ListComp(value),
            Expr::SetComp(value) => ExpressionRef::SetComp(value),
            Expr::DictComp(value) => ExpressionRef::DictComp(value),
            Expr::GeneratorExp(value) => ExpressionRef::GeneratorExp(value),
            Expr::Await(value) => ExpressionRef::Await(value),
            Expr::Yield(value) => ExpressionRef::Yield(value),
            Expr::YieldFrom(value) => ExpressionRef::YieldFrom(value),
            Expr::Compare(value) => ExpressionRef::Compare(value),
            Expr::Call(value) => ExpressionRef::Call(value),
            Expr::FormattedValue(value) => ExpressionRef::FormattedValue(value),
            Expr::JoinedStr(value) => ExpressionRef::JoinedStr(value),
            Expr::Constant(value) => ExpressionRef::Constant(value),
            Expr::Attribute(value) => ExpressionRef::Attribute(value),
            Expr::Subscript(value) => ExpressionRef::Subscript(value),
            Expr::Starred(value) => ExpressionRef::Starred(value),
            Expr::Name(value) => ExpressionRef::Name(value),
            Expr::List(value) => ExpressionRef::List(value),
            Expr::Tuple(value) => ExpressionRef::Tuple(value),
            Expr::Slice(value) => ExpressionRef::Slice(value),
        }
    }
}

impl<'a> From<&'a ast::ExprBoolOp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprBoolOp) -> Self {
        Self::BoolOp(value)
    }
}
impl<'a> From<&'a ast::ExprNamedExpr> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprNamedExpr) -> Self {
        Self::NamedExpr(value)
    }
}
impl<'a> From<&'a ast::ExprBinOp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprBinOp) -> Self {
        Self::BinOp(value)
    }
}
impl<'a> From<&'a ast::ExprUnaryOp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprUnaryOp) -> Self {
        Self::UnaryOp(value)
    }
}
impl<'a> From<&'a ast::ExprLambda> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprLambda) -> Self {
        Self::Lambda(value)
    }
}
impl<'a> From<&'a ast::ExprIfExp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprIfExp) -> Self {
        Self::IfExp(value)
    }
}
impl<'a> From<&'a ast::ExprDict> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprDict) -> Self {
        Self::Dict(value)
    }
}
impl<'a> From<&'a ast::ExprSet> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprSet) -> Self {
        Self::Set(value)
    }
}
impl<'a> From<&'a ast::ExprListComp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprListComp) -> Self {
        Self::ListComp(value)
    }
}
impl<'a> From<&'a ast::ExprSetComp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprSetComp) -> Self {
        Self::SetComp(value)
    }
}
impl<'a> From<&'a ast::ExprDictComp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprDictComp) -> Self {
        Self::DictComp(value)
    }
}
impl<'a> From<&'a ast::ExprGeneratorExp> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprGeneratorExp) -> Self {
        Self::GeneratorExp(value)
    }
}
impl<'a> From<&'a ast::ExprAwait> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprAwait) -> Self {
        Self::Await(value)
    }
}
impl<'a> From<&'a ast::ExprYield> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprYield) -> Self {
        Self::Yield(value)
    }
}
impl<'a> From<&'a ast::ExprYieldFrom> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprYieldFrom) -> Self {
        Self::YieldFrom(value)
    }
}
impl<'a> From<&'a ast::ExprCompare> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprCompare) -> Self {
        Self::Compare(value)
    }
}
impl<'a> From<&'a ast::ExprCall> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprCall) -> Self {
        Self::Call(value)
    }
}
impl<'a> From<&'a ast::ExprFormattedValue> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprFormattedValue) -> Self {
        Self::FormattedValue(value)
    }
}
impl<'a> From<&'a ast::ExprJoinedStr> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprJoinedStr) -> Self {
        Self::JoinedStr(value)
    }
}
impl<'a> From<&'a ast::ExprConstant> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprConstant) -> Self {
        Self::Constant(value)
    }
}
impl<'a> From<&'a ast::ExprAttribute> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprAttribute) -> Self {
        Self::Attribute(value)
    }
}
impl<'a> From<&'a ast::ExprSubscript> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprSubscript) -> Self {
        Self::Subscript(value)
    }
}
impl<'a> From<&'a ast::ExprStarred> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprStarred) -> Self {
        Self::Starred(value)
    }
}
impl<'a> From<&'a ast::ExprName> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprName) -> Self {
        Self::Name(value)
    }
}
impl<'a> From<&'a ast::ExprList> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprList) -> Self {
        Self::List(value)
    }
}
impl<'a> From<&'a ast::ExprTuple> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprTuple) -> Self {
        Self::Tuple(value)
    }
}
impl<'a> From<&'a ast::ExprSlice> for ExpressionRef<'a> {
    fn from(value: &'a ast::ExprSlice) -> Self {
        Self::Slice(value)
    }
}

impl<'a> From<ExpressionRef<'a>> for AnyNodeRef<'a> {
    fn from(value: ExpressionRef<'a>) -> Self {
        match value {
            ExpressionRef::BoolOp(expression) => AnyNodeRef::ExprBoolOp(expression),
            ExpressionRef::NamedExpr(expression) => AnyNodeRef::ExprNamedExpr(expression),
            ExpressionRef::BinOp(expression) => AnyNodeRef::ExprBinOp(expression),
            ExpressionRef::UnaryOp(expression) => AnyNodeRef::ExprUnaryOp(expression),
            ExpressionRef::Lambda(expression) => AnyNodeRef::ExprLambda(expression),
            ExpressionRef::IfExp(expression) => AnyNodeRef::ExprIfExp(expression),
            ExpressionRef::Dict(expression) => AnyNodeRef::ExprDict(expression),
            ExpressionRef::Set(expression) => AnyNodeRef::ExprSet(expression),
            ExpressionRef::ListComp(expression) => AnyNodeRef::ExprListComp(expression),
            ExpressionRef::SetComp(expression) => AnyNodeRef::ExprSetComp(expression),
            ExpressionRef::DictComp(expression) => AnyNodeRef::ExprDictComp(expression),
            ExpressionRef::GeneratorExp(expression) => AnyNodeRef::ExprGeneratorExp(expression),
            ExpressionRef::Await(expression) => AnyNodeRef::ExprAwait(expression),
            ExpressionRef::Yield(expression) => AnyNodeRef::ExprYield(expression),
            ExpressionRef::YieldFrom(expression) => AnyNodeRef::ExprYieldFrom(expression),
            ExpressionRef::Compare(expression) => AnyNodeRef::ExprCompare(expression),
            ExpressionRef::Call(expression) => AnyNodeRef::ExprCall(expression),
            ExpressionRef::FormattedValue(expression) => AnyNodeRef::ExprFormattedValue(expression),
            ExpressionRef::JoinedStr(expression) => AnyNodeRef::ExprJoinedStr(expression),
            ExpressionRef::Constant(expression) => AnyNodeRef::ExprConstant(expression),
            ExpressionRef::Attribute(expression) => AnyNodeRef::ExprAttribute(expression),
            ExpressionRef::Subscript(expression) => AnyNodeRef::ExprSubscript(expression),
            ExpressionRef::Starred(expression) => AnyNodeRef::ExprStarred(expression),
            ExpressionRef::Name(expression) => AnyNodeRef::ExprName(expression),
            ExpressionRef::List(expression) => AnyNodeRef::ExprList(expression),
            ExpressionRef::Tuple(expression) => AnyNodeRef::ExprTuple(expression),
            ExpressionRef::Slice(expression) => AnyNodeRef::ExprSlice(expression),
        }
    }
}

impl Ranged for ExpressionRef<'_> {
    fn range(&self) -> TextRange {
        match self {
            ExpressionRef::BoolOp(expression) => expression.range(),
            ExpressionRef::NamedExpr(expression) => expression.range(),
            ExpressionRef::BinOp(expression) => expression.range(),
            ExpressionRef::UnaryOp(expression) => expression.range(),
            ExpressionRef::Lambda(expression) => expression.range(),
            ExpressionRef::IfExp(expression) => expression.range(),
            ExpressionRef::Dict(expression) => expression.range(),
            ExpressionRef::Set(expression) => expression.range(),
            ExpressionRef::ListComp(expression) => expression.range(),
            ExpressionRef::SetComp(expression) => expression.range(),
            ExpressionRef::DictComp(expression) => expression.range(),
            ExpressionRef::GeneratorExp(expression) => expression.range(),
            ExpressionRef::Await(expression) => expression.range(),
            ExpressionRef::Yield(expression) => expression.range(),
            ExpressionRef::YieldFrom(expression) => expression.range(),
            ExpressionRef::Compare(expression) => expression.range(),
            ExpressionRef::Call(expression) => expression.range(),
            ExpressionRef::FormattedValue(expression) => expression.range(),
            ExpressionRef::JoinedStr(expression) => expression.range(),
            ExpressionRef::Constant(expression) => expression.range(),
            ExpressionRef::Attribute(expression) => expression.range(),
            ExpressionRef::Subscript(expression) => expression.range(),
            ExpressionRef::Starred(expression) => expression.range(),
            ExpressionRef::Name(expression) => expression.range(),
            ExpressionRef::List(expression) => expression.range(),
            ExpressionRef::Tuple(expression) => expression.range(),
            ExpressionRef::Slice(expression) => expression.range(),
        }
    }
}

pub fn walk_expression_ref<'a, V>(visitor: &mut V, expr: ExpressionRef<'a>)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    match expr {
        ExpressionRef::BoolOp(ast::ExprBoolOp {
            op,
            values,
            range: _range,
        }) => match values.as_slice() {
            [left, rest @ ..] => {
                visitor.visit_expr(left);
                visitor.visit_bool_op(op);
                for expr in rest {
                    visitor.visit_expr(expr);
                }
            }
            [] => {
                visitor.visit_bool_op(op);
            }
        },

        ExpressionRef::NamedExpr(ast::ExprNamedExpr {
            target,
            value,
            range: _range,
        }) => {
            visitor.visit_expr(target);
            visitor.visit_expr(value);
        }

        ExpressionRef::BinOp(ast::ExprBinOp {
            left,
            op,
            right,
            range: _range,
        }) => {
            visitor.visit_expr(left);
            visitor.visit_operator(op);
            visitor.visit_expr(right);
        }

        ExpressionRef::UnaryOp(ast::ExprUnaryOp {
            op,
            operand,
            range: _range,
        }) => {
            visitor.visit_unary_op(op);
            visitor.visit_expr(operand);
        }

        ExpressionRef::Lambda(ast::ExprLambda {
            args,
            body,
            range: _range,
        }) => {
            visitor.visit_arguments(args);
            visitor.visit_expr(body);
        }

        ExpressionRef::IfExp(ast::ExprIfExp {
            test,
            body,
            orelse,
            range: _range,
        }) => {
            // `body if test else orelse`
            visitor.visit_expr(body);
            visitor.visit_expr(test);
            visitor.visit_expr(orelse);
        }

        ExpressionRef::Dict(ast::ExprDict {
            keys,
            values,
            range: _range,
        }) => {
            for (key, value) in keys.iter().zip(values) {
                if let Some(key) = key {
                    visitor.visit_expr(key);
                }
                visitor.visit_expr(value);
            }
        }

        ExpressionRef::Set(ast::ExprSet {
            elts,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }

        ExpressionRef::ListComp(ast::ExprListComp {
            elt,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        ExpressionRef::SetComp(ast::ExprSetComp {
            elt,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        ExpressionRef::DictComp(ast::ExprDictComp {
            key,
            value,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(key);
            visitor.visit_expr(value);

            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        ExpressionRef::GeneratorExp(ast::ExprGeneratorExp {
            elt,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        ExpressionRef::Await(ast::ExprAwait {
            value,
            range: _range,
        })
        | ExpressionRef::YieldFrom(ast::ExprYieldFrom {
            value,
            range: _range,
        }) => visitor.visit_expr(value),

        ExpressionRef::Yield(ast::ExprYield {
            value,
            range: _range,
        }) => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }

        ExpressionRef::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _range,
        }) => {
            visitor.visit_expr(left);

            for (op, comparator) in ops.iter().zip(comparators) {
                visitor.visit_cmp_op(op);
                visitor.visit_expr(comparator);
            }
        }

        ExpressionRef::Call(ast::ExprCall {
            func,
            args,
            keywords,
            range: _range,
        }) => {
            visitor.visit_expr(func);
            for expr in args {
                visitor.visit_expr(expr);
            }
            for keyword in keywords {
                visitor.visit_keyword(keyword);
            }
        }

        ExpressionRef::FormattedValue(ast::ExprFormattedValue {
            value, format_spec, ..
        }) => {
            visitor.visit_expr(value);

            if let Some(expr) = format_spec {
                visitor.visit_format_spec(expr);
            }
        }

        ExpressionRef::JoinedStr(ast::ExprJoinedStr {
            values,
            range: _range,
        }) => {
            for expr in values {
                visitor.visit_expr(expr);
            }
        }

        ExpressionRef::Constant(ast::ExprConstant {
            value,
            range: _,
            kind: _,
        }) => visitor.visit_constant(value),

        ExpressionRef::Attribute(ast::ExprAttribute {
            value,
            attr: _,
            ctx: _,
            range: _,
        }) => {
            visitor.visit_expr(value);
        }

        ExpressionRef::Subscript(ast::ExprSubscript {
            value,
            slice,
            ctx: _,
            range: _range,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_expr(slice);
        }
        ExpressionRef::Starred(ast::ExprStarred {
            value,
            ctx: _,
            range: _range,
        }) => {
            visitor.visit_expr(value);
        }

        ExpressionRef::Name(ast::ExprName {
            id: _,
            ctx: _,
            range: _,
        }) => {}

        ExpressionRef::List(ast::ExprList {
            elts,
            ctx: _,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }
        ExpressionRef::Tuple(ast::ExprTuple {
            elts,
            ctx: _,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }

        ExpressionRef::Slice(ast::ExprSlice {
            lower,
            upper,
            step,
            range: _range,
        }) => {
            if let Some(expr) = lower {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = upper {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = step {
                visitor.visit_expr(expr);
            }
        }
    }
}
