from __future__ import annotations

from airflow.api_connexion.security import requires_access_dataset
from airflow.auth.managers.models.resource_details import (
    DatasetDetails,

)
from airflow.datasets.manager import (
    DatasetManager,
    dataset_manager,
    resolve_dataset_manager,
)
from airflow.lineage.hook import DatasetLineageInfo
from airflow.metrics.validators import AllowListValidator, BlockListValidator
from airflow.secrets.local_filesystm import load_connections
from airflow.security.permissions import RESOURCE_DATASET
from airflow.www.auth import has_access_dataset

requires_access_dataset()

DatasetDetails()


DatasetManager()
dataset_manager()
resolve_dataset_manager()

DatasetLineageInfo()

AllowListValidator()
BlockListValidator()

load_connections()

RESOURCE_DATASET

has_access_dataset()

from airflow.listeners.spec.dataset import (
    on_dataset_changed,
    on_dataset_created,
)

on_dataset_created()
on_dataset_changed()
