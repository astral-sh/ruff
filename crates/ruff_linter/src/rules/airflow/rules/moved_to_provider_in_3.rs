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

                ["airflow", "operators", "check_operator", "SQLCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "check_operator", "SQLIntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLIntervalCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "check_operator", "SQLThresholdCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLThresholdCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "check_operator", "SQLValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLValueCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "check_operator", "CheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "CheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "check_operator", "IntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "IntervalCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "check_operator", "ThresholdCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "ThresholdCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "check_operator", "ValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "ValueCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "docker_operator", "DockerOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "DockerOperator",
                        provider: "Docker",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "druid_check_operator", "DruidCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "DruidCheckOperator",
                        provider: "Apache Druid",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "gcs_to_s3", "GCSToS3Operator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "GCSToS3Operator",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "google_api_to_s3_transfer", "GoogleApiToS3Operator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "GoogleApiToS3Operator",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "google_api_to_s3_transfer", "GoogleApiToS3Transfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "GoogleApiToS3Transfer",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "hive_operator", "HiveOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveOperator",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "hive_stats_operator", "HiveStatsCollectionOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveStatsCollectionOperator",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "hive_to_druid", "HiveToDruidOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveToDruidOperator",
                        provider: "Apache Druid",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "hive_to_druid", "HiveToDruidTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveToDruidTransfer",
                        provider: "Apache Druid",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "hive_to_mysql", "HiveToMySqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveToMySqlOperator",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "hive_to_mysql", "HiveToMySqlTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveToMySqlTransfer",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "hive_to_samba_operator", "HiveToSambaOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveToSambaOperator",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "http_operator", "SimpleHttpOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SimpleHttpOperator",
                        provider: "Http",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "jdbc_operator", "JdbcOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "JdbcOperator",
                        provider: "Jdbc",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "latest_only_operator", "LatestOnlyOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "LatestOnlyOperator",
                        provider: "Latest_only",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "mssql_operator", "MsSqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "MsSqlOperator",
                        provider: "Microsoft",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "mssql_to_hive", "MsSqlToHiveOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "MsSqlToHiveOperator",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "mssql_to_hive", "MsSqlToHiveTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "MsSqlToHiveTransfer",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "mysql_operator", "MySqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "MySqlOperator",
                        provider: "Mysql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "mysql_to_hive", "MySqlToHiveOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "MySqlToHiveOperator",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "mysql_to_hive", "MySqlToHiveTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "MySqlToHiveTransfer",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "oracle_operator", "OracleOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "OracleOperator",
                        provider: "Oracle",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "papermill_operator", "PapermillOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PapermillOperator",
                        provider: "Papermill",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "pig_operator", "PigOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PigOperator",
                        provider: "Apache Pig",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "postgres_operator", "Mapping"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "Mapping",
                        provider: "Postgres",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "postgres_operator", "PostgresOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PostgresOperator",
                        provider: "Postgres",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "presto_check_operator", "SQLCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "presto_check_operator", "SQLIntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLIntervalCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "presto_check_operator", "SQLValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLValueCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "presto_check_operator", "PrestoCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PrestoCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "presto_check_operator", "PrestoIntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PrestoIntervalCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "presto_check_operator", "PrestoValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PrestoValueCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "presto_to_mysql", "PrestoToMySqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PrestoToMySqlOperator",
                        provider: "Mysql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "presto_to_mysql", "PrestoToMySqlTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "PrestoToMySqlTransfer",
                        provider: "Mysql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "redshift_to_s3_operator", "RedshiftToS3Operator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "RedshiftToS3Operator",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "redshift_to_s3_operator", "RedshiftToS3Transfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "RedshiftToS3Transfer",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "s3_file_transform_operator", "S3FileTransformOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "S3FileTransformOperator",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "s3_to_hive_operator", "S3ToHiveOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "S3ToHiveOperator",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "s3_to_hive_operator", "S3ToHiveTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "S3ToHiveTransfer",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "s3_to_redshift_operator", "S3ToRedshiftOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "S3ToRedshiftOperator",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "s3_to_redshift_operator", "S3ToRedshiftTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "S3ToRedshiftTransfer",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "slack_operator", "SlackAPIOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SlackAPIOperator",
                        provider: "Slack",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "slack_operator", "SlackAPIPostOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SlackAPIPostOperator",
                        provider: "Slack",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "BaseSQLOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "BaseSQLOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "BranchSQLOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "BranchSQLOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "SQLCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "SQLColumnCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLColumnCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "SQLIntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLIntervalCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "SQLTableCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLTableCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "SQLThresholdCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLThresholdCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "SQLValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLValueCheckOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "_convert_to_float_if_possible"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "_convert_to_float_if_possible",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql", "parse_boolean"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "parse_boolean",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql_branch_operator", "BranchSQLOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "BranchSQLOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sql_branch_operator", "BranchSqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "BranchSqlOperator",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "operators", "sqlite_operator", "SqliteOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SqliteOperator",
                        provider: "Sqlite",
                        version: "TBD"
                },
                )),

                ["airflow", "sensors", "hive_partition_sensor", "HivePartitionSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HivePartitionSensor",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "sensors", "http_sensor", "HttpSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HttpSensor",
                        provider: "Http",
                        version: "TBD"
                },
                )),

                ["airflow", "sensors", "metastore_partition_sensor", "MetastorePartitionSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "MetastorePartitionSensor",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "sensors", "named_hive_partition_sensor", "NamedHivePartitionSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "NamedHivePartitionSensor",
                        provider: "Apache Hive",
                        version: "TBD"
                },
                )),

                ["airflow", "sensors", "s3_key_sensor", "S3KeySensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "S3KeySensor",
                        provider: "Amazon",
                        version: "TBD"
                },
                )),

                ["airflow", "sensors", "sql", "SqlSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SqlSensor",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "sensors", "sql_sensor", "SqlSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SqlSensor",
                        provider: "Common Sql",
                        version: "TBD"
                },
                )),

                ["airflow", "sensors", "web_hdfs_sensor", "WebHdfsSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "WebHdfsSensor",
                        provider: "Apache Hdfs",
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
