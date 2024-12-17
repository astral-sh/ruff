from airflow import PY36, PY37, PY38, PY39, PY310, PY311, PY312
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
from airflow.metrics.validators import AllowListValidator, BlockListValidator
from airflow.operators import dummy_operator
from airflow.operators.bash_operator import BashOperator
from airflow.operators.branch_operator import BaseBranchOperator
from airflow.operators.dummy import DummyOperator, EmptyOperator
from airflow.operators.email_operator import EmailOperator
from airflow.operators.subdag import SubDagOperator
from airflow.secrets.local_filesystem import get_connection, load_connections
from airflow.sensors.base_sensor_operator import BaseSensorOperator
from airflow.sensors.date_time_sensor import DateTimeSensor
from airflow.sensors.external_task import (
    ExternalTaskSensorLink as ExternalTaskSensorLinkFromExternalTask,
)
from airflow.sensors.external_task_sensor import (
    ExternalTaskMarker,
    ExternalTaskSensor,
    ExternalTaskSensorLink as ExternalTaskSensorLinkFromExternalTaskSensor,
)
from airflow.sensors.time_delta_sensor import TimeDeltaSensor
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
from airflow.utils.decorators import apply_defaults
from airflow.utils.file import TemporaryDirectory, mkdirs
from airflow.utils.helpers import chain, cross_downstream
from airflow.utils.state import SHUTDOWN, terminating_states
from airflow.utils.trigger_rule import TriggerRule
from airflow.www.auth import has_access
from airflow.www.utils import get_sensitive_variables_fields, should_hide_value_for_key

# airflow.PY\d{2}
PY36, PY37, PY38, PY39, PY310, PY311, PY312

# airflow.api_connexion.security
requires_access

# airflow.configuration
get, getboolean, getfloat, getint, has_option, remove_option, as_dict, set


# airflow.contrib.*
AWSAthenaHook

# airflow.metrics.validators
AllowListValidator, BlockListValidator

# airflow.operators.dummy_operator
dummy_operator.EmptyOperator
dummy_operator.DummyOperator

# airflow.operators.bash_operator
BashOperator

# airflow.operators.branch_operator
BaseBranchOperator

# airflow.operators.dummy
EmptyOperator, DummyOperator

# airflow.operators.email_operator
EmailOperator

# airflow.operators.subdag.*
SubDagOperator

# airflow.secrets
get_connection, load_connections

# airflow.sensors.base_sensor_operator
BaseSensorOperator

# airflow.sensors.date_time_sensor
DateTimeSensor

# airflow.sensors.external_task
ExternalTaskSensorLinkFromExternalTask

# airflow.sensors.external_task_sensor
ExternalTaskMarker
ExternalTaskSensor
ExternalTaskSensorLinkFromExternalTaskSensor

# airflow.sensors.time_delta_sensor
TimeDeltaSensor

# airflow.triggers.external_task
TaskStateTrigger

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

# airflow.utils.decorators
apply_defaults

# airflow.utils.file
TemporaryDirectory, mkdirs

#  airflow.utils.helpers
chain, cross_downstream

# airflow.utils.state
SHUTDOWN, terminating_states

#  airflow.utils.trigger_rule
TriggerRule.DUMMY
TriggerRule.NONE_FAILED_OR_SKIPPED

# airflow.www.auth
has_access

# airflow.www.utils
get_sensitive_variables_fields, should_hide_value_for_key
