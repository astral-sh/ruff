use crate::importer::ImportRequest;
use crate::rules::airflow::helpers::{ProviderReplacement, is_guarded_by_try_except};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
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
    replacement: ProviderReplacement,
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
            ProviderReplacement::None => {
                format!("`{deprecated}` is removed in Airflow 3.0")
            }
            ProviderReplacement::AutoImport {
                name: _,
                module: _,
                provider,
                version: _,
            }
            | ProviderReplacement::SourceModuleMovedToProvider {
                name: _,
                module: _,
                provider,
                version: _,
            } => {
                format!("`{deprecated}` is moved into `{provider}` provider in Airflow 3.0;")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow3MovedToProvider { replacement, .. } = self;
        match replacement {
            ProviderReplacement::None => None,
            ProviderReplacement::AutoImport {
                name,
                module,
                provider,
                version,
            } => Some(format!(
                "Install `apache-airflow-providers-{provider}>={version}` and use `{module}.{name}` instead."
            )),
            ProviderReplacement::SourceModuleMovedToProvider {
                name,
                module,
                provider,
                version,
            } => Some(format!(
                "Install `apache-airflow-providers-{provider}>={version}` and use `{module}.{name}` instead."
            )),
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
        Expr::Name(_) => check_names_moved_to_provider(checker, expr, expr.range()),
        _ => {}
    }
}

