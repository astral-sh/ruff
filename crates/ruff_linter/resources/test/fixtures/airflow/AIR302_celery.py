from __future__ import annotations

from airflow.config_templates.default_celery import DEFAULT_CELERY_CONFIG
from airflow.executors.celery_executor import (
    CeleryExecutor,
    app,
)

DEFAULT_CELERY_CONFIG

app
CeleryExecutor()
