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
from airflow.hooks.hive_hooks import HIVE_QUEUE_PRIORITIES
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
