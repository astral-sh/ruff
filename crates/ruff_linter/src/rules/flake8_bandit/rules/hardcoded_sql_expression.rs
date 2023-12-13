use once_cell::sync::Lazy;
use regex::Regex;
use ruff_python_ast::{self as ast, Expr, Operator};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::str::raw_contents;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

static SQL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(select\s.+\sfrom\s|delete\s+from\s|(insert|replace)\s.+\svalues\s|update\s.+\sset\s)")
        .unwrap()
});

/// ## What it does
/// Checks for strings that resemble SQL statements involved in some form
/// string building operation.
///
/// ## Why is this bad?
/// SQL injection is a common attack vector for web applications. Directly
/// interpolating user input into SQL statements should always be avoided.
/// Instead, favor parameterized queries, in which the SQL statement is
/// provided separately from its parameters, as supported by `psycopg3`
/// and other database drivers and ORMs.
///
/// ## Example
/// ```python
/// query = "DELETE FROM foo WHERE id = '%s'" % identifier
/// ```
///
/// ## References
/// - [B608: Test for SQL injection](https://bandit.readthedocs.io/en/latest/plugins/b608_hardcoded_sql_expressions.html)
/// - [psycopg3: Server-side binding](https://www.psycopg.org/psycopg3/docs/basic/from_pg2.html#server-side-binding)
#[violation]
pub struct HardcodedSQLExpression;

impl Violation for HardcodedSQLExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Possible SQL injection vector through string-based query construction")
    }
}

/// Concatenates the contents of an f-string, without the prefix and quotes,
/// and escapes any special characters.
///
/// ## Example
///
/// ```python
/// "foo" f"bar {x}" "baz"
/// ```
///
/// becomes `foobar {x}baz`.
fn concatenated_f_string(expr: &ast::ExprFString, locator: &Locator) -> String {
    expr.value
        .iter()
        .filter_map(|part| {
            raw_contents(locator.slice(part)).map(|s| s.escape_default().to_string())
        })
        .collect()
}

/// S608
pub(crate) fn hardcoded_sql_expression(checker: &mut Checker, expr: &Expr) {
    let content = match expr {
        // "select * from table where val = " + "str" + ...
        Expr::BinOp(ast::ExprBinOp {
            op: Operator::Add, ..
        }) => {
            // Only evaluate the full BinOp, not the nested components.
            if !checker
                .semantic()
                .current_expression_parent()
                .map_or(true, |parent| !parent.is_bin_op_expr())
            {
                return;
            }
            if !any_over_expr(expr, &Expr::is_string_literal_expr) {
                return;
            }
            checker.generator().expr(expr)
        }
        // "select * from table where val = %s" % ...
        Expr::BinOp(ast::ExprBinOp {
            left,
            op: Operator::Mod,
            ..
        }) => {
            let Some(string) = left.as_string_literal_expr() else {
                return;
            };
            string.value.to_str().escape_default().to_string()
        }
        Expr::Call(ast::ExprCall { func, .. }) => {
            let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() else {
                return;
            };
            // "select * from table where val = {}".format(...)
            if attr != "format" {
                return;
            }
            let Some(string) = value.as_string_literal_expr() else {
                return;
            };
            string.value.to_str().escape_default().to_string()
        }
        // f"select * from table where val = {val}"
        Expr::FString(f_string) => concatenated_f_string(f_string, checker.locator()),
        _ => return,
    };

    if SQL_REGEX.is_match(&content) {
        checker
            .diagnostics
            .push(Diagnostic::new(HardcodedSQLExpression, expr.range()));
    }
}
