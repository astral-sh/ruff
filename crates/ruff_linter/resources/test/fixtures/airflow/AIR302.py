from __future__ import annotations

from airflow.api.auth.backend import basic_auth, kerberos_auth
from airflow.api.auth.backend.basic_auth import auth_current_user
from airflow.auth.managers.fab.api.auth.backend import (
    kerberos_auth as backend_kerberos_auth,
)
from airflow.auth.managers.fab.fab_auth_manager import FabAuthManager
from airflow.auth.managers.fab.security_manager import override as fab_override
from airflow.config_templates.default_celery import DEFAULT_CELERY_CONFIG
from airflow.executors.celery_executor import CeleryExecutor, app
from airflow.executors.celery_kubernetes_executor import CeleryKubernetesExecutor
from airflow.executors.dask_executor import DaskExecutor
from airflow.executors.kubernetes_executor_types import (
    ALL_NAMESPACES,
    POD_EXECUTOR_DONE_KEY,
)
from airflow.hooks.dbapi import ConnectorProtocol, DbApiHook
from airflow.hooks.dbapi_hook import DbApiHook as DbApiHook2
from airflow.hooks.docker_hook import DockerHook
from airflow.hooks.druid_hook import DruidDbApiHook, DruidHook
from airflow.hooks.filesystem import FSHook
from airflow.hooks.hive_hooks import (
    HIVE_QUEUE_PRIORITIES,
    HiveCliHook,
    HiveMetastoreHook,
    HiveServer2Hook,
)
from airflow.hooks.http_hook import HttpHook
from airflow.hooks.jdbc_hook import JdbcHook, jaydebeapi
from airflow.hooks.mssql_hook import MsSqlHook
from airflow.hooks.mysql_hook import MySqlHook
from airflow.hooks.oracle_hook import OracleHook
from airflow.hooks.package_index import PackageIndexHook
from airflow.hooks.pig_hook import PigCliHook
from airflow.hooks.postgres_hook import PostgresHook
from airflow.hooks.presto_hook import PrestoHook
from airflow.hooks.S3_hook import S3Hook, provide_bucket_name
from airflow.hooks.samba_hook import SambaHook
from airflow.hooks.slack_hook import SlackHook
from airflow.hooks.sqlite_hook import SqliteHook
from airflow.hooks.subprocess import SubprocessHook
from airflow.hooks.webhdfs_hook import WebHDFSHook
from airflow.hooks.zendesk_hook import ZendeskHook
from airflow.kubernetes.k8s_model import K8SModel, append_to_pod
from airflow.kubernetes.kube_client import (
    _disable_verify_ssl,
    _enable_tcp_keepalive,
    get_kube_client,
)
from airflow.kubernetes.kubernetes_helper_functions import (
    add_pod_suffix,
    annotations_for_logging_task_metadata,
    annotations_to_key,
    create_pod_id,
    get_logs_task_metadata,
    rand_str,
)
from airflow.kubernetes.pod import Port, Resources
from airflow.kubernetes.pod_generator import (
    PodDefaults,
    PodGenerator,
    PodGeneratorDeprecated,
    datetime_to_label_safe_datestring,
    extend_object_field,
    label_safe_datestring_to_datetime,
    make_safe_label_value,
    merge_objects,
)
from airflow.kubernetes.pod_generator import (
    add_pod_suffix as add_pod_suffix2,
)
from airflow.kubernetes.pod_generator import (
    rand_str as rand_str2,
)
from airflow.kubernetes.pod_generator_deprecated import (
    PodDefaults as PodDefaults3,
)
from airflow.kubernetes.pod_generator_deprecated import (
    PodGenerator as PodGenerator2,
)
from airflow.kubernetes.pod_generator_deprecated import (
    make_safe_label_value as make_safe_label_value2,
)
from airflow.kubernetes.pod_launcher import PodLauncher, PodStatus
from airflow.kubernetes.pod_launcher_deprecated import (
    PodDefaults as PodDefaults2,
)
from airflow.kubernetes.pod_launcher_deprecated import (
    PodLauncher as PodLauncher2,
)
from airflow.kubernetes.pod_launcher_deprecated import (
    PodStatus as PodStatus2,
)
from airflow.kubernetes.pod_launcher_deprecated import (
    get_kube_client as get_kube_client2,
)
from airflow.kubernetes.pod_runtime_info_env import PodRuntimeInfoEnv
from airflow.kubernetes.secret import K8SModel2, Secret
from airflow.kubernetes.volume import Volume
from airflow.kubernetes.volume_mount import VolumeMount
from airflow.macros.hive import closest_ds_partition, max_partition
from airflow.operators.bash import BashOperator
from airflow.operators.bash_operator import BashOperator as LegacyBashOperator
from airflow.operators.check_operator import (
    CheckOperator,
    IntervalCheckOperator,
    SQLCheckOperator,
    SQLIntervalCheckOperator,
    SQLThresholdCheckOperator,
    SQLValueCheckOperator,
    ThresholdCheckOperator,
    ValueCheckOperator,
)
from airflow.operators.datetime import BranchDateTimeOperator
from airflow.operators.docker_operator import DockerOperator
from airflow.operators.druid_check_operator import DruidCheckOperator
from airflow.operators.gcs_to_s3 import GCSToS3Operator
from airflow.operators.google_api_to_s3_transfer import (
    GoogleApiToS3Operator,
    GoogleApiToS3Transfer,
)
from airflow.operators.hive_operator import HiveOperator
from airflow.operators.hive_stats_operator import HiveStatsCollectionOperator
from airflow.operators.hive_to_druid import HiveToDruidOperator, HiveToDruidTransfer
from airflow.operators.hive_to_mysql import HiveToMySqlOperator, HiveToMySqlTransfer
from airflow.operators.hive_to_samba_operator import HiveToSambaOperator
from airflow.operators.http_operator import SimpleHttpOperator
from airflow.operators.jdbc_operator import JdbcOperator
from airflow.operators.mssql_operator import MsSqlOperator
from airflow.operators.mssql_to_hive import MsSqlToHiveOperator, MsSqlToHiveTransfer
from airflow.operators.mysql_operator import MySqlOperator
from airflow.operators.mysql_to_hive import MySqlToHiveOperator, MySqlToHiveTransfer
from airflow.operators.oracle_operator import OracleOperator
from airflow.operators.papermill_operator import PapermillOperator
from airflow.operators.pig_operator import PigOperator
from airflow.operators.postgres_operator import Mapping, PostgresOperator
from airflow.operators.presto_check_operator import (
    PrestoCheckOperator,
    PrestoIntervalCheckOperator,
    PrestoValueCheckOperator,
)
from airflow.operators.presto_check_operator import (
    SQLCheckOperator as SQLCheckOperator2,
)
from airflow.operators.presto_check_operator import (
    SQLIntervalCheckOperator as SQLIntervalCheckOperator2,
)
from airflow.operators.presto_check_operator import (
    SQLValueCheckOperator as SQLValueCheckOperator2,
)
from airflow.operators.presto_to_mysql import (
    PrestoToMySqlOperator,
    PrestoToMySqlTransfer,
)
from airflow.operators.redshift_to_s3_operator import (
    RedshiftToS3Operator,
    RedshiftToS3Transfer,
)
from airflow.operators.s3_file_transform_operator import S3FileTransformOperator
from airflow.operators.s3_to_hive_operator import S3ToHiveOperator, S3ToHiveTransfer
from airflow.operators.s3_to_redshift_operator import (
    S3ToRedshiftOperator,
    S3ToRedshiftTransfer,
)
from airflow.operators.slack_operator import SlackAPIOperator, SlackAPIPostOperator
from airflow.operators.sql import (
    BaseSQLOperator,
    BranchSQLOperator,
    SQLTableCheckOperator,
    _convert_to_float_if_possible,
    parse_boolean,
)
from airflow.operators.sql import (
    SQLCheckOperator as SQLCheckOperator3,
)
from airflow.operators.sql import (
    SQLColumnCheckOperator as SQLColumnCheckOperator2,
)
from airflow.operators.sql import (
    SQLIntervalCheckOperator as SQLIntervalCheckOperator3,
)
from airflow.operators.sql import (
    SQLThresholdCheckOperator as SQLThresholdCheckOperator2,
)
from airflow.operators.sql import (
    SQLValueCheckOperator as SQLValueCheckOperator3,
)
from airflow.operators.sqlite_operator import SqliteOperator
from airflow.operators.trigger_dagrun import TriggerDagRunOperator
from airflow.operators.weekday import BranchDayOfWeekOperator
from airflow.sensors.date_time import DateTimeSensor
from airflow.sensors.external_task import ExternalTaskMarker, ExternalTaskSensor
from airflow.sensors.filesystem import FileSensor
from airflow.sensors.hive_partition_sensor import HivePartitionSensor
from airflow.sensors.http_sensor import HttpSensor
from airflow.sensors.metastore_partition_sensor import MetastorePartitionSensor
from airflow.sensors.named_hive_partition_sensor import NamedHivePartitionSensor
from airflow.sensors.s3_key_sensor import S3KeySensor
from airflow.sensors.sql import SqlSensor
from airflow.sensors.sql_sensor import SqlSensor2
from airflow.sensors.time_delta import TimeDeltaSensor
from airflow.sensors.time_sensor import TimeSensor
from airflow.sensors.web_hdfs_sensor import WebHdfsSensor
from airflow.sensors.weekday import DayOfWeekSensor
from airflow.triggers.external_task import WorkflowTrigger
from airflow.triggers.file import FileTrigger
from airflow.triggers.temporal import DateTimeTrigger
from airflow.www.security import FabAirflowSecurityManagerOverride

