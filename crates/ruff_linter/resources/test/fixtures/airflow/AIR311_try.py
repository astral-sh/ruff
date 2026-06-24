from __future__ import annotations

try:
    from airflow.sdk import Asset
except ModuleNotFoundError:
    from airflow.datasets import Dataset as Asset

Asset()
