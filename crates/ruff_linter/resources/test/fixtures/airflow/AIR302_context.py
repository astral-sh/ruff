import pendulum

from airflow.decorators import dag, task
from airflow.providers.standard.operators.python import PythonOperator


def access_invalid_key_in_context(**context):
    print("access invalid key", context["conf"])


@task
def access_invalid_key_task_out_of_dag(**context):
    print("access invalid key", context.get("conf"))


@dag(
    schedule=None,
    start_date=pendulum.datetime(2021, 1, 1, tz="UTC"),
    catchup=False,
    tags=[""],
)
def invalid_dag():
    @task()
    def access_invalid_key_task(**context):
        print("access invalid key", context.get("conf"))

    task1 = PythonOperator(
        task_id="task1",
        python_callable=access_invalid_key_in_context,
    )
    access_invalid_key_task() >> task1
    access_invalid_key_task_out_of_dag()


invalid_dag()
