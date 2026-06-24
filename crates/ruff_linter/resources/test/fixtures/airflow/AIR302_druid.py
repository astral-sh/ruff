from __future__ import annotations

from airflow.hooks.druid_hook import (
    DruidDbApiHook,
    DruidHook,
)
from airflow.operators.hive_to_druid import (
    HiveToDruidOperator,
    HiveToDruidTransfer,
)

DruidDbApiHook()
DruidHook()

HiveToDruidOperator()
HiveToDruidTransfer()
