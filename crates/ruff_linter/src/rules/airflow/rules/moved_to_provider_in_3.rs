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
            Replacement::SourceModuleMovedToProvider {
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
        if let Replacement::ProviderName {
            name,
            provider,
            version,
        } = replacement
        {
            Some(format!(
                "Install `apache-airflow-provider-{provider}>={version}` and use `{name}` instead."
            ))
        } else if let Replacement::SourceModuleMovedToProvider {
            name,
            module,
            provider,
            version,
        } = replacement
        {
            Some(format!("Install `apache-airflow-provider-{provider}>={version}` and use `{module}.{name}` instead."))
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
    SourceModuleMovedToProvider {
        name: String,
        module: &'static str,
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
        ["airflow", "hooks", "S3_hook", rest @ (
            "S3Hook" |
            "provide_bucket_name"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.amazon.aws.hooks.s3",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "gcs_to_s3", "GCSToS3Operator"] => Replacement::ProviderName {
            name: "airflow.providers.amazon.aws.transfers.gcs_to_s3.GCSToS3Operator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "google_api_to_s3_transfer", "GoogleApiToS3Operator" | "GoogleApiToS3Transfer"] => Replacement::ProviderName {
            name: "airflow.providers.amazon.aws.transfers.google_api_to_s3.GoogleApiToS3Operator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "redshift_to_s3_operator", "RedshiftToS3Operator" | "RedshiftToS3Transfer"] => Replacement::ProviderName {
            name: "airflow.providers.amazon.aws.transfers.redshift_to_s3.RedshiftToS3Operator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "s3_file_transform_operator", "S3FileTransformOperator"] => Replacement::ProviderName {
            name: "airflow.providers.amazon.aws.operators.s3_file_transform.S3FileTransformOperator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "operators", "s3_to_redshift_operator", "S3ToRedshiftOperator" | "S3ToRedshiftTransfer"] => Replacement::ProviderName {
            name: "airflow.providers.amazon.aws.transfers.s3_to_redshift.S3ToRedshiftOperator",
            provider: "amazon",
            version: "1.0.0"
        },
        ["airflow", "sensors", "s3_key_sensor", "S3KeySensor"] => Replacement::ProviderName {
            name: "airflow.providers.amazon.aws.sensors.s3.S3KeySensor",
            provider: "amazon",
            version: "1.0.0"
        },

        // apache-airflow-providers-celery
        ["airflow", "config_templates", "default_celery", "DEFAULT_CELERY_CONFIG"] => Replacement::ProviderName {
            name: "airflow.providers.celery.executors.default_celery.DEFAULT_CELERY_CONFIG",
            provider: "celery",
            version: "3.3.0"
        },
        ["airflow", "executors", "celery_executor", rest ] => match *rest {
            "app" => Replacement::ProviderName {
                name: "airflow.providers.celery.executors.celery_executor_utils.app",
                provider: "celery",
                version: "3.3.0"
            },
            "CeleryExecutor" => Replacement::ProviderName {
                name: "airflow.providers.celery.executors.celery_executor.CeleryExecutor",
                provider: "celery",
                version: "3.3.0"
            },
            _ => return,
        },
        ["airflow", "executors", "celery_kubernetes_executor", "CeleryKubernetesExecutor"] => Replacement::ProviderName {
            name: "airflow.providers.celery.executors.celery_kubernetes_executor.CeleryKubernetesExecutor",
            provider: "celery",
            version: "3.3.0"
        },

        // apache-airflow-providers-common-sql
        ["airflow", "hooks", "dbapi", rest @ (
            "ConnectorProtocol" |
            "DbApiHook"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.common.sql.hooks.sql",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "hooks", "dbapi_hook", "DbApiHook"] => Replacement::ProviderName {
            name: "airflow.providers.common.sql.hooks.sql.DbApiHook",
            provider: "common-sql",
            version: "1.0.0"
        },
        ["airflow", "operators", "check_operator", rest] => match *rest {
            "SQLCheckOperator" | "CheckOperator" => Replacement::ProviderName {
                name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
                provider: "common-sql",
                version: "1.1.0"
            },
            "SQLIntervalCheckOperator" | "IntervalCheckOperator" => Replacement::ProviderName {
                name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
                provider: "common-sql",
                version: "1.1.0"
            },
            "SQLThresholdCheckOperator" | "ThresholdCheckOperator" => Replacement::ProviderName {
                name: "airflow.providers.common.sql.operators.sql.SQLThresholdCheckOperator",
                provider: "common-sql",
                version: "1.1.0"
            },
            "SQLValueCheckOperator" | "ValueCheckOperator" => Replacement::ProviderName {
                name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
                provider: "common-sql",
                version: "1.1.0"
            },
            _ => return
        },
        ["airflow", "operators", "presto_check_operator", rest] => match *rest {
            "SQLCheckOperator" | "PrestoCheckOperator" => Replacement::ProviderName {
                name: "airflow.providers.common.sql.operators.sql.SQLCheckOperator",
                provider: "common-sql",
                version: "1.1.0"
            },
            "SQLIntervalCheckOperator" | "PrestoIntervalCheckOperator" => Replacement::ProviderName {
                name: "airflow.providers.common.sql.operators.sql.SQLIntervalCheckOperator",
                provider: "common-sql",
                version: "1.1.0"
            },
            "SQLValueCheckOperator" | "PrestoValueCheckOperator" => Replacement::ProviderName {
                name: "airflow.providers.common.sql.operators.sql.SQLValueCheckOperator",
                provider: "common-sql",
                version: "1.1.0"
            },
            _ => return
        }
        ["airflow", "operators", "sql", rest] => match *rest {
            "BaseSQLOperator" |
            "BranchSQLOperator" |
            "SQLCheckOperator" |
            "SQLIntervalCheckOperator" |
            "SQLTablecheckOperator"  |
            "SQLThresholdCheckOperator" => Replacement::SourceModuleMovedToProvider {
                name: (*rest).to_string(),
                module: "airflow.providers.common.sql.operators.sql",
                provider: "common-sql",
                version: "1.1.0"
            },
            "SQLColumnCheckOperator" |
            "SQLValueCheckOperator" |
            "_convert_to_float_if_possible" |
            "parse_boolean" => Replacement::SourceModuleMovedToProvider {
                name: (*rest).to_string(),
                module: "airflow.providers.common.sql.operators.sql",
                provider: "common-sql",
                version: "1.0.0"
            },
            _ => return
        }
        ["airflow", "sensors", "sql" | "sql_sensor", "SqlSensor"] => Replacement::ProviderName {
            name: "airflow.providers.common.sql.sensors.sql.SqlSensor",
            provider: "common-sql",
            version: "1.0.0"
        },

        // apache-airflow-providers-daskexecutor
        ["airflow", "executors", "dask_executor", "DaskExecutor"] => Replacement::ProviderName {
            name: "airflow.providers.daskexecutor.executors.dask_executor.DaskExecutor",
            provider: "daskexecutor",
            version: "1.0.0"
        },

        // apache-airflow-providers-docker
        ["airflow", "hooks", "docker_hook", "DockerHook"] => Replacement::ProviderName {
            name: "airflow.providers.docker.hooks.docker.DockerHook",
            provider: "docker",
            version: "1.0.0"
        },
        ["airflow", "operators", "docker_operator", "DockerOperator"] => Replacement::ProviderName {
            name: "airflow.providers.docker.operators.docker.DockerOperator",
            provider: "docker",
            version: "1.0.0"
        },

        // apache-airflow-providers-apache-druid
        ["airflow", "hooks", "druid_hook", rest @ (
            "DruidDbApiHook" |
            "DruidHook"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.apache.druid.hooks.druid",
            provider: "apache-druid",
            version: "1.0.0"
        },
        ["airflow", "operators", "druid_check_operator", "DruidCheckOperator"] => Replacement::ProviderName {
            name: "DruidCheckOperator",
            provider: "apache-druid",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_to_druid", "HiveToDruidOperator" | "HiveToDruidTransfer"] => Replacement::ProviderName {
            name: "airflow.providers.apache.druid.transfers.hive_to_druid.HiveToDruidOperator",
            provider: "apache-druid",
            version: "1.0.0"
        },

        // apache-airflow-providers-fab
        ["airflow", "api", "auth", "backend", "basic_auth", rest @ (
            "CLIENT_AUTH" |
            "init_app" |
            "auth_current_user" |
            "requires_authentication"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.fab.auth_manager.api.auth.backend.basic_auth",
            provider: "fab",
            version: "1.0.0"
        },
        ["airflow", "api", "auth", "backend", "kerberos_auth", rest @ (
            "log" |
            "CLIENT_AUTH" |
            "find_user" |
            "init_app" |
            "requires_authentication"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
            provider: "fab",
            version: "1.0.0"
        },
        ["airflow", "auth", "managers", "fab", "api", "auth", "backend", "kerberos_auth", rest @ (
            "log" |
            "CLIENT_AUTH" |
            "find_user" |
            "init_app" |
            "requires_authentication"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.fab.auth_manager.api.auth.backend.kerberos_auth",
            provider: "fab",
            version: "1.0.0"
        },
        ["airflow", "auth", "managers", "fab", "fab_auth_manager", "FabAuthManager"] => Replacement::ProviderName {
            name: "airflow.providers.fab.auth_manager.security_manager.FabAuthManager",
            provider: "fab",
            version: "1.0.0"
        },
        ["airflow", "auth", "managers", "fab", "security_manager", "override", rest @ (
            "MAX_NUM_DATABASE_USER_SESSIONS" |
            "FabAirflowSecurityManagerOverride"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.fab.auth_manager.security_manager.override",
            provider: "fab",
            version: "1.0.0"
        },
        ["airflow", "www", "security", "FabAirflowSecurityManagerOverride"] => Replacement::ProviderName {
            name: "airflow.providers.fab.auth_manager.security_manager.override.FabAirflowSecurityManagerOverride",
            provider: "fab",
            version: "1.0.0"
        },

        // apache-airflow-providers-apache-hdfs
        ["airflow", "hooks", "webhdfs_hook", "WebHDFSHook"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hdfs.hooks.webhdfs.WebHDFSHook",
            provider: "apache-hdfs",
            version: "1.0.0"
        },
        ["airflow", "sensors", "web_hdfs_sensor", "WebHdfsSensor"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hdfs.sensors.web_hdfs.WebHdfsSensor",
            provider: "apache-hdfs",
            version: "1.0.0"
        },

        // apache-airflow-providers-apache-hive
        ["airflow", "macros", "hive", rest @ (
            "closest_ds_partition" |
            "max_partition"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.apache.hive.macros.hive",
            provider: "apache-hive",
            version: "5.1.0"
        },
        ["airflow", "hooks", "hive_hooks", rest @ (
            "HiveCliHook" |
            "HiveMetastoreHook" |
            "HiveServer2Hook" |
            "HIVE_QUEUE_PRIORITIES"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.apache.hive.hooks.hive",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_operator", "HiveOperator"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hive.operators.hive.HiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_stats_operator", "HiveStatsCollectionOperator"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hive.operators.hive_stats.HiveStatsCollectionOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_to_mysql", "HiveToMySqlOperator" | "HiveToMySqlTransfer"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hive.transfers.hive_to_mysql.HiveToMySqlOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "hive_to_samba_operator", "HiveToSambaOperator"] => Replacement::ProviderName {
            name: "HiveToSambaOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "mssql_to_hive", "MsSqlToHiveOperator" | "MsSqlToHiveTransfer"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hive.transfers.mssql_to_hive.MsSqlToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "mysql_to_hive", "MySqlToHiveOperator" | "MySqlToHiveTransfer"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hive.transfers.mysql_to_hive.MySqlToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "operators", "s3_to_hive_operator", "S3ToHiveOperator" | "S3ToHiveTransfer"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hive.transfers.s3_to_hive.S3ToHiveOperator",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "sensors", "hive_partition_sensor", "HivePartitionSensor"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hive.sensors.hive_partition.HivePartitionSensor",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "sensors", "metastore_partition_sensor", "MetastorePartitionSensor"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hive.sensors.metastore_partition.MetastorePartitionSensor",
            provider: "apache-hive",
            version: "1.0.0"
        },
        ["airflow", "sensors", "named_hive_partition_sensor", "NamedHivePartitionSensor"] => Replacement::ProviderName {
            name: "airflow.providers.apache.hive.sensors.named_hive_partition.NamedHivePartitionSensor",
            provider: "apache-hive",
            version: "1.0.0"
        },

        // apache-airflow-providers-http
        ["airflow", "hooks", "http_hook", "HttpHook"] => Replacement::ProviderName {
            name: "airflow.providers.http.hooks.http.HttpHook",
            provider: "http",
            version: "1.0.0"
        },
        ["airflow", "operators", "http_operator", "SimpleHttpOperator"] => Replacement::ProviderName {
            name: "airflow.providers.http.operators.http.SimpleHttpOperator",
            provider: "http",
            version: "1.0.0"
        },
        ["airflow", "sensors", "http_sensor", "HttpSensor"] => Replacement::ProviderName {
            name: "airflow.providers.http.sensors.http.HttpSensor",
            provider: "http",
            version: "1.0.0"
        },

        // apache-airflow-providers-jdbc
        ["airflow", "hooks", "jdbc_hook", rest @ (
            "JdbcHook" |
            "jaydebeapi"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.jdbc.hooks.jdbc",
            provider: "jdbc",
            version: "1.0.0"
        },
        ["airflow", "operators", "jdbc_operator", "JdbcOperator"] => Replacement::ProviderName {
            name: "airflow.providers.jdbc.operators.jdbc.JdbcOperator",
            provider: "jdbc",
            version: "1.0.0"
        },

        // apache-airflow-providers-cncf-kubernetes
        ["airflow", "executors", "kubernetes_executor_types", rest @ (
            "ALL_NAMESPACES" |
            "POD_EXECUTOR_DONE_KEY"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.executors.kubernetes_executor_types",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "k8s_model", rest@ (
            "K8SModel" |
            "append_to_pod"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.k8s_model",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kube_client", rest @ (
            "_disable_verify_ssl" |
            "_enable_tcp_keepalive" |
            "get_kube_client"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.kubernetes.airflow.providers.cncf.kubernetes.kube_client",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "kubernetes_helper_functions", rest@ (
            "add_pod_suffix" |
            "annotations_for_logging_task_metadata" |
            "annotations_to_key" |
            "create_pod_id" |
            "get_logs_task_metadata" |
            "rand_str"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod", rest] => match *rest {
            "Port" =>Replacement::ProviderName {
                name: "kubernetes.client.models.V1ContainerPort",
                provider: "cncf-kubernetes",
                version: "7.4.0"
            },
            "Resources" => Replacement::ProviderName {
                name: "kubernetes.client.models.V1ResourceRequirements",
                provider: "cncf-kubernetes",
                version: "7.4.0"
            },
            _ => return
        },
        ["airflow", "kubernetes", "pod_generator", rest ] => match *rest {
            "datetime_to_label_safe_datestring" |
            "extend_object_field" |
            "label_safe_datestring_to_datetime" |
            "make_safe_label_value" |
            "merge_objects" |
            "PodGenerator" |
            "PodDefaults" => Replacement::SourceModuleMovedToProvider {
                name: (*rest).to_string(),
                module: "airflow.providers.cncf.kubernetes.pod_generator",
                provider: "cncf-kubernetes",
                version: "7.4.0"
            },
            "PodGeneratorDeprecated" => Replacement::ProviderName {
                name: "airflow.providers.cncf.kubernetes.pod_generator.PodGenerator",
                provider: "cncf-kubernetes",
                version: "7.4.0"
            },
            "add_pod_suffix" |
            "rand_str" => Replacement::SourceModuleMovedToProvider {
                name: (*rest).to_string(),
                module: "airflow.providers.cncf.kubernetes.kubernetes_helper_functions",
                provider: "cncf-kubernetes",
                version: "7.4.0"
            },
            _ => return,
        },
        ["airflow", "kubernetes", "pod_generator_deprecated", rest@ (
            "make_safe_label_value" |
            "PodDefaults" |
            "PodGenerator"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.pod_generator_deprecated",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_launcher", rest @(
            "PodLauncher" |
            "PodStatus"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.cncf.kubernetes.pod_launcher_deprecated",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "pod_launcher_deprecated", rest] => match *rest {
            "PodLauncher" | "PodStatus" | "PodDefaults" => Replacement::SourceModuleMovedToProvider {
                name: (*rest).to_string(),
                module: "airflow.providers.cncf.kubernetes.pod_launcher_deprecated",
                provider: "cncf-kubernetes",
                version: "7.4.0"
            },
            "get_kube_client" => Replacement::ProviderName {
                name: "airflow.providers.cncf.kubernetes.kube_client.get_kube_client",
                provider: "cncf-kubernetes",
                version: "7.4.0"
            },
            _ => return,
        }
        ["airflow", "kubernetes", "pod_runtime_info_env", "PodRuntimeInfoEnv"] => Replacement::ProviderName {
            name: "kubernetes.client.models.V1EnvVar",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "secret", rest ] => match *rest {
            "K8SModel" => Replacement::ProviderName {
                name: "airflow.providers.cncf.kubernetes.k8s_model.K8SModel",
                provider: "cncf-kubernetes",
                version: "7.4.0"
            },
            "Secret" => Replacement::ProviderName {
                name: "airflow.providers.cncf.kubernetes.secret.Secret",
                provider: "cncf-kubernetes",
                version: "7.4.0"
            },
            _ => return,
        },
        ["airflow", "kubernetes", "volume", "Volume"] => Replacement::ProviderName {
            name: "kubernetes.client.models.V1Volume",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },
        ["airflow", "kubernetes", "volume_mount", "VolumeMount"] => Replacement::ProviderName {
            name: "kubernetes.client.models.V1VolumeMount",
            provider: "cncf-kubernetes",
            version: "7.4.0"
        },

        // apache-airflow-providers-microsoft-mssql
        ["airflow", "hooks", "mssql_hook", "MsSqlHook"] => Replacement::ProviderName {
            name: "airflow.providers.microsoft.mssql.hooks.mssql.MsSqlHook",
            provider: "microsoft-mssql",
            version: "1.0.0"
        },
        ["airflow", "operators", "mssql_operator", "MsSqlOperator"] => Replacement::ProviderName {
            name: "airflow.providers.microsoft.mssql.operators.mssql.MsSqlOperator",
            provider: "microsoft-mssql",
            version: "1.0.0"
        },

        // apache-airflow-providers-mysql
        ["airflow", "hooks", "mysql_hook", "MySqlHook"] => Replacement::ProviderName {
            name: "airflow.providers.mysql.hooks.mysql.MySqlHook",
            provider: "mysql",
            version: "1.0.0"
        },
        ["airflow", "operators", "mysql_operator", "MySqlOperator"] => Replacement::ProviderName {
            name: "airflow.providers.mysql.operators.mysql.MySqlOperator",
            provider: "mysql",
            version: "1.0.0"
        },
        ["airflow", "operators", "presto_to_mysql", "PrestoToMySqlOperator" | "PrestoToMySqlTransfer"] => Replacement::ProviderName {
            name: "airflow.providers.mysql.transfers.presto_to_mysql.PrestoToMySqlOperator",
            provider: "mysql",
            version: "1.0.0"
        },

        // apache-airflow-providers-oracle
        ["airflow", "hooks", "oracle_hook", "OracleHook"] => Replacement::ProviderName {
            name: "airflow.providers.oracle.hooks.oracle.OracleHook",
            provider: "oracle",
            version: "1.0.0"
        },
        ["airflow", "operators", "oracle_operator", "OracleOperator"] => Replacement::ProviderName {
            name: "airflow.providers.oracle.operators.oracle.OracleOperator",
            provider: "oracle",
            version: "1.0.0"
        },

        // apache-airflow-providers-papermill
        ["airflow", "operators", "papermill_operator", "PapermillOperator"] => Replacement::ProviderName {
            name: "airflow.providers.papermill.operators.papermill.PapermillOperator",
            provider: "papermill",
            version: "1.0.0"
        },

        // apache-airflow-providers-apache-pig
        ["airflow", "hooks", "pig_hook", "PigCliHook"] => Replacement::ProviderName {
            name: "airflow.providers.apache.pig.hooks.pig.PigCliHook",
            provider: "apache-pig",
            version: "1.0.0"
        },
        ["airflow", "operators", "pig_operator", "PigOperator"] => Replacement::ProviderName {
            name: "airflow.providers.apache.pig.operators.pig.PigOperator",
            provider: "apache-pig",
            version: "1.0.0"
        },

        // apache-airflow-providers-postgres
        ["airflow", "hooks", "postgres_hook", "PostgresHook"] => Replacement::ProviderName {
            name: "airflow.providers.postgres.hooks.postgres.PostgresHook",
            provider: "postgres",
            version: "1.0.0"
        },
        ["airflow", "operators", "postgres_operator", rest @ (
            "Mapping" |
            "PostgresOperator"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.postgres.operators.postgres",
            provider: "postgres",
            version: "1.0.0"
        },

        // apache-airflow-providers-presto
        ["airflow", "hooks", "presto_hook", "PrestoHook"] => Replacement::ProviderName {
            name: "airflow.providers.presto.hooks.presto.PrestoHook",
            provider: "presto",
            version: "1.0.0"
        },

        // apache-airflow-providers-samba
        ["airflow", "hooks", "samba_hook", "SambaHook"] => Replacement::ProviderName {
            name: "airflow.providers.samba.hooks.samba.SambaHook",
            provider: "samba",
            version: "1.0.0"
        },

        // apache-airflow-providers-slack
        ["airflow", "hooks", "slack_hook", "SlackHook"] => Replacement::ProviderName {
            name: "airflow.providers.slack.hooks.slack.SlackHook",
            provider: "slack",
            version: "1.0.0"
        },
        ["airflow", "operators", "slack_operator", rest @ (
            "SlackAPIOperator" |
            "SlackAPIPostOperator"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.slack.operators.slack",
            provider: "slack",
            version: "1.0.0"
        },

        // apache-airflow-providers-smtp
        ["airflow", "operators", "email_operator" | "email", "EmailOperator"] => Replacement::ProviderName {
            name: "airflow.providers.smtp.operators.smtp.EmailOperator",
            provider: "smtp",
            version: "1.0.0",
        },

        // apache-airflow-providers-sqlite
        ["airflow", "hooks", "sqlite_hook", "SqliteHook"] => Replacement::ProviderName {
            name: "airflow.providers.sqlite.hooks.sqlite.SqliteHook",
            provider: "sqlite",
            version: "1.0.0"
        },
        ["airflow", "operators", "sqlite_operator", "SqliteOperator"] => Replacement::ProviderName {
            name: "airflow.providers.sqlite.operators.sqlite.SqliteOperator",
            provider: "sqlite",
            version: "1.0.0"
        },

        // apache-airflow-providers-zendesk
        ["airflow", "hooks", "zendesk_hook", "ZendeskHook"] =>
            Replacement::ProviderName {
            name: "airflow.providers.zendesk.hooks.zendesk.ZendeskHook",
            provider: "zendesk",
            version: "1.0.0"
        },

        // apache-airflow-providers-standard
        ["airflow", "operators", "bash_operator", "BashOperator"] => Replacement::ProviderName {
            name: "airflow.providers.standard.operators.bash.BashOperator",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "operators", "dagrun_operator", rest@ (
            "TriggerDagRunLink" |
            "TriggerDagRunOperator"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.standard.operators.trigger_dagrun",
            provider: "standard",
            version: "0.0.2"
        },
        ["airflow", "operators", "dummy" | "dummy_operator", "EmptyOperator" | "DummyOperator"] => Replacement::ProviderName {
            name: "airflow.providers.standard.operators.empty.EmptyOperator",
            provider: "standard",
            version: "0.0.2"
        },
        ["airflow", "operators", "latest_only_operator", "LatestOnlyOperator"] => Replacement::ProviderName {
            name: "airflow.providers.standard.operators.latest_only.LatestOnlyOperator",
            provider: "standard",
            version: "0.0.3"
        },
        ["airflow", "operators", "python_operator", rest @ (
            "BranchPythonOperator" |
            "PythonOperator" |
            "PythonVirtualenvOperator" |
            "ShortCircuitOperator"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.standard.operators.python",
            provider: "standard",
            version: "0.0.1"
        },
        ["airflow", "sensors", "external_task_sensor", rest @ (
            "ExternalTaskMarker" |
            "ExternalTaskSensor" |
            "ExternalTaskSensorLink"
        )] => Replacement::SourceModuleMovedToProvider {
            name: (*rest).to_string(),
            module: "airflow.providers.standard.sensors.external_task",
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
