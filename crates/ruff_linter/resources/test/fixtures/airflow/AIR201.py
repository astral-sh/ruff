from airflow.decorators import task
from airflow.operators.bash import BashOperator
from airflow.operators.python import PythonOperator
from airflow.providers.http.operators.http import HttpOperator
from airflow.sensors.external_task import ExternalTaskSensor


def my_callable():
    pass


# Cases that SHOULD trigger AIR201:

# Basic xcom_pull with ti
task_1 = PythonOperator(task_id="task_1", python_callable=my_callable)
task_2 = BashOperator(
    task_id="task_2",
    bash_command="{{ ti.xcom_pull(task_ids='task_1') }}",  # AIR201 (fix: task_1.output)
)

# With task_instance instead of ti
task_3 = BashOperator(
    task_id="task_3",
    bash_command="{{ task_instance.xcom_pull(task_ids='task_1') }}",  # AIR201 (fix: task_1.output)
)

# Positional argument
task_4 = BashOperator(
    task_id="task_4",
    bash_command="{{ ti.xcom_pull('task_1') }}",  # AIR201 (fix: task_1.output)
)

# Double quotes in template
task_5 = BashOperator(
    task_id="task_5",
    bash_command='{{ ti.xcom_pull(task_ids="task_1") }}',  # AIR201 (fix: task_1.output)
)

# No spaces in braces
task_6 = BashOperator(
    task_id="task_6",
    bash_command="{{ti.xcom_pull(task_ids='task_1')}}",  # AIR201 (fix: task_1.output)
)

# Singular task_id keyword
task_7 = BashOperator(
    task_id="task_7",
    bash_command="{{ ti.xcom_pull(task_id='task_1') }}",  # AIR201 (fix: task_1.output)
)

# With sensor
task_8 = ExternalTaskSensor(
    task_id="task_8",
    external_task_id="{{ ti.xcom_pull(task_ids='task_1') }}",  # AIR201 (fix: task_1.output)
)

# Provider operator
task_9 = HttpOperator(
    task_id="task_9",
    endpoint="{{ ti.xcom_pull(task_ids='task_1') }}",  # AIR201 (fix: task_1.output)
)


# task_ids as a single-element list
task_18 = BashOperator(
    task_id="task_18",
    bash_command="{{ ti.xcom_pull(task_ids=['task_1']) }}",  # AIR201 (fix: task_1.output)
)

# task_ids as a single-element tuple
task_19 = BashOperator(
    task_id="task_19",
    bash_command="{{ ti.xcom_pull(task_ids=('task_1',)) }}",  # AIR201 (fix: task_1.output)
)

# With explicit key='return_value' (default)
task_20 = BashOperator(
    task_id="task_20",
    bash_command="{{ ti.xcom_pull(task_ids='task_1', key='return_value') }}",  # AIR201 (fix: task_1.output)
)

# List + key='return_value'
task_21 = BashOperator(
    task_id="task_21",
    bash_command="{{ ti.xcom_pull(task_ids=['task_1'], key='return_value') }}",  # AIR201 (fix: task_1.output)
)

# task_instance with list + key="return_value"
task_22 = BashOperator(
    task_id="task_22",
    bash_command='{{ task_instance.xcom_pull(task_ids=["task_1"], key="return_value") }}',  # AIR201 (fix: task_1.output)
)

# Reordered keyword arguments (key before task_ids)
task_25 = BashOperator(
    task_id="task_25",
    bash_command="{{ ti.xcom_pull(key='return_value', task_ids='task_1') }}",  # AIR201 (fix: task_1.output)
)

# Pulling output from a @task-decorated function
@task
def extract_data():
    return {"key": "value"}


task_16 = BashOperator(
    task_id="task_16",
    bash_command="{{ ti.xcom_pull(task_ids='extract_data') }}",  # AIR201 (fix: extract_data.output)
)

# Referencing a task_id that is not a visible variable (no fix, but still flags)
task_17 = BashOperator(
    task_id="task_17",
    bash_command="{{ ti.xcom_pull(task_ids='unknown_task') }}",  # AIR201 (no fix)
)


# Cases that should NOT trigger AIR201:

# Mixed content (not just xcom_pull) — the modern replacement
# for this pattern is: bash_command="echo {{ task_1.output }}"
task_10 = BashOperator(
    task_id="task_10",
    bash_command="echo {{ ti.xcom_pull(task_ids='task_1') }}",
)

# Additional keyword arguments (non-default key)
task_11 = BashOperator(
    task_id="task_11",
    bash_command="{{ ti.xcom_pull(task_ids='task_1', key='my_key') }}",
)

# Already using .output
task_12 = BashOperator(
    task_id="task_12",
    bash_command=task_1.output,
)

# Regular string (no template)
task_13 = BashOperator(
    task_id="task_13",
    bash_command="echo hello",
)

# Non-string argument
task_14 = BashOperator(
    task_id="task_14",
    bash_command=42,
)

# List of task_ids (multiple elements)
task_15 = BashOperator(
    task_id="task_15",
    bash_command="{{ ti.xcom_pull(task_ids=['task_1', 'task_2']) }}",
)

# Non-default key with single-element list
task_23 = BashOperator(
    task_id="task_23",
    bash_command="{{ ti.xcom_pull(task_ids=['task_1'], key='custom_key') }}",
)

# Non-default key with scalar task_ids
task_24 = BashOperator(
    task_id="task_24",
    bash_command="{{ ti.xcom_pull(task_ids='task_1', key='custom_key') }}",
)

# Not an operator call (not from airflow.operators or airflow.sensors)
from airflow import DAG

dag = DAG(
    dag_id="my_dag",
    schedule=None,
    description="{{ ti.xcom_pull(task_ids='task_1') }}",
)
