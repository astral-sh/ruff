from __future__ import annotations

from airflow.providers.amazon.aws.auth_manager.avp.entities import AvpEntities
from airflow.providers.openlineage.utils.utils import (
    DatasetInfo,
    translate_airflow_dataset,
)
from airflow.secrets.local_filesystem import load_connections
from airflow.security.permissions import RESOURCE_DATASET

AvpEntities.DATASET

# airflow.providers.openlineage.utils.utils
DatasetInfo()
translate_airflow_dataset()

# airflow.secrets.local_filesystem
load_connections()

# airflow.security.permissions
RESOURCE_DATASET

from airflow.providers.amazon.aws.datasets.s3 import (
    convert_dataset_to_openlineage as s3_convert_dataset_to_openlineage,
)
from airflow.providers.amazon.aws.datasets.s3 import create_dataset as s3_create_dataset

s3_create_dataset()
s3_convert_dataset_to_openlineage()

from airflow.providers.common.io.dataset.file import (
    convert_dataset_to_openlineage as io_convert_dataset_to_openlineage,
)
from airflow.providers.common.io.dataset.file import create_dataset as io_create_dataset

io_create_dataset()
io_convert_dataset_to_openlineage()


# # airflow.providers.google.datasets.bigquery
from airflow.providers.google.datasets.bigquery import (
    create_dataset as bigquery_create_dataset,
)

bigquery_create_dataset()

# airflow.providers.google.datasets.gcs
from airflow.providers.google.datasets.gcs import (
    convert_dataset_to_openlineage as gcs_convert_dataset_to_openlineage,
)
from airflow.providers.google.datasets.gcs import create_dataset as gcs_create_dataset

gcs_create_dataset()
gcs_convert_dataset_to_openlineage()
