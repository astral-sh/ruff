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
from airflow.secrets.local_filesystem import load_connections
from airflow.security.permissions import RESOURCE_DATASET

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


from airflow.listeners.spec.dataset import (
    on_dataset_changed,
    on_dataset_created,
)

on_dataset_created()
on_dataset_changed()


# airflow.operators.python
from airflow.operators.python import get_current_context

get_current_context()

# airflow.providers.mysql
from airflow.providers.mysql.datasets.mysql import sanitize_uri

sanitize_uri

# airflow.providers.postgres
from airflow.providers.postgres.datasets.postgres import sanitize_uri

sanitize_uri

# airflow.providers.trino
from airflow.providers.trino.datasets.trino import sanitize_uri

sanitize_uri

# airflow.notifications.basenotifier
from airflow.notifications.basenotifier import BaseNotifier

BaseNotifier()

# airflow.auth.manager
from airflow.auth.managers.base_auth_manager import BaseAuthManager

BaseAuthManager()


from airflow.configuration import (
    as_dict,
    get,
    getboolean,
    getfloat,
    getint,
    has_option,
    remove_option,
    set,
)

# airflow.configuration
get, getboolean, getfloat, getint, has_option, remove_option, as_dict, set
from airflow.hooks.base_hook import BaseHook

# airflow.hooks
BaseHook()

from airflow.sensors.base_sensor_operator import BaseSensorOperator

# airflow.sensors.base_sensor_operator
BaseSensorOperator()
BaseHook()

from airflow.utils.helpers import chain as helper_chain
from airflow.utils.helpers import cross_downstream as helper_cross_downstream

#  airflow.utils.helpers
helper_chain
helper_cross_downstream

# airflow.utils.file
from airflow.utils.file import TemporaryDirectory

TemporaryDirectory()

from airflow.utils.log import secrets_masker

#  airflow.utils.log
secrets_masker

# airflow.utils.setup_teardown
from airflow.utils.setup_teardown import BaseSetupTeardownContext, SetupTeardownContext

BaseSetupTeardownContext
SetupTeardownContext

# airflow.utils.xcom
from airflow.utils.xcom import XCOM_RETURN_KEY

XCOM_RETURN_KEY

# airflow.utils.task_group
from airflow.utils.task_group import TaskGroup

TaskGroup

# airflow.utils.timeout
from airflow.utils.timeout import timeout

timeout

# airflow.utils.weight_rule
from airflow.utils.weight_rule import (
    DB_SAFE_MAXIMUM,
    DB_SAFE_MINIMUM,
    WeightRule,
    db_safe_priority,
)

WeightRule
DB_SAFE_MINIMUM
DB_SAFE_MAXIMUM
db_safe_priority

# airflow.utils.decorators (additional)
from airflow.utils.decorators import fixup_decorator_warning_stack, remove_task_decorator

fixup_decorator_warning_stack
remove_task_decorator
