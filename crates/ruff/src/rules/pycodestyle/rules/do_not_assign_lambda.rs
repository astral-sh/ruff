use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Arguments, Expr, ExprKind, Location, Stmt, StmtKind};

use crate::ast::helpers::{match_leading_content, match_trailing_content, unparse_stmt};
use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Stylist;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct DoNotAssignLambda(pub String);
);
impl AlwaysAutofixableViolation for DoNotAssignLambda {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not assign a `lambda` expression, use a `def`")
    }

    fn autofix_title(&self) -> String {
        let DoNotAssignLambda(name) = self;
        format!("Rewrite `{name}` as a `def`")
    }
}

/// E731
pub fn do_not_assign_lambda(checker: &mut Checker, target: &Expr, value: &Expr, stmt: &Stmt) {
    if let ExprKind::Name { id, .. } = &target.node {
        if let ExprKind::Lambda { args, body } = &value.node {
            let mut diagnostic =
                Diagnostic::new(DoNotAssignLambda(id.to_string()), Range::from_located(stmt));
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
                            indented.push_str(checker.stylist.line_ending().as_str());
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
