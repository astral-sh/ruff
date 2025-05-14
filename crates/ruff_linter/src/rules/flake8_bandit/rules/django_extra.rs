use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, ExprAttribute};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of Django's `extra` function where one or more arguments
/// passed are not literal expressions.
///
/// ## Why is this bad?
/// Django's `extra` function can be used to execute arbitrary SQL queries,
/// which can in turn lead to SQL injection vulnerabilities.
///
/// ## Example
/// ```python
/// from django.contrib.auth.models import User
///
/// # String interpolation creates a security loophole that could be used
/// # for SQL injection:
/// User.objects.all().extra(select={"test": "%secure" % "nos"})
/// ```
///
/// Use instead:
/// ```python
/// from django.contrib.auth.models import User
///
/// # SQL injection is impossible if all arguments are literal expressions:
/// User.objects.all().extra(select={"test": "secure"})
/// ```
///
/// ## References
/// - [Django documentation: SQL injection protection](https://docs.djangoproject.com/en/dev/topics/security/#sql-injection-protection)
/// - [Common Weakness Enumeration: CWE-89](https://cwe.mitre.org/data/definitions/89.html)
#[derive(ViolationMetadata)]
pub(crate) struct DjangoExtra;

impl Violation for DjangoExtra {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of Django `extra` can lead to SQL injection vulnerabilities".to_string()
    }
}

/// S610
pub(crate) fn django_extra(checker: &Checker, call: &ast::ExprCall) {
    let Expr::Attribute(ExprAttribute { attr, .. }) = call.func.as_ref() else {
        return;
    };

    if attr.as_str() != "extra" {
        return;
    }

    if is_call_insecure(call) {
        checker.report_diagnostic(Diagnostic::new(DjangoExtra, call.arguments.range()));
    }
}

fn is_call_insecure(call: &ast::ExprCall) -> bool {
    for (argument_name, position) in [("select", 0), ("where", 1), ("tables", 3)] {
        if let Some(argument) = call.arguments.find_argument_value(argument_name, position) {
            match argument_name {
                "select" => match argument {
                    Expr::Dict(dict) => {
                        if dict.iter().any(|ast::DictItem { key, value }| {
                            key.as_ref()
                                .is_some_and(|key| !key.is_string_literal_expr())
                                || !value.is_string_literal_expr()
                        }) {
                            return true;
                        }
                    }
                    _ => return true,
                },
                "where" | "tables" => match argument {
                    Expr::List(list) => {
                        if !list.iter().all(Expr::is_string_literal_expr) {
                            return true;
                        }
                    }
                    _ => return true,
                },
                _ => (),
            }
        }
    }

    false
}
