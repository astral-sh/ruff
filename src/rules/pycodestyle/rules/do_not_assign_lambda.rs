use rustpython_ast::{Arguments, Location, Stmt, StmtKind};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::helpers::{match_leading_content, match_trailing_content, unparse_stmt};
use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Stylist;
use crate::violations;

/// E731
pub fn do_not_assign_lambda(checker: &mut Checker, target: &Expr, value: &Expr, stmt: &Stmt) {
    if let ExprKind::Name { id, .. } = &target.node {
        if let ExprKind::Lambda { args, body } = &value.node {
            let mut diagnostic = Diagnostic::new(
                violations::DoNotAssignLambda(id.to_string()),
                Range::from_located(stmt),
            );
            if checker.patch(diagnostic.kind.rule()) {
                if !match_leading_content(stmt, checker.locator)
                    && !match_trailing_content(stmt, checker.locator)
                {
                    let first_line = checker.locator.slice_source_code_range(&Range::new(
                        Location::new(stmt.location.row(), 0),
                        Location::new(stmt.location.row() + 1, 0),
                    ));
                    let indentation = &leading_space(first_line);
                    let mut indented = String::new();
                    for (idx, line) in function(id, args, body, checker.stylist)
                        .lines()
                        .enumerate()
                    {
                        if idx == 0 {
                            indented.push_str(line);
                        } else {
                            indented.push('\n');
                            indented.push_str(indentation);
                            indented.push_str(line);
                        }
                    }
                    diagnostic.amend(Fix::replacement(
                        indented,
                        stmt.location,
                        stmt.end_location.unwrap(),
                    ));
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn function(name: &str, args: &Arguments, body: &Expr, stylist: &Stylist) -> String {
    let body = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::Return {
            value: Some(Box::new(body.clone())),
        },
    );
    let func = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::FunctionDef {
            name: name.to_string(),
            args: Box::new(args.clone()),
            body: vec![body],
            decorator_list: vec![],
            returns: None,
            type_comment: None,
        },
    );
    unparse_stmt(&func, stylist)
}
