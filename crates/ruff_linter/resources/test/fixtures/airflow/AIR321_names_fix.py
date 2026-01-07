from __future__ import annotations

# airflow.utils.setup_teardown -> airflow.sdk.definitions._internal.setup_teardown
from airflow.sdk.definitions._internal.setup_teardown import (
    BaseSetupTeardownContext,
    SetupTeardownContext,
)

BaseSetupTeardownContext()
SetupTeardownContext()

# airflow.utils.xcom -> airflow.models.xcom
from airflow.models.xcom import XCOM_RETURN_KEY

XCOM_RETURN_KEY

# airflow.utils.task_group -> airflow.sdk
from airflow.sdk import TaskGroup

TaskGroup()

# airflow.utils.timezone -> airflow.sdk.timezone
from datetime import datetime as dt
from airflow.sdk.timezone import (
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

# airflow.utils.decorators -> airflow.sdk.definitions._internal.decorators
from airflow.sdk.definitions._internal.decorators import (
    remove_task_decorator,
    fixup_decorator_warning_stack,
)

remove_task_decorator()
fixup_decorator_warning_stack()

# airflow.models.abstractoperator -> airflow.sdk.definitions._internal.abstractoperator
from airflow.sdk.definitions._internal.abstractoperator import (
    AbstractOperator,
    NotMapped,
    TaskStateChangeCallback,
)

AbstractOperator()
NotMapped()
TaskStateChangeCallback()

# airflow.models.baseoperator -> airflow.sdk
from airflow.sdk import BaseOperator

BaseOperator()

# airflow.macros -> airflow.sdk.execution_time.macros
from airflow.sdk.execution_time.macros import (
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

# airflow.io -> airflow.sdk.io
from airflow.sdk.io import (
    get_fs,
    has_fs,
    Properties,
)

get_fs()
has_fs()
Properties()
