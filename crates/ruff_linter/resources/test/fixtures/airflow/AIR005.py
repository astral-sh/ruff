from airflow import DAG
from airflow.models import Variable
from airflow.sdk import Variable as SdkVariable
from airflow.decorators import task
from airflow.operators.bash import BashOperator
from airflow.models.baseoperator import BaseOperator


# Violations (should trigger AIR005):

# Variable.get() at module level
foo = Variable.get("foo")  # AIR005
bar = SdkVariable.get("bar")  # AIR005

# Variable.get() in operator constructor arguments
BashOperator(
    task_id="bad_inline",
    bash_command=f"echo {Variable.get('foo')}",  # AIR005
)

BashOperator(
    task_id="bad_env",
    bash_command="echo $FOO",
    env={"FOO": Variable.get("foo")},  # AIR005
)

# Variable.get() in a regular (non-task) function
def helper():
    return Variable.get("foo")  # AIR005


# No violations:

# Variable.get() inside a @task-decorated function
@task
def my_task():
    var = Variable.get("foo")
    print(var)


# Variable.get() inside a @task() with parentheses
@task()
def my_task_with_parens():
    var = Variable.get("foo")
    print(var)


# Variable.get() inside a @task.branch-decorated function
@task.branch
def my_branch_task():
    if Variable.get("flag") == "true":
        return ["downstream"]
    return []


# Variable.get() inside an operator's execute() method
class MyOperator(BaseOperator):
    def execute(self, context):
        var = Variable.get("foo")
        print(var)


# Jinja template usage (no Variable.get() call at all)
BashOperator(
    task_id="good",
    bash_command="echo $FOO",
    env={"FOO": "{{ var.value.foo }}"},
)
