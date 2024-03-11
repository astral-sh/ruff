use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprAttribute, ExprDict, ExprList};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of Django's `extra` function.
///
/// ## Why is this bad?
/// Django's `extra` function can be used to execute arbitrary SQL queries,
/// which can in turn lead to SQL injection vulnerabilities.
///
/// ## Example
/// ```python
/// from django.contrib.auth.models import User
///
/// User.objects.all().extra(select={"test": "%secure" % "nos"})
/// ```
///
/// ## References
/// - [Django documentation: SQL injection protection](https://docs.djangoproject.com/en/dev/topics/security/#sql-injection-protection)
/// - [Common Weakness Enumeration: CWE-89](https://cwe.mitre.org/data/definitions/89.html)
#[violation]
pub struct DjangoExtra;

impl Violation for DjangoExtra {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of Django `extra` can lead to SQL injection vulnerabilities")
    }
}

/// S610
pub(crate) fn django_extra(checker: &mut Checker, call: &ast::ExprCall) {
    let Expr::Attribute(ExprAttribute { attr, .. }) = call.func.as_ref() else {
        return;
    };

    if attr.as_str() != "extra" {
        return;
    }

    if is_call_insecure(call) {
        checker
            .diagnostics
            .push(Diagnostic::new(DjangoExtra, call.arguments.range()));
    }
}

fn is_call_insecure(call: &ast::ExprCall) -> bool {
    for (argument_name, position) in [("select", 0), ("where", 1), ("tables", 3)] {
        if let Some(argument) = call.arguments.find_argument(argument_name, position) {
            match argument_name {
                "select" => match argument {
                    Expr::Dict(ExprDict { keys, values, .. }) => {
                        if !keys.iter().flatten().all(Expr::is_string_literal_expr) {
                            return true;
                        }
                        if !values.iter().all(Expr::is_string_literal_expr) {
                            return true;
                        }
                    }
                    _ => return true,
                },
                "where" | "tables" => match argument {
                    Expr::List(ExprList { elts, .. }) => {
                        if !elts.iter().all(Expr::is_string_literal_expr) {
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
