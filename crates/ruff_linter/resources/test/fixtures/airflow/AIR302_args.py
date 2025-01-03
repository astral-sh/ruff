from datetime import timedelta

from airflow import DAG, dag
from airflow.operators.datetime import BranchDateTimeOperator
from airflow.operators.trigger_dagrun import TriggerDagRunOperator
from airflow.providers.amazon.aws.log.s3_task_handler import S3TaskHandler
from airflow.providers.apache.hdfs.log.hdfs_task_handler import HdfsTaskHandler
from airflow.providers.elasticsearch.log.es_task_handler import ElasticsearchTaskHandler
from airflow.providers.fab.auth_manager.fab_auth_manager import FabAuthManager
from airflow.providers.google.cloud.log.gcs_task_handler import GCSTaskHandler
from airflow.providers.standard.operators import datetime, trigger_dagrun
from airflow.providers.standard.sensors import weekday
from airflow.sensors.weekday import BranchDayOfWeekOperator, DayOfWeekSensor
from airflow.timetables.simple import NullTimetable

DAG(dag_id="class_schedule", schedule="@hourly")

DAG(dag_id="class_schedule_interval", schedule_interval="@hourly")

DAG(dag_id="class_timetable", timetable=NullTimetable())


def sla_callback(*arg, **kwargs):
    pass


DAG(dag_id="class_sla_callback", sla_miss_callback=sla_callback)


@dag(schedule="0 * * * *")
def decorator_schedule():
    pass


@dag(schedule_interval="0 * * * *")
def decorator_schedule_interval():
    pass


@dag(timetable=NullTimetable())
def decorator_timetable():
    pass


@dag(sla_miss_callback=sla_callback)
def decorator_sla_callback():
    pass


@dag()
def decorator_deprecated_operator_args():
    trigger_dagrun_op = trigger_dagrun.TriggerDagRunOperator(
        task_id="trigger_dagrun_op1", execution_date="2024-12-04"
    )
    trigger_dagrun_op2 = TriggerDagRunOperator(
        task_id="trigger_dagrun_op2", execution_date="2024-12-04"
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
        task_id="dof_task_sensor", use_task_execution_day=True
    )
    dof_task_sensor2 = DayOfWeekSensor(
        task_id="dof_task_sensor2", use_task_execution_day=True
    )

    bdow_op = weekday.BranchDayOfWeekOperator(
        task_id="bdow_op", use_task_execution_day=True
    )
    bdow_op2 = BranchDayOfWeekOperator(task_id="bdow_op2", use_task_execution_day=True)

    trigger_dagrun_op >> trigger_dagrun_op2
    branch_dt_op >> branch_dt_op2
    dof_task_sensor >> dof_task_sensor2
    bdow_op >> bdow_op2


# deprecated filename_template arugment in FileTaskHandler
S3TaskHandler(filename_template="/tmp/test")
HdfsTaskHandler(filename_template="/tmp/test")
ElasticsearchTaskHandler(filename_template="/tmp/test")
GCSTaskHandler(filename_template="/tmp/test")

FabAuthManager(None)