# apache-airflow-providers-amazon
provide_bucket_name()
GCSToS3Operator()
GoogleApiToS3Operator()
GoogleApiToS3Transfer()
RedshiftToS3Operator()
RedshiftToS3Transfer()
S3FileTransformOperator()
S3Hook()
S3KeySensor()
S3ToRedshiftOperator()
S3ToRedshiftTransfer()

# apache-airflow-providers-celery
DEFAULT_CELERY_CONFIG
app
CeleryExecutor()
CeleryKubernetesExecutor()

# apache-airflow-providers-common-sql
_convert_to_float_if_possible()
parse_boolean()
BaseSQLOperator()
BashOperator()
LegacyBashOperator()
BranchSQLOperator()
CheckOperator()
ConnectorProtocol()
DbApiHook()
DbApiHook2()
IntervalCheckOperator()
PrestoCheckOperator()
PrestoIntervalCheckOperator()
PrestoValueCheckOperator()
SQLCheckOperator()
SQLCheckOperator2()
SQLCheckOperator3()
SQLColumnCheckOperator2()
SQLIntervalCheckOperator()
SQLIntervalCheckOperator2()
SQLIntervalCheckOperator3()
SQLTableCheckOperator()
SQLThresholdCheckOperator()
SQLThresholdCheckOperator2()
SQLValueCheckOperator()
SQLValueCheckOperator2()
SQLValueCheckOperator3()
SqlSensor()
SqlSensor2()
ThresholdCheckOperator()
ValueCheckOperator()

