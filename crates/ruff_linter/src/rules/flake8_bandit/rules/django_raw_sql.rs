use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of Django's `RawSQL` function.
///
/// ## Why is this bad?
/// Django's `RawSQL` function can be used to execute arbitrary SQL queries,
/// which can in turn lead to SQL injection vulnerabilities.
///
/// ## Example
/// ```python
/// from django.db.models.expressions import RawSQL
/// from django.contrib.auth.models import User
///
/// User.objects.annotate(val=("%secure" % "nos", []))
/// ```
///
/// ## References
/// - [Django documentation: SQL injection protection](https://docs.djangoproject.com/en/dev/topics/security/#sql-injection-protection)
/// - [Common Weakness Enumeration: CWE-89](https://cwe.mitre.org/data/definitions/89.html)
#[violation]
pub struct DjangoRawSql;

impl Violation for DjangoRawSql {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `RawSQL` can lead to SQL injection vulnerabilities")
    }
}

/// S611
pub(crate) fn django_raw_sql(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["django", "db", "models", "expressions", "RawSQL"]
            )
        })
    {
        if !call
            .arguments
            .find_argument("sql", 0)
            .is_some_and(Expr::is_string_literal_expr)
        {
            checker
                .diagnostics
                .push(Diagnostic::new(DjangoRawSql, call.func.range()));
        }
    }
}
