from airflow.triggers.external_task import TaskStateTrigger
from airflow.api_connexion.security import requires_access
from airflow.configuration import (
    get,
    getboolean,
    getfloat,
    getint,
    has_option,
    remove_option,
    as_dict,
    set,
)
from airflow.metrics.validators import AllowListValidator
from airflow.metrics.validators import BlockListValidator
from airflow.secrets.local_filesystem import get_connection, load_connections
from airflow.utils import dates
from airflow.utils.dates import (
    date_range,
    datetime_to_nano,
    days_ago,
    infer_time_unit,
    parse_execution_date,
    round_time,
    scale_time_units,
)
from airflow.utils.file import TemporaryDirectory, mkdirs
from airflow.utils.helpers import chain, cross_downstream
from airflow.utils.state import SHUTDOWN, terminating_states
from airflow.utils.dag_cycle_tester import test_cycle
from airflow.www.auth import has_access
from airflow.www.utils import get_sensitive_variables_fields, should_hide_value_for_key


TaskStateTrigger

requires_access

AllowListValidator
BlockListValidator

dates.date_range
dates.days_ago

date_range
days_ago
parse_execution_date
round_time
scale_time_units
infer_time_unit


# This one was not deprecated.
datetime_to_nano
dates.datetime_to_nano

get, getboolean, getfloat, getint, has_option, remove_option, as_dict, set

get_connection, load_connections

TemporaryDirectory
mkdirs

chain
cross_downstream

SHUTDOWN
terminating_states

test_cycle

has_access
get_sensitive_variables_fields, should_hide_value_for_key
