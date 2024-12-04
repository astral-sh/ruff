from airflow import DAG, dag
from airflow.timetables.simple import NullTimetable

from airflow.operators.trigger_dagrun import TriggerDagRunOperator
from airflow.providers.standard.operators import trigger_dagrun

from airflow.operators.datetime import BranchDateTimeOperator
from airflow.providers.standard.operators import datetime

from airflow.sensors.weekday import DayOfWeekSensor, BranchDayOfWeekOperator
from airflow.providers.standard.sensors import weekday

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
        task_id="branch_dt_op", use_task_execution_day=True
    )
    branch_dt_op2 = BranchDateTimeOperator(
        task_id="branch_dt_op2", use_task_execution_day=True
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
