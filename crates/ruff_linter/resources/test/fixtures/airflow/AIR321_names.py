from __future__ import annotations

# airflow.utils.setup_teardown
from airflow.utils.setup_teardown import (
    BaseSetupTeardownContext,
    SetupTeardownContext,
)

BaseSetupTeardownContext()
SetupTeardownContext()

# airflow.utils.xcom
from airflow.utils.xcom import XCOM_RETURN_KEY

XCOM_RETURN_KEY

# airflow.utils.task_group
from airflow.utils.task_group import TaskGroup

TaskGroup()

# airflow.utils.timezone
from datetime import datetime as dt
from airflow.utils.timezone import (
    coerce_datetime,
    convert_to_utc,
    datetime,
    make_naive,
    parse,
    utc,
    utcnow,
)

current_time = dt.now()

coerce_datetime(v=current_time, tz=None)
convert_to_utc(current_time)
datetime(2026, 1, 1, 21, 30, 0)
make_naive(current_time, tz=None)
parse("2026-01-01 21:30:00")
utc
utcnow()

# airflow.utils.decorators
from airflow.utils.decorators import (
    remove_task_decorator,
    fixup_decorator_warning_stack,
)

remove_task_decorator()
fixup_decorator_warning_stack()

# airflow.models.abstractoperator
from airflow.models.abstractoperator import (
    AbstractOperator,
    NotMapped,
    TaskStateChangeCallback,
)

AbstractOperator()
NotMapped()
TaskStateChangeCallback()

# airflow.models.baseoperator
from airflow.models.baseoperator import BaseOperator

BaseOperator()

# airflow.macros
from airflow.macros import (
    ds_add,
    ds_format,
    datetime_diff_for_humans,
    ds_format_locale,
)

ds_add("2026-01-01", 5)
ds_format("2026-01-01", "%Y-%m-%d", "%m-%d-%y")
datetime_diff_for_humans(
    dt(2026, 1, 1, 21, 30, 0),
    since=dt(2026, 1, 2, 21, 30, 0)
)
ds_format_locale("01/01/2026", "%d/%m/%Y", "EEEE dd MMMM yyyy", "en_US")

# airflow.io
from airflow.io import (
    get_fs,
    has_fs,
    Properties,
)

get_fs()
has_fs()
Properties()

# airflow.secrets.cache
from airflow.secrets.cache import SecretCache
SecretCache()

# airflow.hooks
from airflow.hooks.base import BaseHook
BaseHook()
