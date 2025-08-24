from __future__ import annotations

from airflow import (
    Dataset as DatasetFromRoot,
)
from airflow.datasets import (
    Dataset,
    DatasetAlias,
    DatasetAny,
)
from airflow.datasets.manager import DatasetManager
from airflow.lineage.hook import DatasetLineageInfo, HookLineageCollector
from airflow.providers.amazon.aws.auth_manager.aws_auth_manager import AwsAuthManager
from airflow.providers.apache.beam.hooks import BeamHook, NotAir302HookError
from airflow.providers.google.cloud.secrets.secret_manager import (
    CloudSecretManagerBackend,
)
from airflow.providers.hashicorp.secrets.vault import NotAir302SecretError, VaultBackend
from airflow.providers_manager import ProvidersManager
from airflow.secrets.base_secrets import BaseSecretsBackend
from airflow.secrets.local_filesystem import LocalFilesystemBackend

# airflow.Dataset
dataset_from_root = DatasetFromRoot()
dataset_from_root.iter_datasets()
dataset_from_root.iter_dataset_aliases()

# airflow.datasets
dataset_to_test_method_call = Dataset()
dataset_to_test_method_call.iter_datasets()
dataset_to_test_method_call.iter_dataset_aliases()

alias_to_test_method_call = DatasetAlias()
alias_to_test_method_call.iter_datasets()
alias_to_test_method_call.iter_dataset_aliases()

any_to_test_method_call = DatasetAny()
any_to_test_method_call.iter_datasets()
any_to_test_method_call.iter_dataset_aliases()

# airflow.datasets.manager
dm = DatasetManager()
dm.register_dataset_change()
dm.create_datasets()
dm.notify_dataset_created()
dm.notify_dataset_changed()
dm.notify_dataset_alias_created()

# airflow.lineage.hook
dl_info = DatasetLineageInfo()
dl_info.dataset

hlc = HookLineageCollector()
hlc.create_dataset()
hlc.add_input_dataset()
hlc.add_output_dataset()
hlc.collected_datasets()

# airflow.providers.amazon.auth_manager.aws_auth_manager
aam = AwsAuthManager()
aam.is_authorized_dataset()

# airflow.providers.apache.beam.hooks
# check get_conn_uri is caught if the class inherits from an airflow hook
beam_hook = BeamHook()
beam_hook.get_conn_uri()

not_an_error = NotAir302HookError()
not_an_error.get_conn_uri()

# airflow.providers.google.cloud.secrets.secret_manager
csm_backend = CloudSecretManagerBackend()
csm_backend.get_conn_uri()
csm_backend.get_connections()

# airflow.providers.hashicorp.secrets.vault
vault_backend = VaultBackend()
vault_backend.get_conn_uri()
vault_backend.get_connections()

not_an_error = NotAir302SecretError()
not_an_error.get_conn_uri()

# airflow.providers_manager
pm = ProvidersManager()
pm.initialize_providers_dataset_uri_resources()
pm.dataset_factories
pm.dataset_uri_handlers
pm.dataset_to_openlineage_converters

# airflow.secrets.base_secrets
base_secret_backend = BaseSecretsBackend()
base_secret_backend.get_conn_uri()
base_secret_backend.get_connections()

# airflow.secrets.local_filesystem
lfb = LocalFilesystemBackend()
lfb.get_connections()