fn check_names_moved_to_provider(checker: &Checker, expr: &Expr, ranged: TextRange) {
    let Some(qualified_name) = checker.semantic().resolve_qualified_name(expr) else {
        return;
    };

    let replacement = match qualified_name.segments() {
        // apache-airflow-providers-amazon
        [
            "airflow",
            "hooks",
            "S3_hook",
            rest @ ("S3Hook" | "provide_bucket_name"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.amazon.aws.hooks.s3",
            provider: "amazon",
            version: "1.0.0",
        },
        ["airflow", "operators", "gcs_to_s3", "GCSToS3Operator"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.amazon.aws.transfers.gcs_to_s3",
                name: "GCSToS3Operator",
                provider: "amazon",
                version: "1.0.0",
            }
        }
        [
            "airflow",
            "operators",
            "google_api_to_s3_transfer",
            "GoogleApiToS3Operator" | "GoogleApiToS3Transfer",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.amazon.aws.transfers.google_api_to_s3",
            name: "GoogleApiToS3Operator",
            provider: "amazon",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "redshift_to_s3_operator",
            "RedshiftToS3Operator" | "RedshiftToS3Transfer",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.amazon.aws.transfers.redshift_to_s3",
            name: "RedshiftToS3Operator",
            provider: "amazon",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "s3_file_transform_operator",
            "S3FileTransformOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.amazon.aws.operators.s3",
            name: "S3FileTransformOperator",
            provider: "amazon",
            version: "3.0.0",
        },
        [
            "airflow",
            "operators",
            "s3_to_redshift_operator",
            "S3ToRedshiftOperator" | "S3ToRedshiftTransfer",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.amazon.aws.transfers.s3_to_redshift",
            name: "S3ToRedshiftOperator",
            provider: "amazon",
            version: "1.0.0",
        },
        ["airflow", "sensors", "s3_key_sensor", "S3KeySensor"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.amazon.aws.sensors.s3",
            name: "S3KeySensor",
            provider: "amazon",
            version: "1.0.0",
        },

        // apache-airflow-providers-celery
        [
            "airflow",
            "config_templates",
            "default_celery",
            "DEFAULT_CELERY_CONFIG",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.celery.executors.default_celery",
            name: "DEFAULT_CELERY_CONFIG",
            provider: "celery",
            version: "3.3.0",
        },
        ["airflow", "executors", "celery_executor", rest] => match *rest {
            "app" => ProviderReplacement::AutoImport {
                module: "airflow.providers.celery.executors.celery_executor_utils",
                name: "app",
                provider: "celery",
                version: "3.3.0",
            },
            "CeleryExecutor" => ProviderReplacement::AutoImport {
                module: "airflow.providers.celery.executors.celery_executor",
                name: "CeleryExecutor",
                provider: "celery",
                version: "3.3.0",
            },
            _ => return,
        },
        [
            "airflow",
            "executors",
            "celery_kubernetes_executor",
            "CeleryKubernetesExecutor",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.celery.executors.celery_kubernetes_executor",
            name: "CeleryKubernetesExecutor",
            provider: "celery",
            version: "3.3.0",
        },

        // apache-airflow-providers-common-sql
        [
            "airflow",
            "hooks",
            "dbapi",
            rest @ ("ConnectorProtocol" | "DbApiHook"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.common.sql.hooks.sql",
            provider: "common-sql",
            version: "1.0.0",
        },
        ["airflow", "hooks", "dbapi_hook", "DbApiHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.common.sql.hooks.sql",
            name: "DbApiHook",
            provider: "common-sql",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "check_operator" | "sql",
            "SQLCheckOperator",
        ]
        | [
            "airflow",
            "operators",
            "check_operator" | "druid_check_operator" | "presto_check_operator",
            "CheckOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.common.sql.operators.sql",
            name: "SQLCheckOperator",
            provider: "common-sql",
            version: "1.1.0",
        },
        [
            "airflow",
            "operators",
            "druid_check_operator",
            "DruidCheckOperator",
        ]
        | [
            "airflow",
            "operators",
            "presto_check_operator",
            "PrestoCheckOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.common.sql.operators.sql",
            name: "SQLCheckOperator",
            provider: "common-sql",
            version: "1.1.0",
        },
        [
            "airflow",
            "operators",
            "check_operator",
            "IntervalCheckOperator" | "SQLIntervalCheckOperator",
        ]
        | [
            "airflow",
            "operators",
            "presto_check_operator",
            "IntervalCheckOperator",
        ]
        | ["airflow", "operators", "sql", "SQLIntervalCheckOperator"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.common.sql.operators.sql",
                name: "SQLIntervalCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        [
            "airflow",
            "operators",
            "presto_check_operator",
            "PrestoIntervalCheckOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.common.sql.operators.sql",
            name: "SQLIntervalCheckOperator",
            provider: "common-sql",
            version: "1.1.0",
        },
        [
            "airflow",
            "operators",
            "check_operator",
            "SQLThresholdCheckOperator" | "ThresholdCheckOperator",
        ]
        | ["airflow", "operators", "sql", "SQLThresholdCheckOperator"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.common.sql.operators.sql",
                name: "SQLThresholdCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        [
            "airflow",
            "operators",
            "check_operator",
            "SQLValueCheckOperator" | "ValueCheckOperator",
        ]
        | [
            "airflow",
            "operators",
            "presto_check_operator",
            "ValueCheckOperator",
        ]
        | ["airflow", "operators", "sql", "SQLValueCheckOperator"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.common.sql.operators.sql",
                name: "SQLValueCheckOperator",
                provider: "common-sql",
                version: "1.1.0",
            }
        }
        [
            "airflow",
            "operators",
            "presto_check_operator",
            "PrestoValueCheckOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.common.sql.operators.sql",
            name: "SQLValueCheckOperator",
            provider: "common-sql",
            version: "1.1.0",
        },
        ["airflow", "operators", "sql", rest] => match *rest {
            "BaseSQLOperator" | "BranchSQLOperator" | "SQLTableCheckOperator" => {
                ProviderReplacement::SourceModuleMovedToProvider {
                    name: (*rest).to_string(),
                    module: "airflow.providers.common.sql.operators.sql",
                    provider: "common-sql",
                    version: "1.1.0",
                }
            }
            "SQLColumnCheckOperator" | "_convert_to_float_if_possible" | "parse_boolean" => {
                ProviderReplacement::SourceModuleMovedToProvider {
                    name: (*rest).to_string(),
                    module: "airflow.providers.common.sql.operators.sql",
                    provider: "common-sql",
                    version: "1.0.0",
                }
            }
            _ => return,
        },
        ["airflow", "sensors", "sql" | "sql_sensor", "SqlSensor"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.common.sql.sensors.sql",
                name: "SqlSensor",
                provider: "common-sql",
                version: "1.0.0",
            }
        }
        ["airflow", "operators", "jdbc_operator", "JdbcOperator"]
        | ["airflow", "operators", "mssql_operator", "MsSqlOperator"]
        | ["airflow", "operators", "mysql_operator", "MySqlOperator"]
        | ["airflow", "operators", "oracle_operator", "OracleOperator"]
        | [
            "airflow",
            "operators",
            "postgres_operator",
            "PostgresOperator",
        ]
        | ["airflow", "operators", "sqlite_operator", "SqliteOperator"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.common.sql.operators.sql",
                name: "SQLExecuteQueryOperator",
                provider: "common-sql",
                version: "1.3.0",
            }
        }

        // apache-airflow-providers-daskexecutor
        ["airflow", "executors", "dask_executor", "DaskExecutor"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.daskexecutor.executors.dask_executor",
                name: "DaskExecutor",
                provider: "daskexecutor",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-docker
        ["airflow", "hooks", "docker_hook", "DockerHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.docker.hooks.docker",
            name: "DockerHook",
            provider: "docker",
            version: "1.0.0",
        },
        ["airflow", "operators", "docker_operator", "DockerOperator"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.docker.operators.docker",
                name: "DockerOperator",
                provider: "docker",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-apache-druid
        [
            "airflow",
            "hooks",
            "druid_hook",
            rest @ ("DruidDbApiHook" | "DruidHook"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.apache.druid.hooks.druid",
            provider: "apache-druid",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "hive_to_druid",
            "HiveToDruidOperator" | "HiveToDruidTransfer",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.druid.transfers.hive_to_druid",
            name: "HiveToDruidOperator",
            provider: "apache-druid",
            version: "1.0.0",
        },

        // apache-airflow-providers-fab
        [
            "airflow",
            "api",
            "auth",
            "backend",
            "basic_auth",
            rest @ ("CLIENT_AUTH" | "init_app" | "auth_current_user" | "requires_authentication"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.fab.auth_manager.api.auth.backend.basic_auth",
            provider: "fab",
            version: "1.0.0",
        },
        [
            "airflow",
            "api",
            "auth",
            "backend",
            "kerberos_auth",
            rest @ ("log" | "CLIENT_AUTH" | "find_user" | "init_app" | "requires_authentication"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
            provider: "fab",
            version: "1.0.0",
        },
        [
            "airflow",
            "auth",
            "managers",
            "fab",
            "api",
            "auth",
            "backend",
            "kerberos_auth",
            rest @ ("log" | "CLIENT_AUTH" | "find_user" | "init_app" | "requires_authentication"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
            provider: "fab",
            version: "1.0.0",
        },
        [
            "airflow",
            "auth",
            "managers",
            "fab",
            "fab_auth_manager",
            "FabAuthManager",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.fab.auth_manager.fab_auth_manager",
            name: "FabAuthManager",
            provider: "fab",
            version: "1.0.0",
        },
        [
            "airflow",
            "auth",
            "managers",
            "fab",
            "security_manager",
            "override",
            "MAX_NUM_DATABASE_USER_SESSIONS",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.fab.auth_manager.security_manager.override",
            name: "MAX_NUM_DATABASE_USER_SESSIONS",
            provider: "fab",
            version: "1.0.0",
        },
        [
            "airflow",
            "auth",
            "managers",
            "fab",
            "security_manager",
            "override",
            "FabAirflowSecurityManagerOverride",
        ]
        | [
            "airflow",
            "www",
            "security",
            "FabAirflowSecurityManagerOverride",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.fab.auth_manager.security_manager.override",
            name: "FabAirflowSecurityManagerOverride",
            provider: "fab",
            version: "1.0.0",
        },

        // apache-airflow-providers-apache-hdfs
        ["airflow", "hooks", "webhdfs_hook", "WebHDFSHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hdfs.hooks.webhdfs",
            name: "WebHDFSHook",
            provider: "apache-hdfs",
            version: "1.0.0",
        },
        ["airflow", "sensors", "web_hdfs_sensor", "WebHdfsSensor"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.apache.hdfs.sensors.web_hdfs",
                name: "WebHdfsSensor",
                provider: "apache-hdfs",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-apache-hive
        [
            "airflow",
            "hooks",
            "hive_hooks",
            rest @ ("HiveCliHook"
            | "HiveMetastoreHook"
            | "HiveServer2Hook"
            | "HIVE_QUEUE_PRIORITIES"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.apache.hive.hooks.hive",
            provider: "apache-hive",
            version: "1.0.0",
        },
        [
            "airflow",
            "macros",
            "hive",
            rest @ ("closest_ds_partition" | "max_partition"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.apache.hive.macros.hive",
            provider: "apache-hive",
            version: "5.1.0",
        },
        ["airflow", "operators", "hive_operator", "HiveOperator"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.apache.hive.operators.hive",
                name: "HiveOperator",
                provider: "apache-hive",
                version: "1.0.0",
            }
        }
        [
            "airflow",
            "operators",
            "hive_stats_operator",
            "HiveStatsCollectionOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hive.operators.hive_stats",
            name: "HiveStatsCollectionOperator",
            provider: "apache-hive",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "hive_to_mysql",
            "HiveToMySqlOperator" | "HiveToMySqlTransfer",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hive.transfers.hive_to_mysql",
            name: "HiveToMySqlOperator",
            provider: "apache-hive",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "hive_to_samba_operator",
            "HiveToSambaOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hive.transfers.hive_to_samba",
            name: "HiveToSambaOperator",
            provider: "apache-hive",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "mssql_to_hive",
            "MsSqlToHiveOperator" | "MsSqlToHiveTransfer",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hive.transfers.mssql_to_hive",
            name: "MsSqlToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "mysql_to_hive",
            "MySqlToHiveOperator" | "MySqlToHiveTransfer",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hive.transfers.mysql_to_hive",
            name: "MySqlToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "s3_to_hive_operator",
            "S3ToHiveOperator" | "S3ToHiveTransfer",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hive.transfers.s3_to_hive",
            name: "S3ToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0",
        },
        [
            "airflow",
            "sensors",
            "hive_partition_sensor",
            "HivePartitionSensor",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hive.sensors.hive_partition",
            name: "HivePartitionSensor",
            provider: "apache-hive",
            version: "1.0.0",
        },
        [
            "airflow",
            "sensors",
            "metastore_partition_sensor",
            "MetastorePartitionSensor",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hive.sensors.metastore_partition",
            name: "MetastorePartitionSensor",
            provider: "apache-hive",
            version: "1.0.0",
        },
        [
            "airflow",
            "sensors",
            "named_hive_partition_sensor",
            "NamedHivePartitionSensor",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.hive.sensors.named_hive_partition",
            name: "NamedHivePartitionSensor",
            provider: "apache-hive",
            version: "1.0.0",
        },

        // apache-airflow-providers-http
        ["airflow", "hooks", "http_hook", "HttpHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.http.hooks.http",
            name: "HttpHook",
            provider: "http",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "http_operator",
            "SimpleHttpOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.http.operators.http",
            name: "HttpOperator",
            provider: "http",
            version: "5.0.0",
        },
        ["airflow", "sensors", "http_sensor", "HttpSensor"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.http.sensors.http",
            name: "HttpSensor",
            provider: "http",
            version: "1.0.0",
        },

        // apache-airflow-providers-jdbc
        [
            "airflow",
            "hooks",
            "jdbc_hook",
            rest @ ("JdbcHook" | "jaydebeapi"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.jdbc.hooks.jdbc",
            provider: "jdbc",
            version: "1.0.0",
        },

        // apache-airflow-providers-cncf-kubernetes
        [
            "airflow",
            "executors",
            "kubernetes_executor_types",
            rest @ ("ALL_NAMESPACES" | "POD_EXECUTOR_DONE_KEY"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        [
            "airflow",
            "kubernetes",
            "k8s_model",
            rest @ ("K8SModel" | "append_to_pod"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.k8s_model",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        [
            "airflow",
            "kubernetes",
            "kube_client",
            rest @ ("_disable_verify_ssl" | "_enable_tcp_keepalive" | "get_kube_client"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.kube_client",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        [
            "airflow",
            "kubernetes",
            "kubernetes_helper_functions",
            "add_pod_suffix",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
            name: "add_unique_suffix",
            provider: "cncf-kubernetes",
            version: "10.0.0",
        },
        [
            "airflow",
            "kubernetes",
            "kubernetes_helper_functions",
            "create_pod_id",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
            name: "create_unique_id",
            provider: "cncf-kubernetes",
            version: "10.0.0",
        },
        [
            "airflow",
            "kubernetes",
            "kubernetes_helper_functions",
            rest @ ("annotations_for_logging_task_metadata"
            | "annotations_to_key"
            | "get_logs_task_metadata"
            | "rand_str"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "pod", rest] => match *rest {
            "Port" => ProviderReplacement::AutoImport {
                module: "kubernetes.client.models",
                name: "V1ContainerPort",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            },
            "Resources" => ProviderReplacement::AutoImport {
                module: "kubernetes.client.models",
                name: "V1ResourceRequirements",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            },
            _ => return,
        },
        ["airflow", "kubernetes", "pod_generator", rest] => match *rest {
            "datetime_to_label_safe_datestring"
            | "extend_object_field"
            | "label_safe_datestring_to_datetime"
            | "make_safe_label_value"
            | "merge_objects"
            | "PodGenerator" => ProviderReplacement::SourceModuleMovedToProvider {
                name: (*rest).to_string(),
                module: "airflow.providers.cncf.kubernetes.pod_generator",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            },
            "PodDefaults" => ProviderReplacement::AutoImport {
                module: "airflow.providers.cncf.kubernetes.utils.xcom_sidecar",
                name: "PodDefaults",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            },
            "PodGeneratorDeprecated" => ProviderReplacement::AutoImport {
                module: "airflow.providers.cncf.kubernetes.pod_generator",
                name: "PodGenerator",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            },
            "add_pod_suffix" => ProviderReplacement::AutoImport {
                module: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
                name: "add_unique_suffix",
                provider: "cncf-kubernetes",
                version: "10.0.0",
            },
            "rand_str" => ProviderReplacement::SourceModuleMovedToProvider {
                module: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
                name: "rand_str".to_string(),
                provider: "cncf-kubernetes",
                version: "7.4.0",
            },
            _ => return,
        },
        [
            "airflow",
            "kubernetes",
            "pod_generator_deprecated",
            rest @ ("make_safe_label_value" | "PodGenerator"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.pod_generator",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        [
            "airflow",
            "kubernetes",
            "pod_generator_deprecated" | "pod_launcher_deprecated",
            "PodDefaults",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.cncf.kubernetes.utils.xcom_sidecar",
            name: "PodDefaults",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        [
            "airflow",
            "kubernetes",
            "pod_launcher_deprecated",
            "get_kube_client",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.cncf.kubernetes.kube_client",
            name: "get_kube_client",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        [
            "airflow",
            "kubernetes",
            "pod_launcher" | "pod_launcher_deprecated",
            "PodLauncher",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.cncf.kubernetes.utils.pod_manager",
            name: "PodManager",
            provider: "cncf-kubernetes",
            version: "3.0.0",
        },
        [
            "airflow",
            "kubernetes",
            "pod_launcher" | "pod_launcher_deprecated",
            "PodStatus",
        ] => ProviderReplacement::AutoImport {
            module: " airflow.providers.cncf.kubernetes.utils.pod_manager",
            name: "PodPhase",
            provider: "cncf-kubernetes",
            version: "3.0.0",
        },
        [
            "airflow",
            "kubernetes",
            "pod_runtime_info_env",
            "PodRuntimeInfoEnv",
        ] => ProviderReplacement::AutoImport {
            module: "kubernetes.client.models",
            name: "V1EnvVar",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "secret", rest] => match *rest {
            "K8SModel" => ProviderReplacement::AutoImport {
                module: "airflow.providers.cncf.kubernetes.k8s_model",
                name: "K8SModel",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            },
            "Secret" => ProviderReplacement::AutoImport {
                module: "airflow.providers.cncf.kubernetes.secret",
                name: "Secret",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            },
            _ => return,
        },
        ["airflow", "kubernetes", "volume", "Volume"] => ProviderReplacement::AutoImport {
            module: "kubernetes.client.models",
            name: "V1Volume",
            provider: "cncf-kubernetes",
            version: "7.4.0",
        },
        ["airflow", "kubernetes", "volume_mount", "VolumeMount"] => {
            ProviderReplacement::AutoImport {
                module: "kubernetes.client.models",
                name: "V1VolumeMount",
                provider: "cncf-kubernetes",
                version: "7.4.0",
            }
        }

        // apache-airflow-providers-microsoft-mssql
        ["airflow", "hooks", "mssql_hook", "MsSqlHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.microsoft.mssql.hooks.mssql",
            name: "MsSqlHook",
            provider: "microsoft-mssql",
            version: "1.0.0",
        },

        // apache-airflow-providers-mysql
        ["airflow", "hooks", "mysql_hook", "MySqlHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.mysql.hooks.mysql",
            name: "MySqlHook",
            provider: "mysql",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "presto_to_mysql",
            "PrestoToMySqlOperator" | "PrestoToMySqlTransfer",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.mysql.transfers.presto_to_mysql",
            name: "PrestoToMySqlOperator",
            provider: "mysql",
            version: "1.0.0",
        },

        // apache-airflow-providers-oracle
        ["airflow", "hooks", "oracle_hook", "OracleHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.oracle.hooks.oracle",
            name: "OracleHook",
            provider: "oracle",
            version: "1.0.0",
        },

        // apache-airflow-providers-papermill
        [
            "airflow",
            "operators",
            "papermill_operator",
            "PapermillOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.papermill.operators.papermill",
            name: "PapermillOperator",
            provider: "papermill",
            version: "1.0.0",
        },

        // apache-airflow-providers-apache-pig
        ["airflow", "hooks", "pig_hook", "PigCliHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.apache.pig.hooks.pig",
            name: "PigCliHook",
            provider: "apache-pig",
            version: "1.0.0",
        },
        ["airflow", "operators", "pig_operator", "PigOperator"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.apache.pig.operators.pig",
                name: "PigOperator",
                provider: "apache-pig",
                version: "1.0.0",
            }
        }

        // apache-airflow-providers-postgres
        ["airflow", "hooks", "postgres_hook", "PostgresHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.postgres.hooks.postgres",
            name: "PostgresHook",
            provider: "postgres",
            version: "1.0.0",
        },
        ["airflow", "operators", "postgres_operator", "Mapping"] => ProviderReplacement::None,

        // apache-airflow-providers-presto
        ["airflow", "hooks", "presto_hook", "PrestoHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.presto.hooks.presto",
            name: "PrestoHook",
            provider: "presto",
            version: "1.0.0",
        },

        // apache-airflow-providers-samba
        ["airflow", "hooks", "samba_hook", "SambaHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.samba.hooks.samba",
            name: "SambaHook",
            provider: "samba",
            version: "1.0.0",
        },

        // apache-airflow-providers-slack
        ["airflow", "hooks", "slack_hook", "SlackHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.slack.hooks.slack",
            name: "SlackHook",
            provider: "slack",
            version: "1.0.0",
        },
        [
            "airflow",
            "operators",
            "slack_operator",
            rest @ ("SlackAPIOperator" | "SlackAPIPostOperator"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.slack.operators.slack",
            provider: "slack",
            version: "1.0.0",
        },

        // apache-airflow-providers-smtp
        [
            "airflow",
            "operators",
            "email_operator" | "email",
            "EmailOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.smtp.operators.smtp",
            name: "EmailOperator",
            provider: "smtp",
            version: "1.0.0",
        },

        // apache-airflow-providers-sqlite
        ["airflow", "hooks", "sqlite_hook", "SqliteHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.sqlite.hooks.sqlite",
            name: "SqliteHook",
            provider: "sqlite",
            version: "1.0.0",
        },

        // apache-airflow-providers-zendesk
        ["airflow", "hooks", "zendesk_hook", "ZendeskHook"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.zendesk.hooks.zendesk",
            name: "ZendeskHook",
            provider: "zendesk",
            version: "1.0.0",
        },

        // apache-airflow-providers-standard
        [
            "airflow",
            "hooks",
            "subprocess",
            rest @ ("SubprocessResult" | "working_directory"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.standard.hooks.subprocess",
            provider: "standard",
            version: "0.0.3",
        },
        ["airflow", "operators", "bash_operator", "BashOperator"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.standard.operators.bash",
                name: "BashOperator",
                provider: "standard",
                version: "0.0.1",
            }
        }
        [
            "airflow",
            "operators",
            "dagrun_operator",
            rest @ ("TriggerDagRunLink" | "TriggerDagRunOperator"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.standard.operators.trigger_dagrun",
            provider: "standard",
            version: "0.0.2",
        },
        [
            "airflow",
            "operators",
            "trigger_dagrun",
            "TriggerDagRunLink",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.standard.operators.trigger_dagrun",
            name: "TriggerDagRunLink",
            provider: "standard",
            version: "0.0.2",
        },
        ["airflow", "operators", "datetime", "target_times_as_dates"] => {
            ProviderReplacement::AutoImport {
                module: "airflow.providers.standard.operators.datetime",
                name: "target_times_as_dates",
                provider: "standard",
                version: "0.0.1",
            }
        }
        [
            "airflow",
            "operators",
            "dummy" | "dummy_operator",
            "EmptyOperator" | "DummyOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.standard.operators.empty",
            name: "EmptyOperator",
            provider: "standard",
            version: "0.0.2",
        },
        [
            "airflow",
            "operators",
            "latest_only_operator",
            "LatestOnlyOperator",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.standard.operators.latest_only",
            name: "LatestOnlyOperator",
            provider: "standard",
            version: "0.0.3",
        },
        [
            "airflow",
            "operators",
            "python_operator",
            rest @ ("BranchPythonOperator"
            | "PythonOperator"
            | "PythonVirtualenvOperator"
            | "ShortCircuitOperator"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.standard.operators.python",
            provider: "standard",
            version: "0.0.1",
        },
        [
            "airflow",
            "sensors",
            "external_task",
            "ExternalTaskSensorLink",
        ] => ProviderReplacement::AutoImport {
            module: "airflow.providers.standard.sensors.external_task",
            name: "ExternalDagLink",
            provider: "standard",
            version: "0.0.3",
        },
        [
            "airflow",
            "sensors",
            "external_task_sensor",
            rest @ ("ExternalTaskMarker" | "ExternalTaskSensor" | "ExternalTaskSensorLink"),
        ] => ProviderReplacement::SourceModuleMovedToProvider {
            module: "airflow.providers.standard.sensors.external_task",
            name: (*rest).to_string(),
            provider: "standard",
            version: "0.0.3",
        },
        ["airflow", "sensors", "time_delta", "WaitSensor"] => ProviderReplacement::AutoImport {
            module: "airflow.providers.standard.sensors.time_delta",
            name: "WaitSensor",
            provider: "standard",
            version: "0.0.1",
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

    let semantic = checker.semantic();
    if let Some((module, name)) = match &replacement {
        ProviderReplacement::AutoImport { module, name, .. } => Some((module, *name)),
        ProviderReplacement::SourceModuleMovedToProvider { module, name, .. } => {
            Some((module, name.as_str()))
        }
        ProviderReplacement::None => None,
    } {
        if is_guarded_by_try_except(expr, module, name, semantic) {
            return;
        }
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from(module, name),
                expr.start(),
                checker.semantic(),
            )?;
            let replacement_edit = Edit::range_replacement(binding, ranged.range());
            Ok(Fix::safe_edits(import_edit, [replacement_edit]))
        });
    }
    checker.report_diagnostic(diagnostic);
}
