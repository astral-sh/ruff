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
from airflow.operators.hive_to_mysql import HiveToMySqlOperator
from airflow.operators.hive_to_samba_operator import HiveToSambaOperator

HIVE_QUEUE_PRIORITIES
HiveCliHook()
HiveMetastoreHook()
HiveServer2Hook()

closest_ds_partition()
max_partition()

HiveOperator()
HiveStatsCollectionOperator()
HiveToMySqlOperator()
HiveToSambaOperator()


from airflow.operators.hive_to_mysql import HiveToMySqlTransfer

HiveToMySqlTransfer()

from airflow.operators.mysql_to_hive import MySqlToHiveOperator

MySqlToHiveOperator()

from airflow.operators.mysql_to_hive import MySqlToHiveTransfer

MySqlToHiveTransfer()

from airflow.operators.mssql_to_hive import MsSqlToHiveOperator

MsSqlToHiveOperator()

from airflow.operators.mssql_to_hive import MsSqlToHiveTransfer

MsSqlToHiveTransfer()

from airflow.operators.s3_to_hive_operator import S3ToHiveOperator

S3ToHiveOperator()

from airflow.operators.s3_to_hive_operator import S3ToHiveTransfer

S3ToHiveTransfer()

from airflow.sensors.hive_partition_sensor import HivePartitionSensor

HivePartitionSensor()

from airflow.sensors.metastore_partition_sensor import MetastorePartitionSensor

MetastorePartitionSensor()

from airflow.sensors.named_hive_partition_sensor import NamedHivePartitionSensor

NamedHivePartitionSensor()
