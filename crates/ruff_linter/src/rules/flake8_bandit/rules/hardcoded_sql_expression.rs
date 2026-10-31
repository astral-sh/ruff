use std::sync::LazyLock;

use regex::Regex;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::str::raw_contents;
use ruff_python_ast::{self as ast, Expr, Operator, flat};
use ruff_text_size::Ranged;

use crate::Locator;
use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

static SQL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?isx)
        \b
        (select\s+.*\s+from\s
        |delete\s+from\s
        |(insert|replace)\s+.*\s+values\s
        |update\s+.*\s+set\s
        )
    ",
    )
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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.245", category = Category::Security)]
pub(crate) struct HardcodedSQLExpression;

impl Violation for HardcodedSQLExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Possible SQL injection vector through string-based query construction".to_string()
    }
}

/// S608
pub(crate) fn hardcoded_sql_expression(checker: &Checker, &expr: &flat::View<'_, Expr>) {
    let content = match expr.classify() {
        // "select * from table where val = " + "str" + ...
        flat::ExprKind::BinOp(expr_bin_op) if expr_bin_op.op() == Operator::Add => {
            // Only evaluate the full BinOp, not the nested components.
            if checker
                .semantic()
                .current_expression_parent()
                .is_some_and(flat::View::is_bin_op_expr)
            {
                return;
            }
            if is_explicit_concatenation(expr) != Some(true) {
                return;
            }
            checker.generator().expr_view(expr)
        }
        // "select * from table where val = %s" % ...
        flat::ExprKind::BinOp(expr_bin_op) if expr_bin_op.op() == Operator::Mod => {
            let left = expr_bin_op.left();

            let Some(string) = left.as_string_literal_expr() else {
                return;
            };
            string.value().to_str().escape_default().to_string()
        }
        flat::ExprKind::Call(expr_call) => {
            let func = expr_call.func();

            let flat::ExprKind::Attribute(attribute) = func.classify() else {
                return;
            };
            let attr = attribute.attr();
            let value = attribute.value();
            // "select * from table where val = {}".format(...)
            if attr != "format" {
                return;
            }
            let Some(string) = value.as_string_literal_expr() else {
                return;
            };
            string.value().to_str().escape_default().to_string()
        }

        // f"select * from table where val = {val}"
        flat::ExprKind::FString(f_string)
            if f_string
                .value()
                .f_strings()
                .any(|fs| fs.elements().iter().any(flat::View::is_interpolation)) =>
        {
            concatenated_f_string(f_string, checker.locator())
        }

        _ => return,
    };

    if SQL_REGEX.is_match(&content) {
        checker.report_diagnostic(HardcodedSQLExpression, expr.range());
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
fn concatenated_f_string(expr: flat::View<'_, ast::ExprFString>, locator: &Locator) -> String {
    expr.value()
        .iter()
        .filter_map(|part| raw_contents(locator.slice(part)))
        .collect()
}

/// Returns `Some(true)` if an expression appears to be an explicit string concatenation,
/// `Some(false)` if it's _not_ an explicit concatenation, and `None` if it's ambiguous.
fn is_explicit_concatenation(expr: flat::View<'_, Expr>) -> Option<bool> {
    match expr.classify() {
        flat::ExprKind::BinOp(expr_bin_op) => {
            let left = expr_bin_op.left();
            let right = expr_bin_op.right();

            let left = is_explicit_concatenation(left);
            let right = is_explicit_concatenation(right);
            match (left, right) {
                // If either side is definitively _not_ a string, neither is the expression.
                (Some(false), _) | (_, Some(false)) => Some(false),
                // If either side is definitively a string, the expression is a string.
                (Some(true), _) | (_, Some(true)) => Some(true),
                _ => None,
            }
        }
        // Ambiguous (e.g., `x + y`).
        flat::ExprKind::Call(_) => None,
        flat::ExprKind::Subscript(_) => None,
        flat::ExprKind::Attribute(_) => None,
        flat::ExprKind::Name(_) => None,

        // Non-strings.
        flat::ExprKind::Lambda(_) => Some(false),
        flat::ExprKind::List(_) => Some(false),
        flat::ExprKind::Tuple(_) => Some(false),
        flat::ExprKind::Dict(_) => Some(false),
        flat::ExprKind::Set(_) => Some(false),
        flat::ExprKind::Generator(_) => Some(false),
        flat::ExprKind::Yield(_) => Some(false),
        flat::ExprKind::YieldFrom(_) => Some(false),
        flat::ExprKind::Await(_) => Some(false),
        flat::ExprKind::Starred(_) => Some(false),
        flat::ExprKind::Slice(_) => Some(false),
        flat::ExprKind::BooleanLiteral(_) => Some(false),
        flat::ExprKind::EllipsisLiteral(_) => Some(false),
        flat::ExprKind::NumberLiteral(_) => Some(false),
        flat::ExprKind::ListComp(_) => Some(false),
        flat::ExprKind::SetComp(_) => Some(false),
        flat::ExprKind::DictComp(_) => Some(false),
        flat::ExprKind::Compare(_) => Some(false),
        flat::ExprKind::FString(_) => Some(true),
        // TODO(dylan): decide whether to trigger here
        flat::ExprKind::TString(_) => Some(false),
        flat::ExprKind::StringLiteral(_) => Some(true),
        flat::ExprKind::BytesLiteral(_) => Some(false),
        flat::ExprKind::NoneLiteral(_) => Some(false),
        flat::ExprKind::IpyEscapeCommand(_) => Some(false),

        // Conditionally strings.
        flat::ExprKind::Named(expr_named) => {
            let value = expr_named.value();

            is_explicit_concatenation(value)
        }
        flat::ExprKind::If(expr_if) => {
            let body = expr_if.body();
            let orelse = expr_if.orelse();

            let body = is_explicit_concatenation(body);
            let orelse = is_explicit_concatenation(orelse);
            match (body, orelse) {
                // If either side is definitively a string, the expression could be a string.
                (Some(true), _) | (_, Some(true)) => Some(true),
                // If both sides are definitively _not_ a string, neither is the expression.
                (Some(false), Some(false)) => Some(false),
                _ => None,
            }
        }
        flat::ExprKind::BoolOp(expr_bool_op) => {
            let values = expr_bool_op.values();

            let values = values
                .iter()
                .map(is_explicit_concatenation)
                .collect::<Vec<_>>();
            if values.contains(&Some(true)) {
                Some(true)
            } else if values.iter().all(|v| *v == Some(false)) {
                Some(false)
            } else {
                None
            }
        }
        flat::ExprKind::UnaryOp(expr_unary_op) => {
            let operand = expr_unary_op.operand();

            is_explicit_concatenation(operand)
        }
    }
}
