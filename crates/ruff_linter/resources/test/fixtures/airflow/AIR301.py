from airflow import DAG, dag
from airflow.timetables.trigger import CronTriggerTimetable

DAG(dag_id="class_default_schedule")

DAG(dag_id="class_schedule", schedule="@hourly")

DAG(dag_id="class_timetable", timetable=CronTriggerTimetable())

DAG(dag_id="class_schedule_interval", schedule_interval="@hourly")


@dag()
def decorator_default_schedule():
    pass


@dag(schedule="0 * * * *")
def decorator_schedule():
    pass


@dag(timetable=CronTriggerTimetable())
def decorator_timetable():
    pass


@dag(schedule_interval="0 * * * *")
def decorator_schedule_interval():
    pass
