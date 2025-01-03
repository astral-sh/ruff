from airflow.datasets.manager import DatasetManager
from airflow.lineage.hook import DatasetLineageInfo, HookLineageCollector
from airflow.providers.amazon.auth_manager.aws_auth_manager import AwsAuthManager
from airflow.providers.apache.beam.hooks import BeamHook, NotAir302HookError
from airflow.providers.google.cloud.secrets.secret_manager import (
    CloudSecretManagerBackend,
)
from airflow.providers.hashicorp.secrets.vault import NotAir302SecretError, VaultBackend
from airflow.providers_manager import ProvidersManager
from airflow.secrets.base_secrets import BaseSecretsBackend

dm = DatasetManager()
dm.register_dataset_change()
dm.create_datasets()
dm.notify_dataset_created()
dm.notify_dataset_changed()
dm.notify_dataset_alias_created()

hlc = HookLineageCollector()
hlc.create_dataset()
hlc.add_input_dataset()
hlc.add_output_dataset()
hlc.collected_datasets()

aam = AwsAuthManager()
aam.is_authorized_dataset()

pm = ProvidersManager()
pm.initialize_providers_asset_uri_resources()
pm.dataset_factories

base_secret_backend = BaseSecretsBackend()
base_secret_backend.get_conn_uri()
base_secret_backend.get_connections()

csm_backend = CloudSecretManagerBackend()
csm_backend.get_conn_uri()
csm_backend.get_connections()

vault_backend = VaultBackend()
vault_backend.get_conn_uri()
vault_backend.get_connections()

not_an_error = NotAir302SecretError()
not_an_error.get_conn_uri()

beam_hook = BeamHook()
beam_hook.get_conn_uri()

not_an_error = NotAir302HookError()
not_an_error.get_conn_uri()

provider_manager = ProvidersManager()
provider_manager.dataset_factories
provider_manager.dataset_uri_handlers
provider_manager.dataset_to_openlineage_converters

dl_info = DatasetLineageInfo()
dl_info.dataset
