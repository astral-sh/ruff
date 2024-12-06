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
from airflow.contrib.aws_athena_hook import AWSAthenaHook
from airflow.metrics.validators import AllowListValidator
from airflow.metrics.validators import BlockListValidator
from airflow.operators.subdag import SubDagOperator
from airflow.sensors.external_task import ExternalTaskSensorLink
from airflow.operators.bash_operator import BashOperator
from airflow.operators.branch_operator import BaseBranchOperator
from airflow.operators.dummy import EmptyOperator, DummyOperator
from airflow.operators import dummy_operator
from airflow.operators.email_operator import EmailOperator
from airflow.sensors.base_sensor_operator import BaseSensorOperator
from airflow.sensors.date_time_sensor import DateTimeSensor
from airflow.sensors.external_task_sensor import (
    ExternalTaskMarker,
    ExternalTaskSensor,
    ExternalTaskSensorLink,
)
from airflow.sensors.time_delta_sensor import TimeDeltaSensor
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
from airflow.utils.trigger_rule import TriggerRule
from airflow.www.auth import has_access
from airflow.www.utils import get_sensitive_variables_fields, should_hide_value_for_key


AWSAthenaHook
TaskStateTrigger

requires_access

AllowListValidator
BlockListValidator

SubDagOperator

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


ExternalTaskSensorLink
BashOperator
BaseBranchOperator
EmptyOperator, DummyOperator
dummy_operator.EmptyOperator
dummy_operator.DummyOperator
EmailOperator
BaseSensorOperator
DateTimeSensor
(ExternalTaskMarker, ExternalTaskSensor, ExternalTaskSensorLink)
TimeDeltaSensor

TemporaryDirectory
mkdirs

chain
cross_downstream

SHUTDOWN
terminating_states

TriggerRule.DUMMY
TriggerRule.NONE_FAILED_OR_SKIPPED

test_cycle

has_access
get_sensitive_variables_fields, should_hide_value_for_key
