from __future__ import annotations

# airflow.exceptions (imported by name)
from airflow.exceptions import (
    AirflowDagCycleException,
    AirflowFailException,
    AirflowInactiveAssetInInletOrOutletException,
    AirflowSensorTimeout,
    AirflowSkipException,
    AirflowTaskTerminated,
    AirflowTaskTimeout,
    DagRunTriggerException,
)

AirflowDagCycleException()
AirflowFailException()
AirflowInactiveAssetInInletOrOutletException()
AirflowSensorTimeout()
AirflowSkipException()
AirflowTaskTerminated()
AirflowTaskTimeout()
DagRunTriggerException()

# Accessed through the module
from airflow import exceptions

exceptions.AirflowSkipException()
exceptions.TaskDeferred()

# Not moved: still importable from `airflow.exceptions` without a deprecation warning
from airflow.exceptions import AirflowException, AirflowNotFoundException

AirflowException()
AirflowNotFoundException()

# Already imported from the new location
from airflow.sdk.exceptions import AirflowSkipException as SdkAirflowSkipException

SdkAirflowSkipException()
