from __future__ import annotations

from airflow.operators.bash_operator import BashOperator
from airflow.operators.dagrun_operator import (
    TriggerDagRunLink,
    TriggerDagRunOperator,
)
from airflow.operators.dummy import (
    DummyOperator,
    EmptyOperator,
)
from airflow.operators.latest_only_operator import LatestOnlyOperator
from airflow.operators.python_operator import (
    BranchPythonOperator,
    PythonOperator,
    PythonVirtualenvOperator,
    ShortCircuitOperator,
)
from airflow.sensors.external_task_sensor import (
    ExternalTaskMarker,
    ExternalTaskSensor,
    ExternalTaskSensorLink,
)

BashOperator()

TriggerDagRunLink()
TriggerDagRunOperator()
DummyOperator()
EmptyOperator()

LatestOnlyOperator()

BranchPythonOperator()
PythonOperator()
PythonVirtualenvOperator()
ShortCircuitOperator()

ExternalTaskMarker()
ExternalTaskSensor()
ExternalTaskSensorLink()

from airflow.operators.dummy_operator import (
    DummyOperator,
    EmptyOperator,
)

DummyOperator()
EmptyOperator()
