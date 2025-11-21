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
from airflow.contrib.aws_athena_hook import AWSAthenaHook
from airflow.datasets import DatasetAliasEvent, DatasetEvent
from airflow.operators.postgres_operator import Mapping
from airflow.operators.subdag import SubDagOperator
from airflow.secrets.cache import SecretCache
from airflow.secrets.local_filesystem import LocalFilesystemBackend
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
from airflow.utils.file import mkdirs
from airflow.utils.state import SHUTDOWN, terminating_states
from airflow.utils.trigger_rule import TriggerRule
from airflow.www.auth import has_access, has_access_dataset
from airflow.www.utils import get_sensitive_variables_fields, should_hide_value_for_key

# airflow root
PY36, PY37, PY38, PY39, PY310, PY311, PY312

# airflow.api_connexion.security
requires_access

# airflow.contrib.*
AWSAthenaHook()


# airflow.datasets
DatasetAliasEvent()
DatasetEvent()


# airflow.operators.subdag.*
SubDagOperator()

# airflow.operators.postgres_operator
Mapping()

# airflow.secrets
# get_connection
LocalFilesystemBackend()

# airflow.secrets.cache
SecretCache()


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
mkdirs


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
