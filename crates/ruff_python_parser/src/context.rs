use ruff_python_ast::{self as ast, Expr, ExprContext};

pub(crate) fn set_context(expr: Expr, ctx: ExprContext) -> Expr {
    match expr {
        Expr::Name(ast::ExprName { id, range, .. }) => ast::ExprName { range, id, ctx }.into(),
        Expr::Tuple(ast::ExprTuple { elts, range, .. }) => ast::ExprTuple {
            elts: elts.into_iter().map(|elt| set_context(elt, ctx)).collect(),
            range,
            ctx,
        }
        .into(),

        Expr::List(ast::ExprList { elts, range, .. }) => ast::ExprList {
            elts: elts.into_iter().map(|elt| set_context(elt, ctx)).collect(),
            range,
            ctx,
        }
        .into(),
        Expr::Attribute(ast::ExprAttribute {
            value, attr, range, ..
        }) => ast::ExprAttribute {
            range,
            value,
            attr,
            ctx,
        }
        .into(),
        Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            range,
            ..
        }) => ast::ExprSubscript {
            range,
            value,
            slice,
            ctx,
        }
        .into(),
        Expr::Starred(ast::ExprStarred { value, range, .. }) => ast::ExprStarred {
            value: Box::new(set_context(*value, ctx)),
            range,
            ctx,
        }
        .into(),
        _ => expr,
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_suite;

    #[test]
    fn test_assign_name() {
        let source = "x = (1, 2, 3)";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_tuple() {
        let source = "(x, y) = (1, 2, 3)";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_list() {
        let source = "[x, y] = (1, 2, 3)";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_attribute() {
        let source = "x.y = (1, 2, 3)";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_subscript() {
        let source = "x[y] = (1, 2, 3)";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_starred() {
        let source = "(x, *y) = (1, 2, 3)";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_for() {
        let source = "for x in (1, 2, 3): pass";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_list_comp() {
        let source = "x = [y for y in (1, 2, 3)]";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_set_comp() {
        let source = "x = {y for y in (1, 2, 3)}";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_with() {
        let source = "with 1 as x: pass";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_assign_named_expr() {
        let source = "if x:= 1: pass";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_ann_assign_name() {
        let source = "x: int = 1";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_aug_assign_name() {
        let source = "x += 1";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_aug_assign_attribute() {
        let source = "x.y += (1, 2, 3)";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_aug_assign_subscript() {
        let source = "x[y] += (1, 2, 3)";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_del_name() {
        let source = "del x";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_del_attribute() {
        let source = "del x.y";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_del_subscript() {
        let source = "del x[y]";
        let parse_ast = parse_suite(source, "<test>").unwrap();
        insta::assert_debug_snapshot!(parse_ast);
    }
}
