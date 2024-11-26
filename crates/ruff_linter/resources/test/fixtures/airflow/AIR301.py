from airflow import DAG, dag

DAG(dag_id="class_default_schedule")

DAG(dag_id="class_schedule", schedule="@hourly")


@dag()
def decorator_default_schedule():
    pass


@dag(schedule="0 * * * *")
def decorator_schedule():
    pass
