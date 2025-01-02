use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprAttribute};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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

/// AIR303
pub(crate) fn moved_to_provider_in_3(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    match expr {
        Expr::Attribute(ExprAttribute { attr: ranged, .. }) => {
            check_names_moved_to_provider(checker, expr, ranged);
        }
        ranged @ Expr::Name(_) => check_names_moved_to_provider(checker, expr, ranged),
        _ => {}
    }
}

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

fn check_names_moved_to_provider(checker: &mut Checker, expr: &Expr, ranged: impl Ranged) {
    let result =
        checker
            .semantic()
            .resolve_qualified_name(expr)
            .and_then(|qualname| match qualname.segments() {
                // apache-airflow-providers-amazon
                ["airflow", "hooks", "S3_hook", "S3Hook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.hooks.s3.S3Hook",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "hooks", "S3_hook", "provide_bucket_name"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.hooks.s3.provide_bucket_name",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "gcs_to_s3", "GCSToS3Operator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.transfers.gcs_to_s3.GCSToS3Operator",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "google_api_to_s3_transfer", "GoogleApiToS3Operator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.transfers.google_api_to_s3.GoogleApiToS3Operator",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "google_api_to_s3_transfer", "GoogleApiToS3Transfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.transfers.google_api_to_s3.GoogleApiToS3Operator",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "redshift_to_s3_operator", "RedshiftToS3Operator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.transfers.redshift_to_s3.RedshiftToS3Operator",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "redshift_to_s3_operator", "RedshiftToS3Transfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.transfers.redshift_to_s3.RedshiftToS3Operator",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "s3_file_transform_operator", "S3FileTransformOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.operators.s3_file_transform.S3FileTransformOperator",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "s3_to_redshift_operator", "S3ToRedshiftOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.transfers.s3_to_redshift.S3ToRedshiftOperator",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "s3_to_redshift_operator", "S3ToRedshiftTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.amazon.aws.transfers.s3_to_redshift.S3ToRedshiftOperator",
                        provider: "amazon",
                        version: "1.0.0"
                },
                )),
                ["airflow", "sensors", "s3_key_sensor", "S3KeySensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "S3KeySensor",
                        provider: "amazon",
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

                // apache-airflow-providers-common-sql
                ["airflow", "hooks", "dbapi", "ConnectorProtocol"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.hooks.dbapi.ConnectorProtocol",
                        new_path: "airflow.providers.common.sql.hooks.sql.ConnectorProtocol",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "hooks", "dbapi", "DbApiHook"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.hooks.dbapi.DbApiHook",
                        new_path: "airflow.providers.common.sql.hooks.sql.DbApiHook",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "hooks", "dbapi_hook", "DbApiHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.hooks.sql.DbApiHook",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "check_operator", "SQLCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "check_operator", "SQLIntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "check_operator", "SQLThresholdCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLThresholdCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "check_operator", "SQLValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "check_operator", "CheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "check_operator", "IntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "check_operator", "ThresholdCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLThresholdCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "check_operator", "ValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "presto_check_operator", "SQLCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "presto_check_operator", "SQLIntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "presto_check_operator", "SQLValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "presto_check_operator", "PrestoCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "presto_check_operator", "PrestoIntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "presto_check_operator", "PrestoValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "sql", "BaseSQLOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.BaseSQLOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "sql", "BranchSQLOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.BranchSQLOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "sql", "SQLCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "sql", "SQLColumnCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLColumnCheckOperator",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "sql", "SQLIntervalCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "sql", "SQLTablecheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "SQLTableCheckOperator",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "sql", "SQLThresholdCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLTableCheckOperator",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "sql", "SQLValueCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "sql", "_convert_to_float_if_possible"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql._convert_to_float_if_possible",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "sql", "parse_boolean"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.parse_boolean",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "sql_branch_operator", "BranchSQLOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.BranchSQLOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "operators", "sql_branch_operator", "BranchSqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.operators.sql.BranchSQLOperator",
                        provider: "common-sql",
                        version: "1.1.0"
                },
                )),
                ["airflow", "sensors", "sql", "SqlSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.sensors.sql.SqlSensor",
                        provider: "common-sql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "sensors", "sql_sensor", "SqlSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.common.sql.sensors.sql.SqlSensor",
                        provider: "common-sql",
                        version: "1.0.0"
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

                // apache-airflow-providers-docker
                ["airflow", "hooks", "docker_hook", "DockerHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.docker.hooks.docker.DockerHook",
                        provider: "docker",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "docker_operator", "DockerOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.docker.operators.docker.DockerOperator",
                        provider: "docker",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-apache-druid
                ["airflow", "hooks", "druid_hook", "DruidDbApiHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "DruidDbApiHook",
                        provider: "apache-druid",
                        version: "1.0.0"
                },
                )),
                ["airflow", "hooks", "druid_hook", "DruidHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "DruidHook",
                        provider: "apache-druid",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "druid_check_operator", "DruidCheckOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "DruidCheckOperator",
                        provider: "apache-druid",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "hive_to_druid", "HiveToDruidOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.druid.transfers.hive_to_druid.HiveToDruidOperator",
                        provider: "apache-druid",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "hive_to_druid", "HiveToDruidTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.druid.transfers.hive_to_druid.HiveToDruidOperator",
                        provider: "apache-druid",
                        version: "1.0.0"
                },
                )),


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

                // apache-airflow-providers-apache-hdfs
                ["airflow", "hooks", "webhdfs_hook", "WebHDFSHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hdfs.hooks.webhdfs.WebHDFSHook",
                        provider: "apache-hdfs",
                        version: "1.0.0"
                },
                )),
                ["airflow", "sensors", "web_hdfs_sensor", "WebHdfsSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hdfs.sensors.web_hdfs.WebHdfsSensor",
                        provider: "apache-hdfs",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-apache-hive
                ["airflow", "hooks", "hive_hooks", "HIVE_QUEUE_PRIORITIES"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.hooks.hive.HIVE_QUEUE_PRIORITIES",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "macros", "hive", "closest_ds_partition"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.macros.hive.closest_ds_partition",
                        provider: "apache-hive",
                        version: "5.1.0"
                },
                )),
                ["airflow", "macros", "hive", "max_partition"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.macros.hive.max_partition",
                        provider: "apache-hive",
                        version: "5.1.0"
                },
                )),
                ["airflow", "operators", "hive_to_mysql", "HiveToMySqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.transfers.hive_to_mysql.HiveToMySqlOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "hive_to_mysql", "HiveToMySqlTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.transfers.hive_to_mysql.HiveToMySqlOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "hive_to_samba_operator", "HiveToSambaOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "HiveToSambaOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "mssql_to_hive", "MsSqlToHiveOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.transfers.mssql_to_hive.MsSqlToHiveOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "mssql_to_hive", "MsSqlToHiveTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.transfers.mssql_to_hive.MsSqlToHiveOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "mysql_to_hive", "MySqlToHiveOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.transfers.mysql_to_hive.MySqlToHiveOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "mysql_to_hive", "MySqlToHiveTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.transfers.mysql_to_hive.MySqlToHiveOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "s3_to_hive_operator", "S3ToHiveOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.transfers.s3_to_hive.S3ToHiveOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "s3_to_hive_operator", "S3ToHiveTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.transfers.s3_to_hive.S3ToHiveOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "hooks", "hive_hooks", "HiveCliHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.hooks.hive.HiveCliHook",
                        provider: "apache-hive",
                        version: "1.0.0"
                    },
                )),
                ["airflow", "hooks", "hive_hooks", "HiveMetastoreHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.hooks.hive.HiveMetastoreHook",
                        provider: "apache-hive",
                        version: "1.0.0"
                    },
                )),

                ["airflow", "hooks", "hive_hooks", "HiveServer2Hook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.hooks.hive.HiveServer2Hook",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "hive_operator", "HiveOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.operators.hive.HiveOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "hive_stats_operator", "HiveStatsCollectionOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.operators.hive_stats.HiveStatsCollectionOperator",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "sensors", "hive_partition_sensor", "HivePartitionSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.sensors.hive_partition.HivePartitionSensor",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "sensors", "metastore_partition_sensor", "MetastorePartitionSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.sensors.metastore_partition.MetastorePartitionSensor",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),
                ["airflow", "sensors", "named_hive_partition_sensor", "NamedHivePartitionSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.hive.sensors.named_hive_partition.NamedHivePartitionSensor",
                        provider: "apache-hive",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-http
                ["airflow", "hooks", "http_hook", "HttpHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.http.hooks.http.HttpHook",
                        provider: "http",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "http_operator", "SimpleHttpOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.http.operators.http.SimpleHttpOperator",
                        provider: "http",
                        version: "1.0.0"
                },
                )),
                ["airflow", "sensors", "http_sensor", "HttpSensor"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.http.sensors.http.HttpSensor",
                        provider: "http",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-jdbc
                ["airflow", "hooks", "jdbc_hook", "JdbcHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.jdbc.hooks.jdbc.JdbcHook",
                        provider: "jdbc",
                        version: "1.0.0"
                },
                )),
                ["airflow", "hooks", "jdbc_hook", "jaydebeapi"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.jdbc.hooks.jdbc.jaydebeapi",
                        provider: "jdbc",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "jdbc_operator", "JdbcOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.jdbc.operators.jdbc.JdbcOperator",
                        provider: "jdbc",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-cncf-kubernetes
                ["airflow", "executors", "kubernetes_executor_types", "ALL_NAMESPACES"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.executors.kubernetes_executor_types.ALL_NAMESPACES",
                        new_path: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types.ALL_NAMESPACES",
                        provider: "cncf-kubernetes",
                        version: "7.4.0"
                },
                )),
                ["airflow", "executors", "kubernetes_executor_types", "POD_EXECUTOR_DONE_KEY"] => Some((
                    qualname.to_string(),
                    Replacement::ImportPathMoved{
                        original_path: "airflow.executors.kubernetes_executor_types.POD_EXECUTOR_DONE_KEY",
                        new_path: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types.POD_EXECUTOR_DONE_KEY",
                        provider: "cncf-kubernetes",
                        version: "7.4.0"
                },
                )),


                // apache-airflow-providers-microsoft-mssql
                ["airflow", "hooks", "mssql_hook", "MsSqlHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.microsoft.mssql.hooks.mssql.MsSqlHook",
                        provider: "microsoft-mssql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "mssql_operator", "MsSqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.microsoft.mssql.operators.mssql.MsSqlOperator",
                        provider: "microsoft-mssql",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-mysql
                ["airflow", "hooks", "mysql_hook", "MySqlHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.mysql.hooks.mysql.MySqlHook",
                        provider: "mysql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "mysql_operator", "MySqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.mysql.operators.mysql.MySqlOperator",
                        provider: "mysql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "presto_to_mysql", "PrestoToMySqlOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.mysql.transfers.presto_to_mysql.PrestoToMySqlOperator",
                        provider: "mysql",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "presto_to_mysql", "PrestoToMySqlTransfer"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.mysql.transfers.presto_to_mysql.PrestoToMySqlOperator",
                        provider: "mysql",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-oracle
                ["airflow", "hooks", "oracle_hook", "OracleHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.oracle.hooks.oracle.OracleHook",
                        provider: "oracle",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "oracle_operator", "OracleOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.oracle.operators.oracle.OracleOperator",
                        provider: "oracle",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-papermill
                ["airflow", "operators", "papermill_operator", "PapermillOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.papermill.operators.papermill.PapermillOperator",
                        provider: "papermill",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-apache-pig
                ["airflow", "hooks", "pig_hook", "PigCliHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.pig.hooks.pig.PigCliHook",
                        provider: "apache-pig",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "pig_operator", "PigOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.apache.pig.operators.pig.PigOperator",
                        provider: "apache-pig",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-postgres
                ["airflow", "hooks", "postgres_hook", "PostgresHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.postgres.hooks.postgres.PostgresHook",
                        provider: "postgres",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "postgres_operator", "Mapping"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.postgres.operators.postgres.Mapping",
                        provider: "postgres",
                        version: "1.0.0"
                },
                )),

                ["airflow", "operators", "postgres_operator", "PostgresOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.postgres.operators.postgres.PostgresOperator",
                        provider: "postgres",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-presto
                ["airflow", "hooks", "presto_hook", "PrestoHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.presto.hooks.presto.PrestoHook",
                        provider: "presto",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-samba
                ["airflow", "hooks", "samba_hook", "SambaHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.samba.hooks.samba.SambaHook",
                        provider: "samba",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-slack
                ["airflow", "hooks", "slack_hook", "SlackHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.slack.hooks.slack.SlackHook",
                        provider: "slack",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "slack_operator", "SlackAPIOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.slack.operators.slack.SlackAPIOperator",
                        provider: "slack",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "slack_operator", "SlackAPIPostOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.slack.operators.slack.SlackAPIPostOperator",
                        provider: "slack",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-sqlite
                ["airflow", "hooks", "sqlite_hook", "SqliteHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.sqlite.hooks.sqlite.SqliteHook",
                        provider: "sqlite",
                        version: "1.0.0"
                },
                )),
                ["airflow", "operators", "sqlite_operator", "SqliteOperator"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.sqlite.operators.sqlite.SqliteOperator",
                        provider: "sqlite",
                        version: "1.0.0"
                },
                )),

                // apache-airflow-providers-zendesk
                ["airflow", "hooks", "zendesk_hook", "ZendeskHook"] => Some((
                    qualname.to_string(),
                    Replacement::ProviderName{
                        name: "airflow.providers.zendesk.hooks.zendesk.ZendeskHook",
                        provider: "zendesk",
                        version: "1.0.0"
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
