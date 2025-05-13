from __future__ import annotations

try:
    from airflow.providers.http.operators.http import HttpOperator
except ModuleNotFoundError:
    from airflow.operators.http_operator import SimpleHttpOperator as HttpOperator

HttpOperator()
