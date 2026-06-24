from __future__ import annotations

try:
    from airflow.assets.manager import AssetManager
except ModuleNotFoundError:
    from airflow.datasets.manager import DatasetManager as AssetManager

AssetManager()
