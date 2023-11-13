use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for Django that use `RawSQL` function.
///
/// ## Why is this bad?
/// Django `RawSQL` function can cause SQL injection attack.
///
/// ## Example
/// ```python
/// from django.db.models.expressions import RawSQL
/// from django.contrib.auth.models import User
///
/// User.objects.annotate(val=RawSQL("%secure" % "nos", []))
/// ```
///
/// ## References
/// - [Django documentation: API](https://docs.djangoproject.com/en/dev/ref/models/expressions/#django.db.models.expressions.RawSQL)
/// - [Django documentation: sql injection protection](https://docs.djangoproject.com/en/dev/topics/security/#sql-injection-protection)
/// - [Common Weakness Enumeration: CWE-89](https://cwe.mitre.org/data/definitions/89.html)
#[violation]
pub struct DjangoRawSql;

impl Violation for DjangoRawSql {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of RawSQL potential SQL attack vector.")
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
        let sql = if let Some(arg) = call.arguments.find_argument("sql", 0) {
            arg
        } else {
            &call.arguments.find_keyword("sql").unwrap().value
        };

        if !sql.is_string_literal_expr() {
            checker
                .diagnostics
                .push(Diagnostic::new(DjangoRawSql, call.func.range()));
        }
    }
}
