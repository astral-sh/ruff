from airflow import DAG, dag
from airflow.timetables.simple import NullTimetable

from airflow.operators.trigger_dagrun import TriggerDagRunOperator
from airflow.providers.standard.operators import trigger_dagrun

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
def decorator_trigger_dag_run_operator():
    trigger_dagrun_op1 = trigger_dagrun.TriggerDagRunOperator(
        task_id="trigger_dagrun_op1", execution_date="2024-12-04"
    )
    trigger_dagrun_op2 = TriggerDagRunOperator(
        task_id="trigger_dagrun_op2", execution_date="2024-12-04"
    )

    trigger_dagrun_op1 >> trigger_dagrun_op2
