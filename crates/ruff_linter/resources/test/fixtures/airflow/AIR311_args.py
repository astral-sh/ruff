from __future__ import annotations

from datetime import timedelta

from airflow import DAG, dag
from airflow.operators.datetime import BranchDateTimeOperator


def sla_callback(*arg, **kwargs):
    pass


DAG(dag_id="class_sla_callback", sla_miss_callback=sla_callback)


@dag(sla_miss_callback=sla_callback)
def decorator_sla_callback():
    pass


@dag()
def decorator_deprecated_operator_args():
    branch_dt_op2 = BranchDateTimeOperator(
        task_id="branch_dt_op2",
        sla=timedelta(seconds=10),
    )

    branch_dt_op2
