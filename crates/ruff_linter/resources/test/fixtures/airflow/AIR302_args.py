from airflow import DAG, dag
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
