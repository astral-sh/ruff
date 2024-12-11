use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprAttribute};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

#[derive(Debug, Eq, PartialEq)]
enum Replacement {
    ProviderName(String, String, String),
    ImportPathMoved(String, String, String, String),
}

/// ## What it does
/// Checks for uses of Airflow functions and values that have been moved to it providers.
/// (e.g., apache-airflow-providers-fab)
///
/// ## Why is this bad?
/// Airflow 3.0 moved various deprecated functions, members, and other
/// values to its providers. The user needs to install the corresponding provider and replace
/// the original usage with the one in the provider
///
/// ## Example
/// ```python
/// from airflow.auth.managers.fab.fab_auth_manage import FabAuthManager
/// ```
///
/// Use instead:
/// ```python
/// from airflow.providers.fab.auth_manager.fab_auth_manage import FabAuthManager
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct Airflow3MovedToProvider {
    deprecated: String,
    replacement: Replacement,
}

impl Violation for Airflow3MovedToProvider {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3MovedToProvider {
            deprecated,
            replacement,
        } = self;
        match replacement {
            Replacement::ProviderName(name, provider_name, provider_version) => {
                format!("`{deprecated}` is moved into `{provider_name}` provider in Airflow 3.0;
                        Install `apache-airflow-provider-{provider_name}=={provider_version}` and use `{name}` instead.")
            }
            Replacement::ImportPathMoved(
                original_path,
                new_path,
                provider_name,
                provider_version,
            ) => {
                format!("Import path `{original_path}` is moved into `{provider_name}` provider in Airflow 3.0;
                        Install `apache-airflow-provider-{provider_name}=={provider_version}` and import from `{new_path}` instead.")
            }
        }
    }
}

fn moved_to_provider(checker: &mut Checker, expr: &Expr, ranged: impl Ranged) {
    let result =
        checker
            .semantic()
            .resolve_qualified_name(expr)
            .and_then(|qualname| match qualname.segments() {
                // apache-airflow-providers-fab
                ["airflow", "www", "security", "FabAirflowSecurityManagerOverride"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName(
                        "airflow.providers.fab.auth_manager.security_manager.override.FabAirflowSecurityManagerOverride".to_string(),
                        "fab".to_string(),
                        "1.0.0".to_string()
                    ),
                )),
                ["airflow","auth","managers","fab","fab_auth_manager", "FabAuthManager"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName(
                        "airflow.providers.fab.auth_manager.security_manager.FabAuthManager".to_string(),
                        "fab".to_string(),
                        "1.0.0".to_string()
                    ),
                )),
                ["airflow", "api", "auth", "backend", "basic_auth", ..] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved(
                        "airflow.api.auth.backend.basic_auth".to_string(),
                        "airflow.providers.fab.auth_manager.api.auth.backend.basic_auth".to_string(),
                        "fab".to_string(),
                        "1.0.0".to_string()
                    ),
                )),
                ["airflow", "api","auth","backend","kerberos_auth", ..] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved(
                        "airflow.api.auth.backend.kerberos_auth".to_string(),
                        "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth".to_string(),
                        "fab".to_string(),
                        "1.0.0".to_string()
                    ),
                )),
                ["airflow", "auth", "managers", "fab", "api", "auth", "backend", "kerberos_auth", ..] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved(
                        "airflow.auth_manager.api.auth.backend.kerberos_auth".to_string(),
                        "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth".to_string(),
                        "fab".to_string(),
                        "1.0.0".to_string()
                    ),
                )),
                ["airflow","auth","managers","fab","security_manager","override", ..] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved(
                        "airflow.auth.managers.fab.security_manager.override".to_string(),
                        "airflow.providers.fab.auth_manager.security_manager.override".to_string(),
                        "fab".to_string(),
                        "1.0.0".to_string()
                    ),
                )),

                _ => None,
            });
    if let Some((deprecated, replacement)) = result {
        checker.diagnostics.push(Diagnostic::new(
            Airflow3MovedToProvider {
                deprecated,
                replacement,
            },
            ranged.range(),
        ));
    }
}

/// AIR303
pub(crate) fn moved_to_provider_in_3(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    match expr {
        Expr::Attribute(ExprAttribute { attr: ranged, .. }) => {
            moved_to_provider(checker, expr, ranged);
        }
        ranged @ Expr::Name(_) => moved_to_provider(checker, expr, ranged),
        _ => {}
    }
}
