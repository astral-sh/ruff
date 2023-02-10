use once_cell::sync::Lazy;
use regex::Regex;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Operator};

use super::super::helpers::string_literal;
use crate::ast::helpers::{any_over_expr, unparse_expr};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

static SQL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(select\s.*from\s|delete\s+from\s|insert\s+into\s.*values\s|update\s.*set\s)")
        .unwrap()
});

define_violation!(
    /// ### What it does
    /// Checks for strings that resemble SQL statements involved in some form
    /// string building operation.
    ///
    /// ### Why is this bad?
    /// SQL injection is a common attack vector for web applications. Unless care
    /// is taken to sanitize and control the input data when building such
    /// SQL statement strings, an injection attack becomes possible.
    ///
    /// ### Example
    /// ```python
    /// query = "DELETE FROM foo WHERE id = '%s'" % identifier
    /// ```
    pub struct HardcodedSQLExpression {
        pub string: String,
    }
);
impl Violation for HardcodedSQLExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedSQLExpression { string } = self;
        format!(
            "Possible SQL injection vector through string-based query construction: \"{}\"",
            string.escape_debug()
        )
    }
}

fn has_string_literal(expr: &Expr) -> bool {
    string_literal(expr).is_some()
}

fn matches_sql_statement(string: &str) -> bool {
    SQL_REGEX.is_match(string)
}

fn unparse_string_format_expression(checker: &mut Checker, expr: &Expr) -> Option<String> {
    match &expr.node {
        // "select * from table where val = " + "str" + ...
        // "select * from table where val = %s" % ...
        ExprKind::BinOp {
            op: Operator::Add | Operator::Mod,
            ..
        } => {
            let Some(parent) = checker.current_expr_parent() else {
                if any_over_expr(expr, &has_string_literal) {
                    return Some(unparse_expr(expr, checker.stylist));
                }
                return None;
            };
            // Only evaluate the full BinOp, not the nested components.
            let ExprKind::BinOp { .. } = &parent.node else {
                if any_over_expr(expr, &has_string_literal) {
                    return Some(unparse_expr(expr, checker.stylist));
                }
                return None;
            };
            None
        }
        ExprKind::Call { func, .. } => {
            let ExprKind::Attribute{ attr, value, .. } = &func.node else {
                return None;
            };
            // "select * from table where val = {}".format(...)
            if attr == "format" && string_literal(value).is_some() {
                return Some(unparse_expr(expr, checker.stylist));
            };
            None
        }
        // f"select * from table where val = {val}"
        ExprKind::JoinedStr { .. } => Some(unparse_expr(expr, checker.stylist)),
        _ => None,
    }
}

/// S608
pub fn hardcoded_sql_expression(checker: &mut Checker, expr: &Expr) {
    match unparse_string_format_expression(checker, expr) {
        Some(string) if matches_sql_statement(&string) => {
            checker.diagnostics.push(Diagnostic::new(
                HardcodedSQLExpression { string },
                Range::from_located(expr),
            ));
        }
        _ => (),
    }
}
