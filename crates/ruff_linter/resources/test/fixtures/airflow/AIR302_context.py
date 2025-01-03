from airflow.models import DAG
from airflow.operators.dummy import DummyOperator
from datetime import datetime
from airflow.plugins_manager import AirflowPlugin
from airflow.decorators import task, get_current_context
from airflow.models.baseoperator import BaseOperator

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
            "prev_ds": "{{ prev_ds }}"
        },
    )

class CustomMacrosPlugin(AirflowPlugin):
    name = "custom_macros"
    macros = {
        "execution_date_macro": lambda context: context["execution_date"],
        "next_ds_macro": lambda context: context["next_ds"]
    }

@task
def print_config():
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

class CustomOperator(BaseOperator):
    def execute(self, context):
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
