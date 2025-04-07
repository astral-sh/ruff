use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprAttribute};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

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

/// AIR302
pub(crate) fn moved_to_provider_in_3(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    match expr {
        Expr::Attribute(ExprAttribute { attr, .. }) => {
            check_names_moved_to_provider(checker, expr, attr.range());
        }
        ranged @ Expr::Name(_) => check_names_moved_to_provider(checker, expr, ranged.range()),
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

fn check_names_moved_to_provider(checker: &Checker, expr: &Expr, ranged: TextRange) {
    let Some(qualified_name) = checker.semantic().resolve_qualified_name(expr) else {
        return;
    };

    let replacement = match qualified_name.segments() {
        // ProviderName: for cases that only one name has been moved
        // apache-airflow-providers-amazon
        ["airflow", "hooks", "S3_hook", "S3Hook"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.hooks.s3.S3Hook",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "hooks", "S3_hook", "provide_bucket_name"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.hooks.s3.provide_bucket_name",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "gcs_to_s3", "GCSToS3Operator"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.transfers.gcs_to_s3.GCSToS3Operator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "google_api_to_s3_transfer", "GoogleApiToS3Operator"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.transfers.google_api_to_s3.GoogleApiToS3Operator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "google_api_to_s3_transfer", "GoogleApiToS3Transfer"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.transfers.google_api_to_s3.GoogleApiToS3Operator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "redshift_to_s3_operator", "RedshiftToS3Operator"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.transfers.redshift_to_s3.RedshiftToS3Operator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "redshift_to_s3_operator", "RedshiftToS3Transfer"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.transfers.redshift_to_s3.RedshiftToS3Operator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "s3_file_transform_operator", "S3FileTransformOperator"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.operators.s3_file_transform.S3FileTransformOperator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "s3_to_redshift_operator", "S3ToRedshiftOperator"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.transfers.s3_to_redshift.S3ToRedshiftOperator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "s3_to_redshift_operator", "S3ToRedshiftTransfer"] => Replacement::ProviderName{
            name: "airflow.providers.amazon.aws.transfers.s3_to_redshift.S3ToRedshiftOperator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "sensors", "s3_key_sensor", "S3KeySensor"] => Replacement::ProviderName{
            name: "S3KeySensor",
            provider: "amazon",
            version: "1.0.0"
        },

        // apache-airflow-providers-celery
        ["airflow", "config_templates", "default_celery", "DEFAULT_CELERY_CONFIG"] => Replacement::ProviderName{
            name: "airflow.providers.celery.executors.default_celery.DEFAULT_CELERY_CONFIG",
            provider: "celery",
            version: "3.3.0"
        },
        ["airflow", "executors", "celery_executor", "app"] => Replacement::ProviderName{
            name: "airflow.providers.celery.executors.celery_executor_utils.app",
            provider: "celery",
            version: "3.3.0"
        },
        ["airflow", "executors", "celery_executor", "CeleryExecutor"] => Replacement::ProviderName{
            name: "airflow.providers.celery.executors.celery_executor.CeleryExecutor",
            provider: "celery",
            version: "3.3.0"
        },
        ["airflow", "executors", "celery_kubernetes_executor", "CeleryKubernetesExecutor"] => Replacement::ProviderName{
            name: "airflow.providers.celery.executors.celery_kubernetes_executor.CeleryKubernetesExecutor",
            provider: "celery",
            version: "3.3.0"
        },

        // apache-airflow-providers-common-sql
        ["airflow", "hooks", "dbapi", "ConnectorProtocol"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.hooks.sql.ConnectorProtocol",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "hooks", "dbapi", "DbApiHook"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.hooks.sql.DbApiHook",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "hooks", "dbapi_hook", "DbApiHook"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.hooks.sql.DbApiHook",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "operators", "check_operator", "SQLCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "check_operator", "SQLIntervalCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "check_operator", "SQLThresholdCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLThresholdCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "check_operator", "SQLValueCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "check_operator", "CheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "check_operator", "IntervalCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "check_operator", "ThresholdCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLThresholdCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "check_operator", "ValueCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "presto_check_operator", "SQLCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "presto_check_operator", "SQLIntervalCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "presto_check_operator", "SQLValueCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "presto_check_operator", "PrestoCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "presto_check_operator", "PrestoIntervalCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "presto_check_operator", "PrestoValueCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "sql", "BaseSQLOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.BaseSQLOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "sql", "BranchSQLOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.BranchSQLOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "sql", "SQLCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "sql", "SQLColumnCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLColumnCheckOperator",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "operators", "sql", "SQLIntervalCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "sql", "SQLTablecheckOperator"] => Replacement::ProviderName{
            name: "SQLTableCheckOperator",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "operators", "sql", "SQLThresholdCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLTableCheckOperator",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "operators", "sql", "SQLValueCheckOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "operators", "sql", "_convert_to_float_if_possible"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql._convert_to_float_if_possible",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "operators", "sql", "parse_boolean"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.parse_boolean",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "operators", "sql_branch_operator", "BranchSQLOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.BranchSQLOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "operators", "sql_branch_operator", "BranchSqlOperator"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.operators.sql.BranchSQLOperator",
            provider: "common-sql",
            version: "1.1.0"
        },
        ["airflow", "sensors", "sql", "SqlSensor"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.sensors.sql.SqlSensor",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "sensors", "sql_sensor", "SqlSensor"] => Replacement::ProviderName{
            name: "airflow.providers.common.sql.sensors.sql.SqlSensor",
            provider: "common-sql",
            version: "1.0.0"
        },

        // apache-airflow-providers-daskexecutor
        ["airflow", "executors", "dask_executor", "DaskExecutor"] => Replacement::ProviderName{
            name: "airflow.providers.daskexecutor.executors.dask_executor.DaskExecutor",
            provider: "daskexecutor",
            version: "1.0.0"
        },

        // apache-airflow-providers-docker
        ["airflow", "hooks", "docker_hook", "DockerHook"] => Replacement::ProviderName{
            name: "airflow.providers.docker.hooks.docker.DockerHook",
            provider: "docker",
            version: "1.0.0"
        },
        ["airflow", "operators", "docker_operator", "DockerOperator"] => Replacement::ProviderName{
            name: "airflow.providers.docker.operators.docker.DockerOperator",
            provider: "docker",
            version: "1.0.0"
        },

        // apache-airflow-providers-apache-druid
        ["airflow", "hooks", "druid_hook", "DruidDbApiHook"] => Replacement::ProviderName{
            name: "DruidDbApiHook",
            provider: "apache-druid",
            version: "1.0.0"
        },
        ["airflow", "hooks", "druid_hook", "DruidHook"] => Replacement::ProviderName{
            name: "DruidHook",
            provider: "apache-druid",
            version: "1.0.0"
        },
        ["airflow", "operators", "druid_check_operator", "DruidCheckOperator"] => Replacement::ProviderName{
            name: "DruidCheckOperator",
            provider: "apache-druid",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_to_druid", "HiveToDruidOperator"] => Replacement::ProviderName{
            name: "airflow.providers.apache.druid.transfers.hive_to_druid.HiveToDruidOperator",
            provider: "apache-druid",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_to_druid", "HiveToDruidTransfer"] => Replacement::ProviderName{
            name: "airflow.providers.apache.druid.transfers.hive_to_druid.HiveToDruidOperator",
            provider: "apache-druid",
            version: "1.0.0"
        },


        // apache-airflow-providers-fab
        ["airflow", "www", "security", "FabAirflowSecurityManagerOverride"] => Replacement::ProviderName {
            name: "airflow.providers.fab.auth_manager.security_manager.override.FabAirflowSecurityManagerOverride",
            provider: "fab",
            version: "1.0.0"
            },
        ["airflow", "auth", "managers", "fab", "fab_auth_manager", "FabAuthManager"] => Replacement::ProviderName{
            name: "airflow.providers.fab.auth_manager.security_manager.FabAuthManager",
            provider: "fab",
            version: "1.0.0"
        },

        // apache-airflow-providers-apache-hdfs
        ["airflow", "hooks", "webhdfs_hook", "WebHDFSHook"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hdfs.hooks.webhdfs.WebHDFSHook",
            provider: "apache-hdfs",
            version: "1.0.0"
        },
        ["airflow", "sensors", "web_hdfs_sensor", "WebHdfsSensor"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hdfs.sensors.web_hdfs.WebHdfsSensor",
            provider: "apache-hdfs",
            version: "1.0.0"
        },

        // apache-airflow-providers-apache-hive
        ["airflow", "hooks", "hive_hooks", "HIVE_QUEUE_PRIORITIES"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.hooks.hive.HIVE_QUEUE_PRIORITIES",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "macros", "hive", "closest_ds_partition"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.macros.hive.closest_ds_partition",
            provider: "apache-hive",
            version: "5.1.0"
        },
        ["airflow", "macros", "hive", "max_partition"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.macros.hive.max_partition",
            provider: "apache-hive",
            version: "5.1.0"
        },
        ["airflow", "operators", "hive_to_mysql", "HiveToMySqlOperator"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.transfers.hive_to_mysql.HiveToMySqlOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_to_mysql", "HiveToMySqlTransfer"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.transfers.hive_to_mysql.HiveToMySqlOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_to_samba_operator", "HiveToSambaOperator"] => Replacement::ProviderName{
            name: "HiveToSambaOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "mssql_to_hive", "MsSqlToHiveOperator"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.transfers.mssql_to_hive.MsSqlToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "mssql_to_hive", "MsSqlToHiveTransfer"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.transfers.mssql_to_hive.MsSqlToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "mysql_to_hive", "MySqlToHiveOperator"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.transfers.mysql_to_hive.MySqlToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "mysql_to_hive", "MySqlToHiveTransfer"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.transfers.mysql_to_hive.MySqlToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "s3_to_hive_operator", "S3ToHiveOperator"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.transfers.s3_to_hive.S3ToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "s3_to_hive_operator", "S3ToHiveTransfer"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.transfers.s3_to_hive.S3ToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "hooks", "hive_hooks", "HiveCliHook"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.hooks.hive.HiveCliHook",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "hooks", "hive_hooks", "HiveMetastoreHook"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.hooks.hive.HiveMetastoreHook",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "hooks", "hive_hooks", "HiveServer2Hook"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.hooks.hive.HiveServer2Hook",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_operator", "HiveOperator"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.operators.hive.HiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_stats_operator", "HiveStatsCollectionOperator"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.operators.hive_stats.HiveStatsCollectionOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "sensors", "hive_partition_sensor", "HivePartitionSensor"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.sensors.hive_partition.HivePartitionSensor",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "sensors", "metastore_partition_sensor", "MetastorePartitionSensor"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.sensors.metastore_partition.MetastorePartitionSensor",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "sensors", "named_hive_partition_sensor", "NamedHivePartitionSensor"] => Replacement::ProviderName{
            name: "airflow.providers.apache.hive.sensors.named_hive_partition.NamedHivePartitionSensor",
            provider: "apache-hive",
            version: "1.0.0"
        },

        // apache-airflow-providers-http
        ["airflow", "hooks", "http_hook", "HttpHook"] => Replacement::ProviderName{
            name: "airflow.providers.http.hooks.http.HttpHook",
            provider: "http",
            version: "1.0.0"
        },
        ["airflow", "operators", "http_operator", "SimpleHttpOperator"] => Replacement::ProviderName{
            name: "airflow.providers.http.operators.http.SimpleHttpOperator",
            provider: "http",
            version: "1.0.0"
        },
        ["airflow", "sensors", "http_sensor", "HttpSensor"] => Replacement::ProviderName{
            name: "airflow.providers.http.sensors.http.HttpSensor",
            provider: "http",
            version: "1.0.0"
        },

        // apache-airflow-providers-jdbc
        ["airflow", "hooks", "jdbc_hook", "JdbcHook"] => Replacement::ProviderName{
            name: "airflow.providers.jdbc.hooks.jdbc.JdbcHook",
            provider: "jdbc",
            version: "1.0.0"
        },
        ["airflow", "hooks", "jdbc_hook", "jaydebeapi"] => Replacement::ProviderName{
            name: "airflow.providers.jdbc.hooks.jdbc.jaydebeapi",
            provider: "jdbc",
            version: "1.0.0"
        },
        ["airflow", "operators", "jdbc_operator", "JdbcOperator"] => Replacement::ProviderName{
            name: "airflow.providers.jdbc.operators.jdbc.JdbcOperator",
            provider: "jdbc",
            version: "1.0.0"
        },

        // apache-airflow-providers-cncf-kubernetes
        ["airflow", "executors", "kubernetes_executor_types", "ALL_NAMESPACES"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types.ALL_NAMESPACES",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "executors", "kubernetes_executor_types", "POD_EXECUTOR_DONE_KEY"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types.POD_EXECUTOR_DONE_KEY",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kubernetes_helper_functions", "add_pod_suffix"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions.add_pod_suffix",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kubernetes_helper_functions", "annotations_for_logging_task_metadata"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions.annotations_for_logging_task_metadata",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kubernetes_helper_functions", "annotations_to_key"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions.annotations_to_key",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kubernetes_helper_functions", "create_pod_id"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions.create_pod_id",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kubernetes_helper_functions", "get_logs_task_metadata"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions.get_logs_task_metadata",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kubernetes_helper_functions", "rand_str"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions.rand_str",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod", "Port"] => Replacement::ProviderName{
            name: "kubernetes.client.models.V1ContainerPort",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod", "Resources"] => Replacement::ProviderName{
            name: "kubernetes.client.models.V1ResourceRequirements",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_launcher", "PodLauncher"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_launcher.PodLauncher",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_launcher", "PodStatus"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_launcher.PodStatus",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_launcher_deprecated", "PodLauncher"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_launcher_deprecated.PodLauncher",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_launcher_deprecated", "PodStatus"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_launcher_deprecated.PodStatus",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_launcher_deprecated", "get_kube_client"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.kube_client.get_kube_client",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_launcher_deprecated", "PodDefaults"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator_deprecated.PodDefaults",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_runtime_info_env", "PodRuntimeInfoEnv"] => Replacement::ProviderName{
            name: "kubernetes.client.models.V1EnvVar",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "volume", "Volume"] => Replacement::ProviderName{
            name: "kubernetes.client.models.V1Volume",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "volume_mount", "VolumeMount"] => Replacement::ProviderName{
            name: "kubernetes.client.models.V1VolumeMount",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "k8s_model", "K8SModel"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.k8s_model.K8SModel",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "k8s_model", "append_to_pod"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.k8s_model.append_to_pod",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kube_client", "_disable_verify_ssl"] => Replacement::ProviderName{
            name: "airflow.kubernetes.airflow.providers.cncf.kubernetes.kube_client._disable_verify_ssl",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kube_client", "_enable_tcp_keepalive"] => Replacement::ProviderName{
            name: "airflow.kubernetes.airflow.providers.cncf.kubernetes.kube_client._enable_tcp_keepalive",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kube_client", "get_kube_client"] => Replacement::ProviderName{
            name: "airflow.kubernetes.airflow.providers.cncf.kubernetes.kube_client.get_kube_client",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "datetime_to_label_safe_datestring"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator.datetime_to_label_safe_datestring",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "extend_object_field"] => Replacement::ProviderName{
            name: "airflow.kubernetes.airflow.providers.cncf.kubernetes.pod_generator.extend_object_field",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "label_safe_datestring_to_datetime"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator.label_safe_datestring_to_datetime",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "make_safe_label_value"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator.make_safe_label_value",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "merge_objects"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator.merge_objects",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "PodGenerator"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator.PodGenerator",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator_deprecated", "make_safe_label_value"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator_deprecated.make_safe_label_value",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator_deprecated", "PodDefaults"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator_deprecated.PodDefaults",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator_deprecated", "PodGenerator"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator_deprecated.PodGenerator",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "secret", "Secret"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.secret.Secret",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "PodGeneratorDeprecated"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator.PodGenerator",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "PodDefaults"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.pod_generator_deprecated.PodDefaults",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "add_pod_suffix"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions.add_pod_suffix",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_generator", "rand_str"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions.rand_str",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "secret", "K8SModel"] => Replacement::ProviderName{
            name: "airflow.providers.cncf.kubernetes.k8s_model.K8SModel",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },

        // apache-airflow-providers-microsoft-mssql
        ["airflow", "hooks", "mssql_hook", "MsSqlHook"] => Replacement::ProviderName{
            name: "airflow.providers.microsoft.mssql.hooks.mssql.MsSqlHook",
            provider: "microsoft-mssql",
            version: "1.0.0"
        },
        ["airflow", "operators", "mssql_operator", "MsSqlOperator"] => Replacement::ProviderName{
            name: "airflow.providers.microsoft.mssql.operators.mssql.MsSqlOperator",
            provider: "microsoft-mssql",
            version: "1.0.0"
        },

        // apache-airflow-providers-mysql
        ["airflow", "hooks", "mysql_hook", "MySqlHook"] => Replacement::ProviderName{
            name: "airflow.providers.mysql.hooks.mysql.MySqlHook",
            provider: "mysql",
            version: "1.0.0"
        },
        ["airflow", "operators", "mysql_operator", "MySqlOperator"] => Replacement::ProviderName{
            name: "airflow.providers.mysql.operators.mysql.MySqlOperator",
            provider: "mysql",
            version: "1.0.0"
        },
        ["airflow", "operators", "presto_to_mysql", "PrestoToMySqlOperator"] => Replacement::ProviderName{
            name: "airflow.providers.mysql.transfers.presto_to_mysql.PrestoToMySqlOperator",
            provider: "mysql",
            version: "1.0.0"
        },
        ["airflow", "operators", "presto_to_mysql", "PrestoToMySqlTransfer"] => Replacement::ProviderName{
            name: "airflow.providers.mysql.transfers.presto_to_mysql.PrestoToMySqlOperator",
            provider: "mysql",
            version: "1.0.0"
        },

        // apache-airflow-providers-oracle
        ["airflow", "hooks", "oracle_hook", "OracleHook"] => Replacement::ProviderName{
            name: "airflow.providers.oracle.hooks.oracle.OracleHook",
            provider: "oracle",
            version: "1.0.0"
        },
        ["airflow", "operators", "oracle_operator", "OracleOperator"] => Replacement::ProviderName{
            name: "airflow.providers.oracle.operators.oracle.OracleOperator",
            provider: "oracle",
            version: "1.0.0"
        },

        // apache-airflow-providers-papermill
        ["airflow", "operators", "papermill_operator", "PapermillOperator"] => Replacement::ProviderName{
            name: "airflow.providers.papermill.operators.papermill.PapermillOperator",
            provider: "papermill",
            version: "1.0.0"
        },

        // apache-airflow-providers-apache-pig
        ["airflow", "hooks", "pig_hook", "PigCliHook"] => Replacement::ProviderName{
            name: "airflow.providers.apache.pig.hooks.pig.PigCliHook",
            provider: "apache-pig",
            version: "1.0.0"
        },
        ["airflow", "operators", "pig_operator", "PigOperator"] => Replacement::ProviderName{
            name: "airflow.providers.apache.pig.operators.pig.PigOperator",
            provider: "apache-pig",
            version: "1.0.0"
        },

        // apache-airflow-providers-postgres
        ["airflow", "hooks", "postgres_hook", "PostgresHook"] => Replacement::ProviderName{
            name: "airflow.providers.postgres.hooks.postgres.PostgresHook",
            provider: "postgres",
            version: "1.0.0"
        },
        ["airflow", "operators", "postgres_operator", "Mapping"] => Replacement::ProviderName{
            name: "airflow.providers.postgres.operators.postgres.Mapping",
            provider: "postgres",
            version: "1.0.0"
        },

        ["airflow", "operators", "postgres_operator", "PostgresOperator"] => Replacement::ProviderName{
            name: "airflow.providers.postgres.operators.postgres.PostgresOperator",
            provider: "postgres",
            version: "1.0.0"
        },

        // apache-airflow-providers-presto
        ["airflow", "hooks", "presto_hook", "PrestoHook"] => Replacement::ProviderName{
            name: "airflow.providers.presto.hooks.presto.PrestoHook",
            provider: "presto",
            version: "1.0.0"
        },

        // apache-airflow-providers-samba
        ["airflow", "hooks", "samba_hook", "SambaHook"] => Replacement::ProviderName{
            name: "airflow.providers.samba.hooks.samba.SambaHook",
            provider: "samba",
            version: "1.0.0"
        },

        // apache-airflow-providers-slack
        ["airflow", "hooks", "slack_hook", "SlackHook"] => Replacement::ProviderName{
            name: "airflow.providers.slack.hooks.slack.SlackHook",
            provider: "slack",
            version: "1.0.0"
        },
        ["airflow", "operators", "slack_operator", "SlackAPIOperator"] => Replacement::ProviderName{
            name: "airflow.providers.slack.operators.slack.SlackAPIOperator",
            provider: "slack",
            version: "1.0.0"
        },
        ["airflow", "operators", "slack_operator", "SlackAPIPostOperator"] => Replacement::ProviderName{
            name: "airflow.providers.slack.operators.slack.SlackAPIPostOperator",
            provider: "slack",
            version: "1.0.0"
        },

        // apache-airflow-providers-standard
        ["airflow", "sensors", "filesystem", "FileSensor"] => Replacement::ProviderName{
            name: "airflow.providers.standard.sensors.filesystem.FileSensor",
            provider: "standard",
            version: "0.0.2"
        },
        ["airflow", "operators", "trigger_dagrun", "TriggerDagRunOperator"] => Replacement::ProviderName{
            name: "airflow.providers.standard.operators.trigger_dagrun.TriggerDagRunOperator",
            provider: "standard",
            version: "0.0.2"
        },
        ["airflow", "sensors", "external_task", "ExternalTaskMarker"] => Replacement::ProviderName{
            name: "airflow.providers.standard.sensors.external_task.ExternalTaskMarker",
            provider: "standard",
            version: "0.0.3"
        },
        ["airflow", "sensors", "external_task", "ExternalTaskSensor"] => Replacement::ProviderName{
            name: "airflow.providers.standard.sensors.external_task.ExternalTaskSensor",
            provider: "standard",
            version: "0.0.3"
        },

        // apache-airflow-providers-sqlite
        ["airflow", "hooks", "sqlite_hook", "SqliteHook"] => Replacement::ProviderName{
            name: "airflow.providers.sqlite.hooks.sqlite.SqliteHook",
            provider: "sqlite",
            version: "1.0.0"
        },
        ["airflow", "operators", "sqlite_operator", "SqliteOperator"] => Replacement::ProviderName{
            name: "airflow.providers.sqlite.operators.sqlite.SqliteOperator",
            provider: "sqlite",
            version: "1.0.0"
        },

        // apache-airflow-providers-zendesk
        ["airflow", "hooks", "zendesk_hook", "ZendeskHook"] =>
            Replacement::ProviderName{
            name: "airflow.providers.zendesk.hooks.zendesk.ZendeskHook",
            provider: "zendesk",
            version: "1.0.0"
        },

        // ImportPathMoved: for cases that the whole module has been moved
        // apache-airflow-providers-fab
        ["airflow", "api", "auth", "backend", "basic_auth", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.api.auth.backend.basic_auth",
            new_path: "airflow.providers.fab.auth_manager.api.auth.backend.basic_auth",
            provider:"fab",
            version: "1.0.0"
        },
        ["airflow", "api", "auth", "backend", "kerberos_auth", ..] => Replacement::ImportPathMoved{
            original_path:"airflow.api.auth.backend.kerberos_auth",
            new_path: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
            provider: "fab",
            version:"1.0.0"
        },
        ["airflow", "auth", "managers", "fab", "api", "auth", "backend", "kerberos_auth", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.auth_manager.api.auth.backend.kerberos_auth",
            new_path: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
            provider: "fab",
            version: "1.0.0"
        },
        ["airflow", "auth", "managers", "fab", "security_manager", "override", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.auth.managers.fab.security_manager.override",
            new_path: "airflow.providers.fab.auth_manager.security_manager.override",
            provider: "fab",
            version: "1.0.0"
        },

        // apache-airflow-providers-standard
        ["airflow", "operators", "bash", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.operators.bash",
            new_path: "airflow.providers.standard.operators.bash",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "operators", "bash_operator", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.operators.bash_operator",
            new_path: "airflow.providers.standard.operators.bash",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "operators", "datetime", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.operators.datetime",
            new_path: "airflow.providers.standard.time.operators.datetime",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "operators", "weekday", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.operators.weekday",
            new_path: "airflow.providers.standard.time.operators.weekday",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "sensors", "date_time", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.sensors.date_time",
            new_path: "airflow.providers.standard.time.sensors.date_time",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "sensors", "time_sensor", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.sensors.time_sensor",
            new_path: "airflow.providers.standard.time.sensors.time",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "sensors", "time_delta", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.sensors.time_delta",
            new_path: "airflow.providers.standard.time.sensors.time_delta",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "sensors", "weekday", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.sensors.weekday",
            new_path: "airflow.providers.standard.time.sensors.weekday",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "hooks", "filesystem", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.hooks.filesystem",
            new_path: "airflow.providers.standard.hooks.filesystem",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "hooks", "package_index", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.hooks.package_index",
            new_path: "airflow.providers.standard.hooks.package_index",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "hooks", "subprocess", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.hooks.subprocess",
            new_path: "airflow.providers.standard.hooks.subprocess",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "triggers", "external_task", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.triggers.external_task",
            new_path: "airflow.providers.standard.triggers.external_task",
            provider: "standard",
            version: "0.0.3"
        },
        ["airflow", "triggers", "file", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.triggers.file",
            new_path: "airflow.providers.standard.triggers.file",
            provider: "standard",
            version: "0.0.3"
        },
        ["airflow", "triggers", "temporal", ..] => Replacement::ImportPathMoved{
            original_path: "airflow.triggers.temporal",
            new_path: "airflow.providers.standard.triggers.temporal",
            provider: "standard",
            version: "0.0.3"
        },
        _ => return,
    };
    checker.report_diagnostic(Diagnostic::new(
        Airflow3MovedToProvider {
            deprecated: qualified_name.to_string(),
            replacement,
        },
        ranged.range(),
    ));
}
