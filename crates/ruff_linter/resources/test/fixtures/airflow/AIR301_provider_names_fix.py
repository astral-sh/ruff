from __future__ import annotations

from airflow.providers.amazon.aws.auth_manager.avp.entities import AvpEntities
from airflow.providers.openlineage.utils.utils import (
    AssetInfo,
translate_airflow_asset,
)
from airflow.secrets.local_filesystem import load_connections_dict
from airflow.security.permissions import RESOURCE_ASSET

AvpEntities

# airflow.providers.openlineage.utils.utils
AssetInfo()
translate_airflow_asset()

# airflow.secrets.local_filesystem
load_connections_dict()

# airflow.security.permissions
RESOURCE_ASSET

from airflow.providers.amazon.aws.assets.s3 import create_asset, convert_asset_to_openlineage

create_asset()
convert_asset_to_openlineage()

from airflow.providers.common.io.assets.file import create_asset, convert_asset_to_openlineage

create_asset()
convert_asset_to_openlineage()


# # airflow.providers.google.datasets.bigquery
from airflow.providers.google.assets.bigquery import create_asset

create_asset()

# airflow.providers.google.datasets.gcs
from airflow.providers.google.assets.gcs import create_asset, convert_asset_to_openlineage

create_asset()
convert_asset_to_openlineage()
