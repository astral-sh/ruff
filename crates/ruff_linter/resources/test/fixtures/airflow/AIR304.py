import random
import time
import uuid
from datetime import date, datetime, timedelta

import pendulum
from airflow import DAG, dag
from airflow.decorators import task
from airflow.operators.bash import BashOperator
from airflow.providers.standard.sensors.python import PythonSensor


# Violations

DAG(dag_id="a", start_date=datetime.now())
DAG(dag_id="b", start_date=datetime.now() - timedelta(days=1))
DAG(dag_id="c", default_args={"start_date": datetime.now()})
DAG(dag_id="d", start_date=datetime.utcnow())
DAG(dag_id="e", start_date=datetime.today())
DAG(dag_id="f", start_date=date.today())
DAG(dag_id="g", tags=[f"v{random.randint(1, 9)}"])
DAG(dag_id="h", start_date=pendulum.now())
DAG(dag_id="i", start_date=pendulum.yesterday())
DAG(dag_id="j", start_date=pendulum.tomorrow())
DAG(dag_id="k", start_date=pendulum.today())
DAG(dag_id="l", owner=f"team-{uuid.uuid4()}")
DAG(dag_id="m", description=f"built at {time.time()}")


@dag(start_date=pendulum.now())
def my_dag():
    pass


BashOperator(task_id="t", bash_command="echo hi", start_date=datetime.today())

PythonSensor(task_id="s", start_date=datetime.now())


@task(start_date=datetime.utcnow())
def my_task():
    pass


# Non-violations

DAG(dag_id="ok_a", start_date=datetime(2024, 1, 1))
DAG(dag_id="ok_b", start_date=pendulum.datetime(2024, 1, 1))
DAG(dag_id="ok_c", schedule=timedelta(hours=1))
DAG(dag_id="ok_d", default_args={"retries": 3})

BashOperator(task_id="t_ok", bash_command="echo hello")


@task(retries=3)
def my_static_task():
    pass


# Non-airflow function call with dynamic arg — should not trigger
def not_airflow(start_date):
    pass


not_airflow(start_date=datetime.now())
