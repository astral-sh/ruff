from __future__ import annotations

from airflow.hooks.filesystem import FSHook
from airflow.hooks.package_index import PackageIndexHook
from airflow.hooks.subprocess import SubprocessHook
from airflow.operators.bash import BashOperator
from airflow.operators.datetime import BranchDateTimeOperator
from airflow.operators.empty import EmptyOperator
from airflow.operators.latest_only import LatestOnlyOperator
from airflow.operators.trigger_dagrun import TriggerDagRunOperator
from airflow.operators.weekday import BranchDayOfWeekOperator

FSHook()
PackageIndexHook()
SubprocessHook()

BashOperator()
BranchDateTimeOperator()
TriggerDagRunOperator()
EmptyOperator()

LatestOnlyOperator()
BranchDayOfWeekOperator()

from airflow.operators.python import (
    BranchPythonOperator,
    PythonOperator,
    PythonVirtualenvOperator,
    ShortCircuitOperator,
)
from airflow.sensors.bash import BashSensor
from airflow.sensors.date_time import DateTimeSensor

BranchPythonOperator()
PythonOperator()
PythonVirtualenvOperator()
ShortCircuitOperator()

BashSensor()
DateTimeSensor()
from airflow.sensors.date_time import DateTimeSensorAsync
from airflow.sensors.external_task import (
    ExternalTaskMarker,
    ExternalTaskSensor,
)
from airflow.sensors.filesystem import FileSensor
from airflow.sensors.python import PythonSensor

BranchPythonOperator()
PythonOperator()
PythonVirtualenvOperator()
ShortCircuitOperator()
DateTimeSensorAsync()
ExternalTaskMarker()
ExternalTaskSensor()
FileSensor()
PythonSensor()

from airflow.sensors.time_sensor import (
    TimeSensor,
    TimeSensorAsync,
)

TimeSensor()
TimeSensorAsync()

from airflow.sensors.time_delta import (
    TimeDeltaSensor,
    TimeDeltaSensorAsync,
)
from airflow.sensors.weekday import DayOfWeekSensor
from airflow.triggers.external_task import (
    DagStateTrigger,
    WorkflowTrigger,
)
from airflow.triggers.file import FileTrigger
from airflow.triggers.temporal import (
    DateTimeTrigger,
    TimeDeltaTrigger,
)

TimeDeltaSensor()
TimeDeltaSensorAsync()
DayOfWeekSensor()
DagStateTrigger()
WorkflowTrigger()
FileTrigger()
DateTimeTrigger()
TimeDeltaTrigger()
