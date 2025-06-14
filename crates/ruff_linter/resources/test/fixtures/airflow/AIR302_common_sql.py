from __future__ import annotations

from airflow.hooks.dbapi import (
    ConnectorProtocol,
    DbApiHook,
)

ConnectorProtocol()
DbApiHook()

from airflow.hooks.dbapi_hook import DbApiHook
from airflow.operators.check_operator import SQLCheckOperator

DbApiHook()
SQLCheckOperator()


from airflow.operators.check_operator import CheckOperator
from airflow.operators.sql import SQLCheckOperator

SQLCheckOperator()
CheckOperator()


from airflow.operators.druid_check_operator import CheckOperator

CheckOperator()


from airflow.operators.presto_check_operator import CheckOperator

CheckOperator()


from airflow.operators.check_operator import (
    IntervalCheckOperator,
    SQLIntervalCheckOperator,
)
from airflow.operators.druid_check_operator import DruidCheckOperator
from airflow.operators.presto_check_operator import PrestoCheckOperator

DruidCheckOperator()
PrestoCheckOperator()
IntervalCheckOperator()
SQLIntervalCheckOperator()


from airflow.operators.presto_check_operator import (
    IntervalCheckOperator,
    PrestoIntervalCheckOperator,
)
from airflow.operators.sql import SQLIntervalCheckOperator

IntervalCheckOperator()
SQLIntervalCheckOperator()
PrestoIntervalCheckOperator()


from airflow.operators.check_operator import (
    SQLThresholdCheckOperator,
    ThresholdCheckOperator,
)

SQLThresholdCheckOperator()
ThresholdCheckOperator()


from airflow.operators.sql import SQLThresholdCheckOperator

SQLThresholdCheckOperator()


from airflow.operators.check_operator import (
    SQLValueCheckOperator,
    ValueCheckOperator,
)

SQLValueCheckOperator()
ValueCheckOperator()


from airflow.operators.presto_check_operator import (
    PrestoValueCheckOperator,
    ValueCheckOperator,
)
from airflow.operators.sql import SQLValueCheckOperator

SQLValueCheckOperator()
ValueCheckOperator()
PrestoValueCheckOperator()


from airflow.operators.sql import (
    BaseSQLOperator,
    BranchSQLOperator,
    SQLColumnCheckOperator,
    SQLTableCheckOperator,
    _convert_to_float_if_possible,
    parse_boolean,
)

BaseSQLOperator()
BranchSQLOperator()
SQLTableCheckOperator()
SQLColumnCheckOperator()
_convert_to_float_if_possible()
parse_boolean()


from airflow.sensors.sql import SqlSensor

SqlSensor()


from airflow.sensors.sql_sensor import SqlSensor

SqlSensor()


from airflow.providers.common.sql.operators.sql import SQLExecuteQueryOperator

SQLExecuteQueryOperator()
SQLExecuteQueryOperator()
SQLExecuteQueryOperator()
SQLExecuteQueryOperator()
SQLExecuteQueryOperator()
SQLExecuteQueryOperator()
