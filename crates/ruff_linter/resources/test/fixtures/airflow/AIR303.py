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
