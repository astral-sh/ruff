from __future__ import annotations

from airflow.hooks.hive_hooks import (
    HIVE_QUEUE_PRIORITIES,
    HiveCliHook,
    HiveMetastoreHook,
    HiveServer2Hook,
)
from airflow.macros.hive import (
    closest_ds_partition,
    max_partition,
)
from airflow.operators.hive_operator import HiveOperator
from airflow.operators.hive_stats_operator import HiveStatsCollectionOperator
from airflow.operators.hive_to_mysql import (
    HiveToMySqlOperator,
    HiveToMySqlTransfer,
)
from airflow.operators.hive_to_samba_operator import HiveToSambaOperator
from airflow.operators.mssql_to_hive import (
    MsSqlToHiveOperator,
    MsSqlToHiveTransfer,
)
from airflow.operators.mysql_to_hive import (
    MySqlToHiveOperator,
    MySqlToHiveTransfer,
)
from airflow.operators.s3_to_hive_operator import (
    S3ToHiveOperator,
    S3ToHiveTransfer,
)
from airflow.sensors.hive_partition_sensor import HivePartitionSensor
from airflow.sensors.metastore_partition_sensor import MetastorePartitionSensor
from airflow.sensors.named_hive_partition_sensor import NamedHivePartitionSensor

closest_ds_partition()
max_partition()

HiveCliHook()
HiveMetastoreHook()
HiveServer2Hook()
HIVE_QUEUE_PRIORITIES

HiveOperator()

HiveStatsCollectionOperator()

HiveToMySqlOperator()
HiveToMySqlTransfer()

HiveToSambaOperator()

MsSqlToHiveOperator()
MsSqlToHiveTransfer()

MySqlToHiveOperator()
MySqlToHiveTransfer()

S3ToHiveOperator()
S3ToHiveTransfer()

HivePartitionSensor()

MetastorePartitionSensor()

NamedHivePartitionSensor()
