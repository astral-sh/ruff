use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;
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
/// User.objects.annotate(val=RawSQL("%s" % input_param, []))
/// ```
///
/// ## References
/// - [Django documentation: SQL injection protection](https://docs.djangoproject.com/en/dev/topics/security/#sql-injection-protection)
/// - [Common Weakness Enumeration: CWE-89](https://cwe.mitre.org/data/definitions/89.html)
#[derive(ViolationMetadata)]
pub(crate) struct DjangoRawSql;

impl Violation for DjangoRawSql {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `RawSQL` can lead to SQL injection vulnerabilities".to_string()
    }
}

/// S611
pub(crate) fn django_raw_sql(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["django", "db", "models", "expressions", "RawSQL"]
            )
        })
    {
        if !call
            .arguments
            .find_argument_value("sql", 0)
            .is_some_and(Expr::is_string_literal_expr)
        {
            checker.report_diagnostic(Diagnostic::new(DjangoRawSql, call.func.range()));
        }
    }
}
