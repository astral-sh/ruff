from __future__ import annotations

from datetime import timedelta

from airflow import DAG, dag
from airflow.operators.datetime import BranchDateTimeOperator
from airflow.operators.trigger_dagrun import TriggerDagRunOperator
from airflow.operators.weekday import BranchDayOfWeekOperator
from airflow.providers.amazon.aws.log.s3_task_handler import S3TaskHandler
from airflow.providers.apache.hdfs.log.hdfs_task_handler import HdfsTaskHandler
from airflow.providers.elasticsearch.log.es_task_handler import ElasticsearchTaskHandler
from airflow.providers.fab.auth_manager.fab_auth_manager import FabAuthManager
from airflow.providers.google.cloud.log.gcs_task_handler import GCSTaskHandler
from airflow.providers.standard.operators import datetime, trigger_dagrun
from airflow.providers.standard.sensors import weekday
from airflow.sensors.weekday import DayOfWeekSensor
from airflow.timetables.simple import NullTimetable

DAG(dag_id="class_schedule", schedule="@hourly")

DAG(dag_id="class_schedule_interval", schedule_interval="@hourly")

DAG(dag_id="class_timetable", timetable=NullTimetable())


DAG(dag_id="class_fail_stop", fail_stop=True)

DAG(dag_id="class_default_view", default_view="dag_default_view")

DAG(dag_id="class_orientation", orientation="BT")

allow_future_exec_dates_dag = DAG(dag_id="class_allow_future_exec_dates")
allow_future_exec_dates_dag.allow_future_exec_dates


@dag(schedule="0 * * * *")
def decorator_schedule():
    pass


@dag(schedule_interval="0 * * * *")
def decorator_schedule_interval():
    pass


@dag(timetable=NullTimetable())
def decorator_timetable():
    pass


@dag()
def decorator_deprecated_operator_args():
    trigger_dagrun_op = trigger_dagrun.TriggerDagRunOperator(
        task_id="trigger_dagrun_op1", trigger_dag_id="test", execution_date="2024-12-04"
    )
    trigger_dagrun_op2 = TriggerDagRunOperator(
        task_id="trigger_dagrun_op2", trigger_dag_id="test", execution_date="2024-12-04"
    )

    branch_dt_op = datetime.BranchDateTimeOperator(
        task_id="branch_dt_op", use_task_execution_day=True, task_concurrency=5
    )
    branch_dt_op2 = BranchDateTimeOperator(
        task_id="branch_dt_op2",
        use_task_execution_day=True,
        sla=timedelta(seconds=10),
    )

    dof_task_sensor = weekday.DayOfWeekSensor(
        task_id="dof_task_sensor",
        week_day=1,
        use_task_execution_day=True,
    )
    dof_task_sensor2 = DayOfWeekSensor(
        task_id="dof_task_sensor2",
        week_day=1,
        use_task_execution_day=True,
    )

    bdow_op = weekday.BranchDayOfWeekOperator(
        task_id="bdow_op",
        follow_task_ids_if_false=None,
        follow_task_ids_if_true=None,
        week_day=1,
        use_task_execution_day=True,
    )
    bdow_op2 = BranchDayOfWeekOperator(
        task_id="bdow_op2",
        follow_task_ids_if_false=None,
        follow_task_ids_if_true=None,
        week_day=1,
        use_task_execution_day=True,
    )

    trigger_dagrun_op >> trigger_dagrun_op2
    branch_dt_op >> branch_dt_op2
    dof_task_sensor >> dof_task_sensor2
    bdow_op >> bdow_op2


# deprecated filename_template argument in FileTaskHandler
S3TaskHandler(filename_template="/tmp/test")
HdfsTaskHandler(filename_template="/tmp/test")
ElasticsearchTaskHandler(filename_template="/tmp/test")
GCSTaskHandler(filename_template="/tmp/test")

FabAuthManager(None)
