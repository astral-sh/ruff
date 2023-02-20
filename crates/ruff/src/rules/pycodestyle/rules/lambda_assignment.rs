use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Arguments, Expr, ExprKind, Location, Stmt, StmtKind};

use crate::ast::helpers::{match_leading_content, match_trailing_content, unparse_stmt};
use crate::ast::types::{Range, ScopeKind};
use crate::ast::whitespace::leading_space;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Stylist;
use crate::violation::{AutofixKind, Availability, Violation};

define_violation!(
    pub struct LambdaAssignment {
        pub name: String,
        pub fixable: bool,
    }
);
impl Violation for LambdaAssignment {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not assign a `lambda` expression, use a `def`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|v| format!("Rewrite `{}` as a `def`", v.name))
    }
}

/// E731
pub fn lambda_assignment(checker: &mut Checker, target: &Expr, value: &Expr, stmt: &Stmt) {
    if let ExprKind::Name { id, .. } = &target.node {
        if let ExprKind::Lambda { args, body } = &value.node {
            // If the assignment is in a class body, it might not be safe
            // to replace it because the assignment might be
            // carrying a type annotation that will be used by some
            // package like dataclasses, which wouldn't consider the
            // rewritten function definition to be equivalent.
            // See https://github.com/charliermarsh/ruff/issues/3046
            let fixable = !matches!(checker.current_scope().kind, ScopeKind::Class(_));

            let mut diagnostic = Diagnostic::new(
                LambdaAssignment {
                    name: id.to_string(),
                    fixable,
                },
                Range::from_located(stmt),
            );

            if checker.patch(diagnostic.kind.rule())
                && fixable
                && !match_leading_content(stmt, checker.locator)
                && !match_trailing_content(stmt, checker.locator)
            {
                let first_line = checker.locator.slice(&Range::new(
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
