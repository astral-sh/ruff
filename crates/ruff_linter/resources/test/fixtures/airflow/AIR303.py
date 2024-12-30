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
from airflow.hooks.base_hook import BaseHook
from airflow.hooks.dbapi import ConnectorProtocol, DbApiHook
from airflow.hooks.dbapi_hook import DbApiHook as DbApiHook2
from airflow.hooks.docker_hook import DockerHook
from airflow.hooks.druid_hook import DruidDbApiHook, DruidHook
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
from airflow.hooks.pig_hook import PigCliHook
from airflow.hooks.postgres_hook import PostgresHook
from airflow.hooks.presto_hook import PrestoHook
from airflow.hooks.S3_hook import S3Hook, provide_bucket_name
from airflow.hooks.samba_hook import SambaHook
from airflow.hooks.slack_hook import SlackHook
from airflow.hooks.sqlite_hook import SqliteHook
from airflow.hooks.webhdfs_hook import WebHDFSHook
from airflow.hooks.zendesk_hook import ZendeskHook
from airflow.macros.hive import closest_ds_partition, max_partition
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
from airflow.operators.latest_only_operator import LatestOnlyOperator
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
    SQLCheckOperator as SQLCheckOperator2,
    SQLIntervalCheckOperator as SQLIntervalCheckOperator2,
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
    SQLCheckOperator as SQLCheckOperator3,
    SQLColumnCheckOperator as SQLColumnCheckOperator2,
    SQLIntervalCheckOperator as SQLIntervalCheckOperator3,
    SQLTableCheckOperator,
    SQLThresholdCheckOperator as SQLThresholdCheckOperator2,
    SQLValueCheckOperator as SQLValueCheckOperator3,
    _convert_to_float_if_possible,
    parse_boolean,
)
from airflow.operators.sql_branch_operator import BranchSqlOperator
from airflow.operators.sqlite_operator import SqliteOperator
from airflow.www.security import FabAirflowSecurityManagerOverride

# apache-airflow-providers-fab
basic_auth, kerberos_auth
auth_current_user
backend_kerberos_auth
fab_override

FabAuthManager()
FabAirflowSecurityManagerOverride()

# apache-airflow-providers-celery
DEFAULT_CELERY_CONFIG
app
CeleryExecutor()
CeleryKubernetesExecutor()

# apache-airflow-providers-daskexecutor
DaskExecutor()

# apache-airflow-providers-common-sql
ConnectorProtocol()
DbApiHook()

# apache-airflow-providers-cncf-kubernetes
ALL_NAMESPACES
POD_EXECUTOR_DONE_KEY

# apache-airflow-providers-apache-hive
HIVE_QUEUE_PRIORITIES
closest_ds_partition()
max_partition()

# TODO: reorganize
S3Hook()
provide_bucket_name()
BaseHook()
DbApiHook2()
DockerHook()
DruidDbApiHook()
DruidHook()
HIVE_QUEUE_PRIORITIES()
HiveCliHook()
HiveMetastoreHook()
HiveServer2Hook()
HttpHook()
JdbcHook()
jaydebeapi()
MsSqlHook()
MySqlHook()
OracleHook()
PigCliHook()
PostgresHook()
PrestoHook()
SambaHook()
SlackHook()
SqliteHook()
WebHDFSHook()
ZendeskHook()

SQLCheckOperator()
SQLIntervalCheckOperator()
SQLThresholdCheckOperator()
SQLValueCheckOperator()
CheckOperator()
IntervalCheckOperator()
ThresholdCheckOperator()
ValueCheckOperator()
DockerOperator()
DruidCheckOperator()
GCSToS3Operator()
GoogleApiToS3Operator()
GoogleApiToS3Transfer()
HiveOperator()
HiveStatsCollectionOperator()
HiveToDruidOperator()
HiveToDruidTransfer()
HiveToMySqlOperator()
HiveToMySqlTransfer()
HiveToSambaOperator()
SimpleHttpOperator()
JdbcOperator()
LatestOnlyOperator()
MsSqlOperator()
MsSqlToHiveOperator()
MsSqlToHiveTransfer()
MySqlOperator()
MySqlToHiveOperator()
MySqlToHiveTransfer()
OracleOperator()
PapermillOperator()
PigOperator()
Mapping()
PostgresOperator()
SQLCheckOperator2()
SQLIntervalCheckOperator2()
SQLValueCheckOperator2()
PrestoCheckOperator()
PrestoIntervalCheckOperator()
PrestoValueCheckOperator()
PrestoToMySqlOperator()
PrestoToMySqlTransfer()
RedshiftToS3Operator()
RedshiftToS3Transfer()
S3FileTransformOperator()
S3ToHiveOperator()
S3ToHiveTransfer()
S3ToRedshiftOperator()
S3ToRedshiftTransfer()
SlackAPIOperator()
SlackAPIPostOperator()
BaseSQLOperator()
BranchSQLOperator()
SQLCheckOperator3()
SQLColumnCheckOperator2()
SQLIntervalCheckOperator3()
SQLTableCheckOperator()
SQLThresholdCheckOperator2()
SQLValueCheckOperator3()
_convert_to_float_if_possible()
parse_boolean()
BranchSQLOperator()
BranchSqlOperator()
SqliteOperator()
