from __future__ import annotations

from airflow.providers.amazon.aws.auth_manager.avp.entities import AvpEntities
from airflow.providers.amazon.aws.datasets.s3 import (
    convert_dataset_to_openlineage as s3_convert_dataset_to_openlineage,
)
from airflow.providers.amazon.aws.datasets.s3 import create_dataset as s3_create_dataset
from airflow.providers.common.io.dataset.file import (
    convert_dataset_to_openlineage as io_convert_dataset_to_openlineage,
)
from airflow.providers.common.io.dataset.file import create_dataset as io_create_dataset

from airflow.providers.google.datasets.bigquery import (
    create_dataset as bigquery_create_dataset,
)
from airflow.providers.google.datasets.gcs import (
    convert_dataset_to_openlineage as gcs_convert_dataset_to_openlineage,
)
from airflow.providers.google.datasets.gcs import create_dataset as gcs_create_dataset
from airflow.providers.openlineage.utils.utils import (
    DatasetInfo,
    translate_airflow_dataset,
)

AvpEntities.DATASET

s3_create_dataset()
s3_convert_dataset_to_openlineage()

io_create_dataset()
io_convert_dataset_to_openlineage()



# airflow.providers.google.datasets.bigquery
bigquery_create_dataset()
# airflow.providers.google.datasets.gcs
gcs_create_dataset()
gcs_convert_dataset_to_openlineage()
# airflow.providers.openlineage.utils.utils
DatasetInfo()
translate_airflow_dataset()
#
# airflow.secrets.local_filesystem
load_connections()
#
# airflow.security.permissions
RESOURCE_DATASET

# airflow.timetables
DatasetTriggeredTimetable()
#
# airflow.www.auth
has_access_dataset
