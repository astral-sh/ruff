from __future__ import annotations

from airflow.auth.managers.base_auth_manager import is_authorized_dataset
from airflow.datasets.manager import (
    DatasetManager,
    dataset_manager,
    resolve_dataset_manager,
)
from airflow.lineage.hook import DatasetLineageInfo
from airflow.listeners.spec.dataset import (
    on_dataset_changed,
    on_dataset_created,
)
from airflow.metrics.validators import (
    AllowListValidator,
    BlockListValidator,
)
from airflow.secrets.local_filesystem import load_connections
from airflow.security.permissions import RESOURCE_DATASET
from airflow.timetables.simple import DatasetTriggeredTimetable
from airflow.www.auth import has_access_dataset

# airflow.datasets.manager
DatasetManager()
dataset_manager
resolve_dataset_manager

# airflow.auth.managers
is_authorized_dataset
DatasetDetails()

# airflow.lineage.hook
DatasetLineageInfo()

# airflow.listeners.spec.dataset
on_dataset_changed
on_dataset_created
# airflow.metrics.validators
AllowListValidator()
BlockListValidator()

# airflow.secrets.local_filesystem
load_connections()

# airflow.security.permissions
RESOURCE_DATASET
# airflow.timetables
DatasetTriggeredTimetable()

# airflow.www.auth
has_access_dataset
