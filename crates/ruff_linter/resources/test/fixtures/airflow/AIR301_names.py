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
from airflow.api_connexion.security import requires_access
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
from airflow.hooks.base_hook import BaseHook
from airflow.operators.subdag import SubDagOperator
from airflow.secrets.local_filesystem import LocalFilesystemBackend
from airflow.sensors.base_sensor_operator import BaseSensorOperator
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
from airflow.www.auth import has_access
from airflow.www.utils import get_sensitive_variables_fields, should_hide_value_for_key

# airflow root
PY36, PY37, PY38, PY39, PY310, PY311, PY312

# airflow.api_connexion.security
requires_access


# airflow.configuration
get, getboolean, getfloat, getint, has_option, remove_option, as_dict, set


# airflow.contrib.*
AWSAthenaHook()


# airflow.datasets
DatasetAliasEvent()


# airflow.hooks
BaseHook()


# airflow.operators.subdag.*
SubDagOperator()


# airflow.secrets
# get_connection
LocalFilesystemBackend()


# airflow.sensors.base_sensor_operator
BaseSensorOperator()


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

# airflow.www.utils
get_sensitive_variables_fields
should_hide_value_for_key

# airflow.operators.python
from airflow.operators.python import get_current_context

get_current_context()

# airflow.providers.mysql
from airflow.providers.mysql.datasets.mysql import sanitize_uri

sanitize_uri

# airflow.providers.postgres
from airflow.providers.postgres.datasets.postgres import sanitize_uri

sanitize_uri

# airflow.providers.trino
from airflow.providers.trino.datasets.trino import sanitize_uri

sanitize_uri

# airflow.notifications.basenotifier
from airflow.notifications.basenotifier import BaseNotifier

BaseNotifier()

# airflow.auth.manager
from airflow.auth.managers.base_auth_manager import BaseAuthManager

BaseAuthManager()
