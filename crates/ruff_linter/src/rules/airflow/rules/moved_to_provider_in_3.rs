use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprAttribute};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

#[derive(Debug, Eq, PartialEq)]
enum Replacement {
    ProviderName {
        name: &'static str,
        provider: &'static str,
        version: &'static str,
    },
    ImportPathMoved {
        original_path: &'static str,
        new_path: &'static str,
        provider: &'static str,
        version: &'static str,
    },
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
            Replacement::ProviderName {
                name: _,
                provider,
                version: _,
            } => {
                format!("`{deprecated}` is moved into `{provider}` provider in Airflow 3.0;")
            }
            Replacement::ImportPathMoved {
                original_path,
                new_path: _,
                provider,
                version: _,
            } => {
                format!("Import path `{original_path}` is moved into `{provider}` provider in Airflow 3.0;")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3MovedToProvider { replacement, .. } = self;
        if let Replacement::ProviderName {
            name,
            provider,
            version,
        } = replacement
        {
            Some(format!(
                "Install `apache-airflow-provider-{provider}>={version}` and use `{name}` instead."
            ))
        } else if let Replacement::ImportPathMoved {
            original_path: _,
            new_path,
            provider,
            version,
        } = replacement
        {
            Some(format!("Install `apache-airflow-provider-{provider}>={version}` and import from `{new_path}` instead."))
        } else {
            None
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
                    Replacement::ProviderName {
                        name: "airflow.providers.fab.auth_manager.security_manager.override.FabAirflowSecurityManagerOverride",
                        provider: "fab",
                        version: "1.0.0"
                    },
                )),
                ["airflow", "auth", "managers", "fab", "fab_auth_manager", "FabAuthManager"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.fab.auth_manager.security_manager.FabAuthManager",
                        provider: "fab",
                        version: "1.0.0"
                },
                )),
                ["airflow", "api", "auth", "backend", "basic_auth", ..] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.api.auth.backend.basic_auth",
                        new_path: "airflow.providers.fab.auth_manager.api.auth.backend.basic_auth",
                        provider:"fab",
                        version: "1.0.0"
                },
                )),
                ["airflow", "api", "auth", "backend", "kerberos_auth", ..] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path:"airflow.api.auth.backend.kerberos_auth",
                        new_path: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
                        provider: "fab",
                        version:"1.0.0"
                },
                )),
                ["airflow", "auth", "managers", "fab", "api", "auth", "backend", "kerberos_auth", ..] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.auth_manager.api.auth.backend.kerberos_auth",
                        new_path: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
                        provider: "fab",
                        version: "1.0.0"
                },
                )),
                ["airflow", "auth", "managers", "fab", "security_manager", "override", ..] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.auth.managers.fab.security_managr.override",
                        new_path: "airflow.providers.fab.auth_manager.security_manager.override",
                        provider: "fab",
                        version: "1.0.0"
                },
                )),
                // apache-airflow-providers-celery
                ["airflow", "config_templates", "default_celery", "DEFAULT_CELERY_CONFIG"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.config_templates.default_celery.DEFAULT_CELERY_CONFIG",
                        new_path: "airflow.providers.celery.executors.default_celery.DEFAULT_CELERY_CONFIG",
                        provider: "celery",
                        version: "3.3.0"
                },
                )),
                ["airflow", "executors", "celery_executor", "app"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.executors.celery_executor.app",
                        new_path: "airflow.providers.celery.executors.celery_executor_utils.app",
                        provider: "celery",
                        version: "3.3.0"
                },
                )),
                ["airflow", "executors", "celery_executor", "CeleryExecutor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.celery.executors.celery_executor.CeleryExecutor",
                        provider: "celery",
                        version: "3.3.0"
                },
                )),
                ["airflow", "executors", "celery_kubernetes_executor", "CeleryKubernetesExecutor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.celery.executors.celery_kubernetes_executor.CeleryKubernetesExecutor",
                        provider: "celery",
                        version: "3.3.0"
                },
                )),
                // apache-airflow-providers-daskexecutor
                ["airflow", "executors", "dask_executor", "DaskExecutor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.daskexecutor.executors.dask_executor.DaskExecutor",
                        provider: "daskexecutor",
                        version: "1.0.0"
                },
                )),
                // apache-airflow-providers-common-sql
                ["airflow", "hooks", "dbapi", "ConnectorProtocol"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.hooks.dbapi.ConnectorProtocol",
                        new_path: "airflow.providers.common.sql.hooks.sql.ConnectorProtocol",
                        provider: "Common SQL",
                        version: "1.0.0"
                },
                )),
                ["airflow", "hooks", "dbapi", "DbApiHook"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.hooks.dbapi.DbApiHook",
                        new_path: "airflow.providers.common.sql.hooks.sql.DbApiHook",
                        provider: "Common SQL",
                        version: "1.0.0"
                },
                )),
                // apache-airflow-providers-cncf-kubernetes
                ["airflow", "executors", "kubernetes_executor_types", "ALL_NAMESPACES"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.executors.kubernetes_executor_types.ALL_NAMESPACES",
                        new_path: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types.ALL_NAMESPACES",
                        provider: "Kubernetes",
                        version: "7.4.0"
                },
                )),
                ["airflow", "executors", "kubernetes_executor_types", "POD_EXECUTOR_DONE_KEY"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.executors.kubernetes_executor_types.POD_EXECUTOR_DONE_KEY",
                        new_path: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types.POD_EXECUTOR_DONE_KEY",
                        provider: "Kubernetes",
                        version: "7.4.0"
                },
                )),
                // apache-airflow-providers-apache-hive
                ["airflow", "hooks", "hive_hooks", "HIVE_QUEUE_PRIORITIES"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HIVE_QUEUE_PRIORITIES",
                        provider: "Apache Hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "macros", "hive", "closest_ds_partition"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.macros.hive.closest_ds_partition",
                        new_path: "airflow.providers.apache.hive.macros.hive.closest_ds_partition",
                        provider: "Apache Hive",
                        version: "5.1.0"
                },
                )),
                ["airflow", "macros", "hive", "max_partition"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.macros.hive.max_partition",
                        new_path: "airflow.providers.apache.hive.macros.hive.max_partition",
                        provider: "Apache Hive",
                        version: "5.1.0"
                },
                )),

                // TODO: reorganize
                ["airflow", "hooks", "S3_hook", "S3Hook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "S3Hook",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "S3_hook", "provide_bucket_name"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "provide_bucket_name",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "base_hook", "BaseHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "BaseHook",
                        provider: "Base",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "dbapi_hook", "DbApiHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "DbApiHook",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "docker_hook", "DockerHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "DockerHook",
                        provider: "Docker",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "druid_hook", "DruidDbApiHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "DruidDbApiHook",
                        provider: "Apache Druid",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "druid_hook", "DruidHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "DruidHook",
                        provider: "Apache Druid",
                        version: "TBD"
                },
                )),


                ["airflow", "hooks", "hive_hooks", "HiveCliHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveCliHook",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "hive_hooks", "HiveMetastoreHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveMetastoreHook",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "hive_hooks", "HiveServer2Hook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveServer2Hook",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "http_hook", "HttpHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HttpHook",
                        provider: "Http",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "jdbc_hook", "JdbcHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "JdbcHook",
                        provider: "Jdbc",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "jdbc_hook", "jaydebeapi"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "jaydebeapi",
                        provider: "Jdbc",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "mssql_hook", "MsSqlHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "MsSqlHook",
                        provider: "Microsoft",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "mysql_hook", "MySqlHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "MySqlHook",
                        provider: "Mysql",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "oracle_hook", "OracleHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "OracleHook",
                        provider: "Oracle",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "pig_hook", "PigCliHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PigCliHook",
                        provider: "Apache Pig",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "postgres_hook", "PostgresHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PostgresHook",
                        provider: "Postgres",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "presto_hook", "PrestoHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PrestoHook",
                        provider: "Presto",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "samba_hook", "SambaHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SambaHook",
                        provider: "Samba",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "slack_hook", "SlackHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SlackHook",
                        provider: "Slack",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "sqlite_hook", "SqliteHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SqliteHook",
                        provider: "Sqlite",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "webhdfs_hook", "WebHDFSHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "WebHDFSHook",
                        provider: "Apache Hdfs",
                        version: "TBD"
                },
                )),

                ["airflow", "hooks", "zendesk_hook", "ZendeskHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "ZendeskHook",
                        provider: "Zendesk",
                        version: "TBD"
                },
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
