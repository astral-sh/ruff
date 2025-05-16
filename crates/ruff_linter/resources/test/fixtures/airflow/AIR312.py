from __future__ import annotations

from airflow.hooks.filesystem import FSHook
from airflow.hooks.package_index import PackageIndexHook
from airflow.hooks.subprocess import SubprocessHook
from airflow.operators.bash import BashOperator
from airflow.operators.datetime import BranchDateTimeOperator
from airflow.operators.empty import EmptyOperator
from airflow.operators.latest_only import LatestOnlyOperator
from airflow.operators.python import (
    BranchPythonOperator,
    PythonOperator,
    PythonVirtualenvOperator,
    ShortCircuitOperator,
)
from airflow.operators.trigger_dagrun import TriggerDagRunOperator
from airflow.operators.weekday import BranchDayOfWeekOperator
from airflow.sensors.date_time import DateTimeSensor, DateTimeSensorAsync
from airflow.sensors.external_task import (
    ExternalTaskMarker,
    ExternalTaskSensor,

)
from airflow.sensors.filesystem import FileSensor
from airflow.sensors.time_delta import TimeDeltaSensor, TimeDeltaSensorAsync
from airflow.sensors.time_sensor import TimeSensor, TimeSensorAsync
from airflow.sensors.weekday import DayOfWeekSensor
from airflow.triggers.external_task import DagStateTrigger, WorkflowTrigger
from airflow.triggers.file import FileTrigger
from airflow.triggers.temporal import DateTimeTrigger, TimeDeltaTrigger

FSHook()
PackageIndexHook()
SubprocessHook()
BashOperator()
BranchDateTimeOperator()
TriggerDagRunOperator()
EmptyOperator()
LatestOnlyOperator()
(
    BranchPythonOperator(),
    PythonOperator(),
    PythonVirtualenvOperator(),
    ShortCircuitOperator(),
)
BranchDayOfWeekOperator()
DateTimeSensor(), DateTimeSensorAsync()
ExternalTaskMarker(), ExternalTaskSensor()
FileSensor()
TimeSensor(), TimeSensorAsync()
TimeDeltaSensor(), TimeDeltaSensorAsync()
DayOfWeekSensor()
DagStateTrigger(), WorkflowTrigger()
FileTrigger()
DateTimeTrigger(), TimeDeltaTrigger()
