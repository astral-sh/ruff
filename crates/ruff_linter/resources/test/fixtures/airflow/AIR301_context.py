from __future__ import annotations

from datetime import datetime

import pendulum
from airflow.decorators import dag, task
from airflow.models import DAG
from airflow.models.baseoperator import BaseOperator
from airflow.operators.dummy import DummyOperator
from airflow.plugins_manager import AirflowPlugin
from airflow.providers.standard.operators.python import PythonOperator
from airflow.utils.context import get_current_context


def access_invalid_key_in_context(**context):
    print("access invalid key", context["conf"])
    print("access invalid key", context.get("conf"))


@task
def access_invalid_key_task_out_of_dag(**context):
    print("access invalid key", context["conf"])
    print("access invalid key", context.get("conf"))


@task
def access_invalid_argument_task_out_of_dag(
    execution_date, tomorrow_ds, logical_date, **context
):
    print("execution date", execution_date)
    print("access invalid key", context.get("conf"))


@task
def print_config(**context):
    # This should not throw an error as logical_date is part of airflow context.
    logical_date = context["logical_date"]

    # Removed usage - should trigger violations
    execution_date = context["execution_date"]
    next_ds = context["next_ds"]
    next_ds_nodash = context["next_ds_nodash"]
    next_execution_date = context["next_execution_date"]
    prev_ds = context["prev_ds"]
    prev_ds_nodash = context["prev_ds_nodash"]
    prev_execution_date = context["prev_execution_date"]
    prev_execution_date_success = context["prev_execution_date_success"]
    tomorrow_ds = context["tomorrow_ds"]
    yesterday_ds = context["yesterday_ds"]
    yesterday_ds_nodash = context["yesterday_ds_nodash"]


@task
def print_config_with_get_current_context():
    context = get_current_context()
    execution_date = context["execution_date"]
    next_ds = context["next_ds"]
    next_ds_nodash = context["next_ds_nodash"]
    next_execution_date = context["next_execution_date"]
    prev_ds = context["prev_ds"]
    prev_ds_nodash = context["prev_ds_nodash"]
    prev_execution_date = context["prev_execution_date"]
    prev_execution_date_success = context["prev_execution_date_success"]
    tomorrow_ds = context["tomorrow_ds"]
    yesterday_ds = context["yesterday_ds"]
    yesterday_ds_nodash = context["yesterday_ds_nodash"]


@task(task_id="print_the_context")
def print_context(ds=None, **kwargs):
    """Print the Airflow context and ds variable from the context."""
    print(ds)
    print(kwargs.get("tomorrow_ds"))
    c = get_current_context()
    c.get("execution_date")


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

    @task()
    def access_invalid_key_explicit_task(execution_date):
        print(execution_date)

    task1 = PythonOperator(
        task_id="task1",
        python_callable=access_invalid_key_in_context,
    )

    access_invalid_key_task() >> task1
    access_invalid_key_explicit_task()
    access_invalid_argument_task_out_of_dag()
    access_invalid_key_task_out_of_dag()
    print_config()
    print_config_with_get_current_context()
    print_context()


invalid_dag()

with DAG(
    dag_id="example_dag",
    schedule_interval="@daily",
    start_date=datetime(2023, 1, 1),
    template_searchpath=["/templates"],
) as dag:
    task1 = DummyOperator(
        task_id="task1",
        params={
            # Removed variables in template
            "execution_date": "{{ execution_date }}",
            "next_ds": "{{ next_ds }}",
            "prev_ds": "{{ prev_ds }}",
        },
    )


class CustomMacrosPlugin(AirflowPlugin):
    name = "custom_macros"
    macros = {
        "execution_date_macro": lambda context: context["execution_date"],
        "next_ds_macro": lambda context: context["next_ds"],
    }


class CustomOperator(BaseOperator):
    def execute(self, next_ds, context):
        execution_date = context["execution_date"]
        next_ds = context["next_ds"]
        next_ds_nodash = context["next_ds_nodash"]
        next_execution_date = context["next_execution_date"]
        prev_ds = context["prev_ds"]
        prev_ds_nodash = context["prev_ds_nodash"]
        prev_execution_date = context["prev_execution_date"]
        prev_execution_date_success = context["prev_execution_date_success"]
        tomorrow_ds = context["tomorrow_ds"]
        yesterday_ds = context["yesterday_ds"]
        yesterday_ds_nodash = context["yesterday_ds_nodash"]


class CustomOperatorNew(BaseOperator):
    def execute(self, context):
        execution_date = context.get("execution_date")
        next_ds = context.get("next_ds")
