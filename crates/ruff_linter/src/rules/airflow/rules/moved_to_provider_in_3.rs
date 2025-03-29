use crate::importer::ImportRequest;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3MovedToProvider {
            deprecated,
            replacement,
        } = self;
        match replacement {
            Replacement::ProviderName {
                path: _,
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
            path,
            name,
            provider,
            version,
        } = replacement
        {
            Some(format!(
                "Install `apache-airflow-provider-{provider}>={version}` and use `{path}.{name}` instead."
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

#[derive(Debug, Clone, Eq, PartialEq)]
enum Replacement {
    ProviderName {
        path: &'static str,
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
        ["airflow", "hooks", "S3_hook", "S3Hook"] => Replacement::ProviderName {
            path: "airflow.providers.amazon.aws.hooks.s3",
            name: "S3Hook",
            provider: "amazon",
            version: "1.0.0",
        },
        ["airflow", "hooks", "S3_hook", "provide_bucket_name"] => Replacement::ProviderName {
            path: "airflow.providers.amazon.aws.hooks.s3",
            name: "provide_bucket_name",
            provider: "amazon",
            version: "1.0.0",
        },
        ["airflow", "operators", "gcs_to_s3", "GCSToS3Operator"] => Replacement::ProviderName {
            path: "airflow.providers.amazon.aws.transfers.gcs_to_s3",
            name: "GCSToS3Operator",
            provider: "amazon",
            version: "1.0.0",
        },
        ["airflow", "operators", "google_api_to_s3_transfer", "GoogleApiToS3Operator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.amazon.aws.transfers.google_api_to_s3",
                name: "GoogleApiToS3Operator",
                provider: "amazon",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "google_api_to_s3_transfer", "GoogleApiToS3Transfer"] => {
            Replacement::ProviderName {
                path: "airflow.providers.amazon.aws.transfers.google_api_to_s3",
                name: "GoogleApiToS3Operator",
                provider: "amazon",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "redshift_to_s3_operator", "RedshiftToS3Operator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.amazon.aws.transfers.redshift_to_s3",
                name: "RedshiftToS3Operator",
                provider: "amazon",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "redshift_to_s3_operator", "RedshiftToS3Transfer"] => {
            Replacement::ProviderName {
                path: "airflow.providers.amazon.aws.transfers.redshift_to_s3",
                name: "RedshiftToS3Operator",
                provider: "amazon",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "s3_file_transform_operator", "S3FileTransformOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.amazon.aws.operators.s3_file_transform",
                name: "S3FileTransformOperator",
                provider: "amazon",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "s3_to_redshift_operator", "S3ToRedshiftOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.amazon.aws.transfers.s3_to_redshift",
                name: "S3ToRedshiftOperator",
                provider: "amazon",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "s3_to_redshift_operator", "S3ToRedshiftTransfer"] => {
            Replacement::ProviderName {
                path: "airflow.providers.amazon.aws.transfers.s3_to_redshift",
                name: "S3ToRedshiftOperator",
                provider: "amazon",
                version: "1.0.0",
            }
        }
        ["airflow", "sensors", "s3_key_sensor", "S3KeySensor"] => Replacement::ProviderName {
            path: "airflow.providers.amazon.aws.sensors.s3",
            name: "S3KeySensor",
            provider: "amazon",
            version: "1.0.0",
        },

        // apache-airflow-providers-celery
        ["airflow", "config_templates", "default_celery", "DEFAULT_CELERY_CONFIG"] => {
            Replacement::ProviderName {
                path: "airflow.providers.celery.executors.default_celery",
                name: "DEFAULT_CELERY_CONFIG",
                provider: "celery",
                version: "3.3.0",
            }
        }
        ["airflow", "executors", "celery_executor", "app"] => Replacement::ProviderName {
            path: "airflow.providers.celery.executors.celery_executor_utils",
            name: "app",
            provider: "celery",
            version: "3.3.0",
        },
        ["airflow", "executors", "celery_executor", "CeleryExecutor"] => {
            Replacement::ProviderName {
                path: "airflow.providers.celery.executors.celery_executor",
                name: "CeleryExecutor",
                provider: "celery",
                version: "3.3.0",
            }
        }
        ["airflow", "executors", "celery_kubernetes_executor", "CeleryKubernetesExecutor"] => {
            Replacement::ProviderName {
                path: "airflow.providers.celery.executors.celery_kubernetes_executor",
                name: "CeleryKubernetesExecutor",
                provider: "celery",
                version: "3.3.0",
            }
        }

        // apache-airflow-providers-common-sql
        ["airflow", "hooks", "dbapi", "ConnectorProtocol"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.hooks.sql",
            name: "ConnectorProtocol",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "hooks", "dbapi", "DbApiHook"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.hooks.sql",
            name: "DbApiHook",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "hooks", "dbapi_hook", "DbApiHook"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.hooks.sql",
            name: "DbApiHook",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "operators", "check_operator", "SQLCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "check_operator", "SQLIntervalCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLIntervalCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "check_operator", "SQLThresholdCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLThresholdCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "check_operator", "SQLValueCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLValueCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "check_operator", "CheckOperator"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "SQLCheckOperator",
            provider: "common-sql",
            version: "1.1.0",
        },
        ["airflow", "operators", "check_operator", "IntervalCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLIntervalCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "check_operator", "ThresholdCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLThresholdCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "check_operator", "ValueCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLValueCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "presto_check_operator", "SQLCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "presto_check_operator", "SQLIntervalCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLIntervalCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "presto_check_operator", "SQLValueCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLValueCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "presto_check_operator", "PrestoCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "presto_check_operator", "PrestoIntervalCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLIntervalCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "presto_check_operator", "PrestoValueCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLValueCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "sql", "BaseSQLOperator"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "BaseSQLOperator",
            provider: "common-sql",
            version: "1.1.0",
        },
        ["airflow", "operators", "sql", "BranchSQLOperator"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "BranchSQLOperator",
            provider: "common-sql",
            version: "1.1.0",
        },
        ["airflow", "operators", "sql", "SQLCheckOperator"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "SQLCheckOperator",
            provider: "common-sql",
            version: "1.1.0",
        },
        ["airflow", "operators", "sql", "SQLColumnCheckOperator"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "SQLColumnCheckOperator",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "operators", "sql", "SQLIntervalCheckOperator"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "SQLIntervalCheckOperator",
            provider: "common-sql",
            version: "1.1.0",
        },
        ["airflow", "operators", "sql", "SQLTablecheckOperator"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "SQLTableCheckOperator",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "operators", "sql", "SQLThresholdCheckOperator"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "SQLTableCheckOperator",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "operators", "sql", "SQLValueCheckOperator"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "SQLValueCheckOperator",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "operators", "sql", "_convert_to_float_if_possible"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "_convert_to_float_if_possible",
                provider: "common-sql",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "sql", "parse_boolean"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.operators.sql",
            name: "parse_boolean",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "operators", "sql_branch_operator", "BranchSQLOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "BranchSQLOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "sql_branch_operator", "BranchSqlOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "BranchSQLOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "sensors", "sql", "SqlSensor"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.sensors.sql",
            name: "SqlSensor",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "sensors", "sql_sensor", "SqlSensor"] => Replacement::ProviderName {
            path: "airflow.providers.common.sql.sensors.sql",
            name: "SqlSensor",
            provider: "common-sql",
            version: "1.0.0",
        },

        // apache-airflow-providers-daskexecutor
        ["airflow", "executors", "dask_executor", "DaskExecutor"] => Replacement::ProviderName {
            path: "airflow.providers.daskexecutor.executors.dask_executor",
            name: "DaskExecutor",
            provider: "daskexecutor",
            version: "1.0.0",
        },

        // apache-airflow-providers-docker
        ["airflow", "hooks", "docker_hook", "DockerHook"] => Replacement::ProviderName {
            path: "airflow.providers.docker.hooks.docker",
            name: "DockerHook",
            provider: "docker",
            version: "1.0.0",
        },
        ["airflow", "operators", "docker_operator", "DockerOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.docker.operators.docker",
                name: "DockerOperator",
                provider: "docker",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-apache-druid
        ["airflow", "hooks", "druid_hook", "DruidDbApiHook"] => Replacement::ProviderName {
            path: "airflow.providers.apache.druid.hooks.druid",
            name: "DruidDbApiHook",
            provider: "apache-druid",
            version: "1.0.0",
        },
        ["airflow", "hooks", "druid_hook", "DruidHook"] => Replacement::ProviderName {
            path: "airflow.providers.apache.druid.hooks.druid",
            name: "DruidHook",
            provider: "apache-druid",
            version: "1.0.0",
        },
        ["airflow", "operators", "druid_check_operator", "DruidCheckOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.common.sql.operators.sql",
                name: "SQLCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        ["airflow", "operators", "hive_to_druid", "HiveToDruidOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.druid.transfers.hive_to_druid",
                name: "HiveToDruidOperator",
                provider: "apache-druid",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "hive_to_druid", "HiveToDruidTransfer"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.druid.transfers.hive_to_druid",
                name: "HiveToDruidOperator",
                provider: "apache-druid",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-fab
        ["airflow", "www", "security", "FabAirflowSecurityManagerOverride"] => {
            Replacement::ProviderName {
                path: "airflow.providers.fab.auth_manager.security_manager.override",
                name: "FabAirflowSecurityManagerOverride",
                provider: "fab",
                version: "1.0.0",
            }
        }
        ["airflow", "auth", "managers", "fab", "fab_auth_manager", "FabAuthManager"] => {
            Replacement::ProviderName {
                path: "airflow.providers.fab.auth_manager.security_manager",
                name: "FabAuthManager",
                provider: "fab",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-apache-hdfs
        ["airflow", "hooks", "webhdfs_hook", "WebHDFSHook"] => Replacement::ProviderName {
            path: "airflow.providers.apache.hdfs.hooks.webhdfs",
            name: "WebHDFSHook",
            provider: "apache-hdfs",
            version: "1.0.0",
        },
        ["airflow", "sensors", "web_hdfs_sensor", "WebHdfsSensor"] => Replacement::ProviderName {
            path: "airflow.providers.apache.hdfs.sensors.web_hdfs",
            name: "WebHdfsSensor",
            provider: "apache-hdfs",
            version: "1.0.0",
        },

        // apache-airflow-providers-apache-hive
        ["airflow", "hooks", "hive_hooks", "HIVE_QUEUE_PRIORITIES"] => Replacement::ProviderName {
            path: "airflow.providers.apache.hive.hooks.hive",
            name: "HIVE_QUEUE_PRIORITIES",
            provider: "apache-hive",
            version: "1.0.0",
        },
        ["airflow", "macros", "hive", "closest_ds_partition"] => Replacement::ProviderName {
            path: "airflow.providers.apache.hive.macros.hive",
            name: "closest_ds_partition",
            provider: "apache-hive",
            version: "5.1.0",
        },
        ["airflow", "macros", "hive", "max_partition"] => Replacement::ProviderName {
            path: "airflow.providers.apache.hive.macros.hive",
            name: "max_partition",
            provider: "apache-hive",
            version: "5.1.0",
        },
        ["airflow", "operators", "hive_to_mysql", "HiveToMySqlOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.transfers.hive_to_mysql",
                name: "HiveToMySqlOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "hive_to_mysql", "HiveToMySqlTransfer"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.transfers.hive_to_mysql",
                name: "HiveToMySqlOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "hive_to_samba_operator", "HiveToSambaOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.transfers.hive_to_samba",
                name: "HiveToSambaOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "mssql_to_hive", "MsSqlToHiveOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.transfers.mssql_to_hive",
                name: "MsSqlToHiveOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "mssql_to_hive", "MsSqlToHiveTransfer"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.transfers.mssql_to_hive",
                name: "MsSqlToHiveOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "mysql_to_hive", "MySqlToHiveOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.transfers.mysql_to_hive",
                name: "MySqlToHiveOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "mysql_to_hive", "MySqlToHiveTransfer"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.transfers.mysql_to_hive",
                name: "MySqlToHiveOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "s3_to_hive_operator", "S3ToHiveOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.transfers.s3_to_hive",
                name: "S3ToHiveOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "s3_to_hive_operator", "S3ToHiveTransfer"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.transfers.s3_to_hive",
                name: "S3ToHiveOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "hooks", "hive_hooks", "HiveCliHook"] => Replacement::ProviderName {
            path: "airflow.providers.apache.hive.hooks.hive",
            name: "HiveCliHook",
            provider: "apache-hive",
            version: "1.0.0",
        },
        ["airflow", "hooks", "hive_hooks", "HiveMetastoreHook"] => Replacement::ProviderName {
            path: "airflow.providers.apache.hive.hooks.hive",
            name: "HiveMetastoreHook",
            provider: "apache-hive",
            version: "1.0.0",
        },
        ["airflow", "hooks", "hive_hooks", "HiveServer2Hook"] => Replacement::ProviderName {
            path: "airflow.providers.apache.hive.hooks.hive",
            name: "HiveServer2Hook",
            provider: "apache-hive",
            version: "1.0.0",
        },
        ["airflow", "operators", "hive_operator", "HiveOperator"] => Replacement::ProviderName {
            path: "airflow.providers.apache.hive.operators.hive",
            name: "HiveOperator",
            provider: "apache-hive",
            version: "1.0.0",
        },
        ["airflow", "operators", "hive_stats_operator", "HiveStatsCollectionOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.operators.hive_stats",
                name: "HiveStatsCollectionOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "sensors", "hive_partition_sensor", "HivePartitionSensor"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.sensors.hive_partition",
                name: "HivePartitionSensor",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "sensors", "metastore_partition_sensor", "MetastorePartitionSensor"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.sensors.metastore_partition",
                name: "MetastorePartitionSensor",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        ["airflow", "sensors", "named_hive_partition_sensor", "NamedHivePartitionSensor"] => {
            Replacement::ProviderName {
                path: "airflow.providers.apache.hive.sensors.named_hive_partition",
                name: "NamedHivePartitionSensor",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-http
        ["airflow", "hooks", "http_hook", "HttpHook"] => Replacement::ProviderName {
            path: "airflow.providers.http.hooks.http",
            name: "HttpHook",
            provider: "http",
            version: "1.0.0",
        },
        ["airflow", "operators", "http_operator", "SimpleHttpOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.http.operators.http",
                name: "SimpleHttpOperator",
                provider: "http",
                version: "1.0.0",
            }
        }
        ["airflow", "sensors", "http_sensor", "HttpSensor"] => Replacement::ProviderName {
            path: "airflow.providers.http.sensors.http",
            name: "HttpSensor",
            provider: "http",
            version: "1.0.0",
        },

        // apache-airflow-providers-jdbc
        ["airflow", "hooks", "jdbc_hook", "JdbcHook"] => Replacement::ProviderName {
            path: "airflow.providers.jdbc.hooks.jdbc",
            name: "JdbcHook",
            provider: "jdbc",
            version: "1.0.0",
        },
        ["airflow", "hooks", "jdbc_hook", "jaydebeapi"] => Replacement::ProviderName {
            path: "airflow.providers.jdbc.hooks.jdbc",
            name: "jaydebeapi",
            provider: "jdbc",
            version: "1.0.0",
        },
        ["airflow", "operators", "jdbc_operator", "JdbcOperator"] => Replacement::ProviderName {
            path: "airflow.providers.jdbc.operators.jdbc",
            name: "JdbcOperator",
            provider: "jdbc",
            version: "1.0.0",
        },

        // apache-airflow-providers-cncf-kubernetes
        ["airflow", "executors", "kubernetes_executor_types", "ALL_NAMESPACES"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types",
                name: "ALL_NAMESPACES",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "executors", "kubernetes_executor_types", "POD_EXECUTOR_DONE_KEY"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types",
                name: "POD_EXECUTOR_DONE_KEY",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "kubernetes_helper_functions", "add_pod_suffix"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
                name: "add_pod_suffix",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "kubernetes_helper_functions", "annotations_for_logging_task_metadata"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
                name: "annotations_for_logging_task_metadata",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "kubernetes_helper_functions", "annotations_to_key"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
                name: "annotations_to_key",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "kubernetes_helper_functions", "create_pod_id"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
                name: "create_pod_id",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "kubernetes_helper_functions", "get_logs_task_metadata"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
                name: "get_logs_task_metadata",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "kubernetes_helper_functions", "rand_str"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
                name: "rand_str",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod", "Port"] => Replacement::ProviderName {
            path: "kubernetes.client.models",
            name: "V1ContainerPort",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod", "Resources"] => Replacement::ProviderName {
            path: "kubernetes.client.models",
            name: "V1ResourceRequirements",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod_launcher", "PodLauncher"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.pod_launcher",
            name: "PodLauncher",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod_launcher", "PodStatus"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.pod_launcher",
            name: "PodStatus",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod_launcher_deprecated", "PodLauncher"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_launcher_deprecated",
                name: "PodLauncher",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_launcher_deprecated", "PodStatus"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_launcher_deprecated",
                name: "PodStatus",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_launcher_deprecated", "get_kube_client"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.kube_client",
                name: "get_kube_client",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_launcher_deprecated", "PodDefaults"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_generator_deprecated",
                name: "PodDefaults",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_runtime_info_env", "PodRuntimeInfoEnv"] => {
            Replacement::ProviderName {
                path: "kubernetes.client.models",
                name: "V1EnvVar",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "volume", "Volume"] => Replacement::ProviderName {
            path: "kubernetes.client.models",
            name: "V1Volume",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "volume_mount", "VolumeMount"] => Replacement::ProviderName {
            path: "kubernetes.client.models",
            name: "V1VolumeMount",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "k8s_model", "K8SModel"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.k8s_model",
            name: "K8SModel",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "k8s_model", "append_to_pod"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.k8s_model",
            name: "append_to_pod",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "kube_client", "_disable_verify_ssl"] => {
            Replacement::ProviderName {
                path: "airflow.kubernetes.airflow.providers.cncf.kubernetes.kube_client",
                name: "_disable_verify_ssl",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "kube_client", "_enable_tcp_keepalive"] => {
            Replacement::ProviderName {
                path: "airflow.kubernetes.airflow.providers.cncf.kubernetes.kube_client",
                name: "_enable_tcp_keepalive",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "kube_client", "get_kube_client"] => Replacement::ProviderName {
            path: "airflow.kubernetes.airflow.providers.cncf.kubernetes.kube_client",
            name: "get_kube_client",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod_generator", "datetime_to_label_safe_datestring"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_generator",
                name: "datetime_to_label_safe_datestring",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_generator", "extend_object_field"] => {
            Replacement::ProviderName {
                path: "airflow.kubernetes.airflow.providers.cncf.kubernetes.pod_generator",
                name: "extend_object_field",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_generator", "label_safe_datestring_to_datetime"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_generator",
                name: "label_safe_datestring_to_datetime",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_generator", "make_safe_label_value"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_generator",
                name: "make_safe_label_value",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_generator", "merge_objects"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.pod_generator",
            name: "merge_objects",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod_generator", "PodGenerator"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.pod_generator",
            name: "PodGenerator",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod_generator_deprecated", "make_safe_label_value"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_generator_deprecated",
                name: "make_safe_label_value",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_generator_deprecated", "PodDefaults"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_generator_deprecated",
                name: "PodDefaults",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_generator_deprecated", "PodGenerator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_generator_deprecated",
                name: "PodGenerator",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "secret", "Secret"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.secret",
            name: "Secret",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod_generator", "PodGeneratorDeprecated"] => {
            Replacement::ProviderName {
                path: "airflow.providers.cncf.kubernetes.pod_generator",
                name: "PodGenerator",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }
        ["airflow", "kubernetes", "pod_generator", "PodDefaults"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.pod_generator_deprecated",
            name: "PodDefaults",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod_generator", "add_pod_suffix"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
            name: "add_pod_suffix",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod_generator", "rand_str"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
            name: "rand_str",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "secret", "K8SModel"] => Replacement::ProviderName {
            path: "airflow.providers.cncf.kubernetes.k8s_model",
            name: "K8SModel",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },

        // apache-airflow-providers-microsoft-mssql
        ["airflow", "hooks", "mssql_hook", "MsSqlHook"] => Replacement::ProviderName {
            path: "airflow.providers.microsoft.mssql.hooks.mssql",
            name: "MsSqlHook",
            provider: "microsoft-mssql",
            version: "1.0.0",
        },
        ["airflow", "operators", "mssql_operator", "MsSqlOperator"] => Replacement::ProviderName {
            path: "airflow.providers.microsoft.mssql.operators.mssql",
            name: "MsSqlOperator",
            provider: "microsoft-mssql",
            version: "1.0.0",
        },

        // apache-airflow-providers-mysql
        ["airflow", "hooks", "mysql_hook", "MySqlHook"] => Replacement::ProviderName {
            path: "airflow.providers.mysql.hooks.mysql",
            name: "MySqlHook",
            provider: "mysql",
            version: "1.0.0",
        },
        ["airflow", "operators", "mysql_operator", "MySqlOperator"] => Replacement::ProviderName {
            path: "airflow.providers.mysql.operators.mysql",
            name: "MySqlOperator",
            provider: "mysql",
            version: "1.0.0",
        },
        ["airflow", "operators", "presto_to_mysql", "PrestoToMySqlOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.mysql.transfers.presto_to_mysql",
                name: "PrestoToMySqlOperator",
                provider: "mysql",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "presto_to_mysql", "PrestoToMySqlTransfer"] => {
            Replacement::ProviderName {
                path: "airflow.providers.mysql.transfers.presto_to_mysql",
                name: "PrestoToMySqlOperator",
                provider: "mysql",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-oracle
        ["airflow", "hooks", "oracle_hook", "OracleHook"] => Replacement::ProviderName {
            path: "airflow.providers.oracle.hooks.oracle",
            name: "OracleHook",
            provider: "oracle",
            version: "1.0.0",
        },
        ["airflow", "operators", "oracle_operator", "OracleOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.oracle.operators.oracle",
                name: "OracleOperator",
                provider: "oracle",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-papermill
        ["airflow", "operators", "papermill_operator", "PapermillOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.papermill.operators.papermill",
                name: "PapermillOperator",
                provider: "papermill",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-apache-pig
        ["airflow", "hooks", "pig_hook", "PigCliHook"] => Replacement::ProviderName {
            path: "airflow.providers.apache.pig.hooks.pig",
            name: "PigCliHook",
            provider: "apache-pig",
            version: "1.0.0",
        },
        ["airflow", "operators", "pig_operator", "PigOperator"] => Replacement::ProviderName {
            path: "airflow.providers.apache.pig.operators.pig",
            name: "PigOperator",
            provider: "apache-pig",
            version: "1.0.0",
        },

        // apache-airflow-providers-postgres
        ["airflow", "hooks", "postgres_hook", "PostgresHook"] => Replacement::ProviderName {
            path: "airflow.providers.postgres.hooks.postgres",
            name: "PostgresHook",
            provider: "postgres",
            version: "1.0.0",
        },
        ["airflow", "operators", "postgres_operator", "Mapping"] => Replacement::ProviderName {
            path: "airflow.providers.postgres.operators.postgres",
            name: "Mapping",
            provider: "postgres",
            version: "1.0.0",
        },

        ["airflow", "operators", "postgres_operator", "PostgresOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.postgres.operators.postgres",
                name: "PostgresOperator",
                provider: "postgres",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-presto
        ["airflow", "hooks", "presto_hook", "PrestoHook"] => Replacement::ProviderName {
            path: "airflow.providers.presto.hooks.presto",
            name: "PrestoHook",
            provider: "presto",
            version: "1.0.0",
        },

        // apache-airflow-providers-samba
        ["airflow", "hooks", "samba_hook", "SambaHook"] => Replacement::ProviderName {
            path: "airflow.providers.samba.hooks.samba",
            name: "SambaHook",
            provider: "samba",
            version: "1.0.0",
        },

        // apache-airflow-providers-slack
        ["airflow", "hooks", "slack_hook", "SlackHook"] => Replacement::ProviderName {
            path: "airflow.providers.slack.hooks.slack",
            name: "SlackHook",
            provider: "slack",
            version: "1.0.0",
        },
        ["airflow", "operators", "slack_operator", "SlackAPIOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.slack.operators.slack",
                name: "SlackAPIOperator",
                provider: "slack",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "slack_operator", "SlackAPIPostOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.slack.operators.slack",
                name: "SlackAPIPostOperator",
                provider: "slack",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-standard
        ["airflow", "sensors", "filesystem", "FileSensor"] => Replacement::ProviderName {
            path: "airflow.providers.standard.sensors.filesystem",
            name: "FileSensor",
            provider: "standard",
            version: "0.0.2",
        },
        ["airflow", "operators", "trigger_dagrun", "TriggerDagRunOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.standard.operators.trigger_dagrun",
                name: "TriggerDagRunOperator",
                provider: "standard",
                version: "0.0.2",
            }
        }
        ["airflow", "sensors", "external_task", "ExternalTaskMarker"] => {
            Replacement::ProviderName {
                path: "airflow.providers.standard.sensors.external_task",
                name: "ExternalTaskMarker",
                provider: "standard",
                version: "0.0.3",
            }
        }
        ["airflow", "sensors", "external_task", "ExternalTaskSensor"] => {
            Replacement::ProviderName {
                path: "airflow.providers.standard.sensors.external_task",
                name: "ExternalTaskSensor",
                provider: "standard",
                version: "0.0.3",
            }
        }

        // apache-airflow-providers-sqlite
        ["airflow", "hooks", "sqlite_hook", "SqliteHook"] => Replacement::ProviderName {
            path: "airflow.providers.sqlite.hooks.sqlite",
            name: "SqliteHook",
            provider: "sqlite",
            version: "1.0.0",
        },
        ["airflow", "operators", "sqlite_operator", "SqliteOperator"] => {
            Replacement::ProviderName {
                path: "airflow.providers.sqlite.operators.sqlite",
                name: "SqliteOperator",
                provider: "sqlite",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-zendesk
        ["airflow", "hooks", "zendesk_hook", "ZendeskHook"] => Replacement::ProviderName {
            path: "airflow.providers.zendesk.hooks.zendesk",
            name: "ZendeskHook",
            provider: "zendesk",
            version: "1.0.0",
        },

        // ImportPathMoved: for cases that the whole module has been moved
        // apache-airflow-providers-fab
        ["airflow", "api", "auth", "backend", "basic_auth", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.api.auth.backend.basic_auth",
            new_path: "airflow.providers.fab.auth_manager.api.auth.backend.basic_auth",
            provider: "fab",
            version: "1.0.0",
        },
        ["airflow", "api", "auth", "backend", "kerberos_auth", ..] => {
            Replacement::ImportPathMoved {
                original_path: "airflow.api.auth.backend.kerberos_auth",
                new_path: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
                provider: "fab",
                version: "1.0.0",
            }
        }
        ["airflow", "auth", "managers", "fab", "api", "auth", "backend", "kerberos_auth", ..] => {
            Replacement::ImportPathMoved {
                original_path: "airflow.auth_manager.api.auth.backend.kerberos_auth",
                new_path: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
                provider: "fab",
                version: "1.0.0",
            }
        }
        ["airflow", "auth", "managers", "fab", "security_manager", "override", ..] => {
            Replacement::ImportPathMoved {
                original_path: "airflow.auth.managers.fab.security_manager.override",
                new_path: "airflow.providers.fab.auth_manager.security_manager.override",
                provider: "fab",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-standard
        ["airflow", "operators", "bash", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.operators.bash",
            new_path: "airflow.providers.standard.operators.bash",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "operators", "bash_operator", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.operators.bash_operator",
            new_path: "airflow.providers.standard.operators.bash",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "operators", "datetime", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.operators.datetime",
            new_path: "airflow.providers.standard.time.operators.datetime",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "operators", "weekday", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.operators.weekday",
            new_path: "airflow.providers.standard.time.operators.weekday",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "sensors", "date_time", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.sensors.date_time",
            new_path: "airflow.providers.standard.time.sensors.date_time",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "sensors", "time_sensor", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.sensors.time_sensor",
            new_path: "airflow.providers.standard.time.sensors.time",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "sensors", "time_delta", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.sensors.time_delta",
            new_path: "airflow.providers.standard.time.sensors.time_delta",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "sensors", "weekday", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.sensors.weekday",
            new_path: "airflow.providers.standard.time.sensors.weekday",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "hooks", "filesystem", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.hooks.filesystem",
            new_path: "airflow.providers.standard.hooks.filesystem",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "hooks", "package_index", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.hooks.package_index",
            new_path: "airflow.providers.standard.hooks.package_index",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "hooks", "subprocess", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.hooks.subprocess",
            new_path: "airflow.providers.standard.hooks.subprocess",
            provider: "standard",
            version: "0.0.1",
        },
        ["airflow", "triggers", "external_task", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.triggers.external_task",
            new_path: "airflow.providers.standard.triggers.external_task",
            provider: "standard",
            version: "0.0.3",
        },
        ["airflow", "triggers", "file", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.triggers.file",
            new_path: "airflow.providers.standard.triggers.file",
            provider: "standard",
            version: "0.0.3",
        },
        ["airflow", "triggers", "temporal", ..] => Replacement::ImportPathMoved {
            original_path: "airflow.triggers.temporal",
            new_path: "airflow.providers.standard.triggers.temporal",
            provider: "standard",
            version: "0.0.3",
        },
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(
        Airflow3MovedToProvider {
            deprecated: qualified_name.to_string(),
            replacement: replacement.clone(),
        },
        ranged.range(),
    );

    if let Replacement::ProviderName {
        path,
        name,
        provider: _,
        version: _,
    } = replacement
    {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from(path, name),
                expr.start(),
                checker.semantic(),
            )?;
            let replacement_edit = Edit::range_replacement(binding, ranged.range());
            Ok(Fix::safe_edits(import_edit, [replacement_edit]))
        });
    };

    checker.report_diagnostic(diagnostic);
}