# apache-airflow-providers-daskexecutor
DaskExecutor()

# apache-airflow-providers-docker
DockerHook()
DockerOperator()

# apache-airflow-providers-apache-druid
DruidDbApiHook()
DruidHook()
DruidCheckOperator()

# apache-airflow-providers-apache-hdfs
WebHDFSHook()
WebHdfsSensor()

# apache-airflow-providers-apache-hive
HIVE_QUEUE_PRIORITIES
closest_ds_partition()
max_partition()
HiveCliHook()
HiveMetastoreHook()
HiveOperator()
HivePartitionSensor()
HiveServer2Hook()
HiveStatsCollectionOperator()
HiveToDruidOperator()
HiveToDruidTransfer()
HiveToSambaOperator()
S3ToHiveOperator()
S3ToHiveTransfer()
MetastorePartitionSensor()
NamedHivePartitionSensor()

# apache-airflow-providers-http
HttpHook()
HttpSensor()
SimpleHttpOperator()

# apache-airflow-providers-jdbc
jaydebeapi
JdbcHook()
JdbcOperator()

# apache-airflow-providers-fab
basic_auth, kerberos_auth
auth_current_user
backend_kerberos_auth
fab_override
FabAuthManager()
FabAirflowSecurityManagerOverride()

# apache-airflow-providers-cncf-kubernetes
ALL_NAMESPACES
POD_EXECUTOR_DONE_KEY
_disable_verify_ssl()
_enable_tcp_keepalive()
append_to_pod()
annotations_for_logging_task_metadata()
annotations_to_key()
create_pod_id()
datetime_to_label_safe_datestring()
extend_object_field()
get_logs_task_metadata()
label_safe_datestring_to_datetime()
merge_objects()
Port()
Resources()
PodRuntimeInfoEnv()
PodGeneratorDeprecated()
Volume()
VolumeMount()
Secret()

add_pod_suffix()
add_pod_suffix2()
get_kube_client()
get_kube_client2()
make_safe_label_value()
make_safe_label_value2()
rand_str()
rand_str2()
K8SModel()
K8SModel2()
PodLauncher()
PodLauncher2()
PodStatus()
PodStatus2()
PodDefaults()
PodDefaults2()
PodDefaults3()
PodGenerator()
PodGenerator2()


# apache-airflow-providers-microsoft-mssql
MsSqlHook()
MsSqlOperator()
MsSqlToHiveOperator()
MsSqlToHiveTransfer()

# apache-airflow-providers-mysql
HiveToMySqlOperator()
HiveToMySqlTransfer()
MySqlHook()
MySqlOperator()
MySqlToHiveOperator()
MySqlToHiveTransfer()
PrestoToMySqlOperator()
PrestoToMySqlTransfer()

# apache-airflow-providers-oracle
OracleHook()
OracleOperator()

# apache-airflow-providers-papermill
PapermillOperator()

# apache-airflow-providers-apache-pig
PigCliHook()
PigOperator()

# apache-airflow-providers-postgres
Mapping
PostgresHook()
PostgresOperator()

# apache-airflow-providers-presto
PrestoHook()

# apache-airflow-providers-samba
SambaHook()

# apache-airflow-providers-slack
SlackHook()
SlackAPIOperator()
SlackAPIPostOperator()

# apache-airflow-providers-sqlite
SqliteHook()
SqliteOperator()

# apache-airflow-providers-zendesk
ZendeskHook()

# apache-airflow-providers-standard
FileSensor()
TriggerDagRunOperator()
ExternalTaskMarker(), ExternalTaskSensor()
BranchDateTimeOperator()
BranchDayOfWeekOperator()
DateTimeSensor()
TimeSensor()
TimeDeltaSensor()
DayOfWeekSensor()
FSHook()
PackageIndexHook()
SubprocessHook()
WorkflowTrigger()
FileTrigger()
DateTimeTrigger()
