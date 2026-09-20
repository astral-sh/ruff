from __future__ import annotations

try:
    from airflow.sdk.exceptions import AirflowSkipException
except ModuleNotFoundError:
    from airflow.exceptions import AirflowSkipException

AirflowSkipException()
