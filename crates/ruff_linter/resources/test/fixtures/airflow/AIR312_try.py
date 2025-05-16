from __future__ import annotations

try:
    from airflow.providers.standard.triggers.file import FileTrigger
except ModuleNotFoundError:
    from airflow.triggers.file import FileTrigger

FileTrigger()
