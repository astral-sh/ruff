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


# Mixed-content cases that SHOULD trigger AIR201:

# Positional arg with surrounding text (was previously non-triggering; now detected)
task_10 = BashOperator(
    task_id="task_10",
    bash_command="echo {{ ti.xcom_pull(task_ids='task_1') }}",  # AIR201
)

# task_ids= keyword with text before
task_mc1 = BashOperator(
    task_id="task_mc1",
    bash_command="result: {{ ti.xcom_pull(task_ids='task_1') }}",  # AIR201
)

# task_id= keyword (singular) with surrounding text
task_mc2 = BashOperator(
    task_id="task_mc2",
    bash_command="echo {{ ti.xcom_pull(task_id='task_1') }}",  # AIR201
)

# list-wrapped task_ids with surrounding text
task_mc3 = BashOperator(
    task_id="task_mc3",
    bash_command="echo {{ ti.xcom_pull(task_ids=['task_1']) }}",  # AIR201
)

# tuple-wrapped task_ids with surrounding text
task_mc4 = BashOperator(
    task_id="task_mc4",
    bash_command="echo {{ ti.xcom_pull(task_ids=('task_1',)) }}",  # AIR201
)

# key='return_value' with surrounding text
task_mc5 = BashOperator(
    task_id="task_mc5",
    bash_command="echo {{ ti.xcom_pull(task_ids='task_1', key='return_value') }}",  # AIR201
)

# task_instance receiver in mixed-content
task_mc6 = BashOperator(
    task_id="task_mc6",
    bash_command="echo {{ task_instance.xcom_pull('task_1') }}",  # AIR201
)

# Two xcom_pull patterns in one string — each flagged independently (same task_id)
task_mc7 = BashOperator(
    task_id="task_mc7",
    bash_command="{{ ti.xcom_pull('task_1') }} and {{ ti.xcom_pull('task_1') }}",  # AIR201, AIR201
)

# Two xcom_pull patterns in one string referencing DIFFERENT task_ids — each
# flagged and fixed independently, verifying the scanner handles distinct
# task variable references within the same template string
task_mc11 = BashOperator(
    task_id="task_mc11",
    bash_command="{{ ti.xcom_pull('task_1') }} and {{ ti.xcom_pull('task_2') }}",  # AIR201, AIR201
)

# Subscript access within the Jinja block — only xcom_pull call is replaced,
# subscript and surrounding {{ }} are preserved
task_mc8 = BashOperator(
    task_id="task_mc8",
    bash_command="{{ ti.xcom_pull('task_1')['item'] }}",  # AIR201
)

# Triple-quoted string — raw-source scanner is quote-agnostic
task_mc9 = BashOperator(
    task_id="task_mc9",
    bash_command="""echo {{ ti.xcom_pull('task_1') }}""",  # AIR201
)

# Mixed-content with unknown task_id (violation reported, no fix available)
task_mc10 = BashOperator(
    task_id="task_mc10",
    bash_command="echo {{ ti.xcom_pull('unknown_task') }}",  # AIR201 (no fix)
)


# Cases that should NOT trigger AIR201:

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

# Non-triggering: kebab-case task ID in pure-template form (not a valid Python identifier)
task_nt1 = BashOperator(
    task_id="task_nt1",
    bash_command="{{ ti.xcom_pull('kebab-task') }}",
)

# Non-triggering: kebab-case task ID in mixed-content form
task_nt2 = BashOperator(
    task_id="task_nt2",
    bash_command="echo {{ ti.xcom_pull('kebab-task') }}",
)

# Non-triggering: dotted group task ID (not a valid Python identifier; future
# rule could suggest group['task'].output once Airflow supports that syntax)
task_nt3 = BashOperator(
    task_id="task_nt3",
    bash_command="{{ ti.xcom_pull('group_1.task_1') }}",
)

# Non-triggering: f-string arguments are ExprFString, not ExprStringLiteral,
# and are excluded by as_string_literal_expr(). The runtime value of this
# f-string is "{{ ti.xcom_pull('task_1') }}" — a valid Jinja template — but
# the task ID is determined at runtime via {task_1.task_id}, so AIR201 cannot
# statically analyze it.
task_nt4 = BashOperator(
    task_id="task_nt4",
    bash_command=f"{{{{ ti.xcom_pull('{task_1.task_id}') }}}}",
)
