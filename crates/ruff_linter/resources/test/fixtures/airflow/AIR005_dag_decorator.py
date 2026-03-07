from airflow.decorators import dag, task
from airflow.models import Variable

var = Variable.get("foo")  # AIR005


@dag
def my_dag():
    @task
    def my_task():
        Variable.get("bar")  # OK - inside task

    my_task()


my_dag()
