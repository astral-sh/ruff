from __future__ import annotations

from airflow import (
    PY36,
    PY37,
    PY38,
    PY39,
    PY310,
    PY311,
    PY312,
)
from airflow.api_connexion.security import requires_access, requires_access_dataset
from airflow.auth.managers.base_auth_manager import is_authorized_dataset
from airflow.auth.managers.models.resource_details import DatasetDetails
from airflow.configuration import (
    as_dict,
    get,
    getboolean,
    getfloat,
    getint,
    has_option,
    remove_option,
    set,
)
from airflow.contrib.aws_athena_hook import AWSAthenaHook
from airflow.datasets import DatasetAliasEvent
from airflow.datasets.manager import (
    DatasetManager,
    dataset_manager,
    resolve_dataset_manager,
)
from airflow.hooks.base_hook import BaseHook
from airflow.lineage.hook import DatasetLineageInfo
from airflow.listeners.spec.dataset import on_dataset_changed, on_dataset_created
from airflow.metrics.validators import AllowListValidator, BlockListValidator
from airflow.operators.subdag import SubDagOperator
from airflow.providers.amazon.aws.auth_manager.avp.entities import AvpEntities
from airflow.providers.amazon.aws.datasets import s3
from airflow.providers.common.io.datasets import file as common_io_file
from airflow.providers.fab.auth_manager import fab_auth_manager
from airflow.providers.google.datasets import bigquery, gcs
from airflow.providers.mysql.datasets import mysql
from airflow.providers.openlineage.utils.utils import (
    DatasetInfo,
    translate_airflow_dataset,
)
from airflow.providers.postgres.datasets import postgres
from airflow.providers.trino.datasets import trino
from airflow.secrets.local_filesystem import LocalFilesystemBackend, load_connections
from airflow.security.permissions import RESOURCE_DATASET
from airflow.sensors.base_sensor_operator import BaseSensorOperator
from airflow.timetables.simple import DatasetTriggeredTimetable
from airflow.triggers.external_task import TaskStateTrigger
from airflow.utils import dates
from airflow.utils.dag_cycle_tester import test_cycle
from airflow.utils.dates import (
    date_range,
    datetime_to_nano,
    days_ago,
    infer_time_unit,
    parse_execution_date,
    round_time,
    scale_time_units,
)
from airflow.utils.db import create_session
from airflow.utils.decorators import apply_defaults
from airflow.utils.file import TemporaryDirectory, mkdirs
from airflow.utils.helpers import chain as helper_chain
from airflow.utils.helpers import cross_downstream as helper_cross_downstream
from airflow.utils.log import secrets_masker
from airflow.utils.state import SHUTDOWN, terminating_states
from airflow.utils.trigger_rule import TriggerRule
from airflow.www.auth import has_access, has_access_dataset
from airflow.www.utils import get_sensitive_variables_fields, should_hide_value_for_key

# airflow root
PY36, PY37, PY38, PY39, PY310, PY311, PY312
DatasetFromRoot()

# airflow.api_connexion.security
requires_access, requires_access_dataset

# airflow.auth.managers
is_authorized_dataset
DatasetDetails()

# airflow.configuration
get, getboolean, getfloat, getint, has_option, remove_option, as_dict, set


# airflow.contrib.*
AWSAthenaHook()


# airflow.datasets
DatasetAliasEvent()

# airflow.datasets.manager
DatasetManager()
dataset_manager
resolve_dataset_manager

# airflow.hooks
BaseHook()

# airflow.lineage.hook
DatasetLineageInfo()

# airflow.listeners.spec.dataset
on_dataset_changed
on_dataset_created

# airflow.metrics.validators
AllowListValidator()
BlockListValidator()


# airflow.operators.branch_operator
BaseBranchOperator()

# airflow.operators.dagrun_operator
TriggerDagRunLink()
TriggerDagRunOperator()

# airflow.operators.email_operator
EmailOperator()

# airflow.operators.latest_only_operator
LatestOnlyOperator()

# airflow.operators.python_operator
BranchPythonOperator()
PythonOperator()
PythonVirtualenvOperator()
ShortCircuitOperator()

# airflow.operators.subdag.*
SubDagOperator()

# airflow.providers.amazon
AvpEntities.DATASET
s3.create_dataset
s3.convert_dataset_to_openlineage
s3.sanitize_uri

# airflow.providers.common.io
common_io_file.convert_dataset_to_openlineage
common_io_file.create_dataset
common_io_file.sanitize_uri

# airflow.providers.fab
fab_auth_manager.is_authorized_dataset

# airflow.providers.google
bigquery.sanitize_uri

gcs.create_dataset
gcs.sanitize_uri
gcs.convert_dataset_to_openlineage

# airflow.providers.mysql
mysql.sanitize_uri

# airflow.providers.openlineage
DatasetInfo()
translate_airflow_dataset

# airflow.providers.postgres
postgres.sanitize_uri

# airflow.providers.trino
trino.sanitize_uri

# airflow.secrets
# get_connection
LocalFilesystemBackend()
load_connections

# airflow.security.permissions
RESOURCE_DATASET

# airflow.sensors.base_sensor_operator
BaseSensorOperator()


# airflow.timetables
DatasetTriggeredTimetable()

# airflow.triggers.external_task
TaskStateTrigger()

# airflow.utils.date
dates.date_range
dates.days_ago

date_range
days_ago
infer_time_unit
parse_execution_date
round_time
scale_time_units

# This one was not deprecated.
datetime_to_nano
dates.datetime_to_nano

# airflow.utils.dag_cycle_tester
test_cycle


# airflow.utils.db
create_session

# airflow.utils.decorators
apply_defaults

# airflow.utils.file
TemporaryDirectory()
mkdirs

#  airflow.utils.helpers
helper_chain
helper_cross_downstream

#  airflow.utils.log
secrets_masker

# airflow.utils.state
SHUTDOWN
terminating_states

#  airflow.utils.trigger_rule
TriggerRule.DUMMY
TriggerRule.NONE_FAILED_OR_SKIPPED

# airflow.www.auth
has_access
has_access_dataset

# airflow.www.utils
get_sensitive_variables_fields
should_hide_value_for_key
