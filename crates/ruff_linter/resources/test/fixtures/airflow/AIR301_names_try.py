from __future__ import annotations

try:
    from airflow.sdk import Asset
except ModuleNotFoundError:
    from airflow.datasets import Dataset as Asset

Asset

try:
    from airflow.sdk import Asset
except ModuleNotFoundError:
    from airflow import datasets

    Asset = datasets.Dataset

asset = Asset()
