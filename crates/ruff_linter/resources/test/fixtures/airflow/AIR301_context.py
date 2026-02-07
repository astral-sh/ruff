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
    events = context["triggering_dataset_events"]


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
    events = context["triggering_dataset_events"]


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
        events = context["triggering_dataset_events"]


class CustomOperatorNew(BaseOperator):
    def execute(self, context):
        execution_date = context.get("execution_date")
        next_ds = context.get("next_ds")


# dagrun.external_trigger is removed
@task
def test_dag_run_external_trigger_via_variable(**context):
    dag_run = context["dag_run"]
    print(dag_run.external_trigger)
    print(dag_run.run_type)
    print(dag_run.dag_id)

@task
def test_dag_run_external_trigger_via_get(**context):
    dag_run = context.get("dag_run")
    print(dag_run.external_trigger)

@task
def test_dag_run_external_trigger_direct_subscript(**context):
    print(context["dag_run"].external_trigger)

@task
def test_dag_run_external_trigger_direct_get(**context):
    print(context.get("dag_run").external_trigger)

@task
def test_dag_run_external_trigger_get_current_context():
    context = get_current_context()
    dag_run = context["dag_run"]
    print(dag_run.external_trigger)

# inlet_events should be accessed through Asset/Dataset object
@task
def test_inlet_events_string_subscript(**context):
    print(context["inlet_events"]["this://is-url"])
    print(context.get("inlet_events")["this://is-url"])
    print(context["inlet_events"].get("this://is-url"))
    print(context.get("inlet_events").get("this://is-url"))


@task
def test_inlet_events_string_subscript_via_variable(**context):
    inlet_events = context["inlet_events"]
    print(inlet_events["this://is-url"])
    print(inlet_events.get("this://is-url"))


@task
def test_inlet_events_dataset_subscript_ok(**context):
    from airflow.datasets import Dataset
    from airflow.sdk import Asset

    print(context["inlet_events"][Dataset("this://is-url")])
    print(context["inlet_events"][Asset("this://is-url")])
