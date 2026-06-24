from __future__ import annotations

from airflow.hooks.mysql_hook import MySqlHook
from airflow.operators.presto_to_mysql import (
    PrestoToMySqlOperator,
    PrestoToMySqlTransfer,
)

MySqlHook()
PrestoToMySqlOperator()
PrestoToMySqlTransfer()
