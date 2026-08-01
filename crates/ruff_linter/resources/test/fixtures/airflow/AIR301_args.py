from __future__ import annotations

from datetime import timedelta

from airflow import DAG, dag
from airflow.operators.datetime import BranchDateTimeOperator
from airflow.operators.trigger_dagrun import TriggerDagRunOperator
from airflow.operators.weekday import BranchDayOfWeekOperator
from airflow.providers.amazon.aws.log.s3_task_handler import S3TaskHandler
from airflow.providers.apache.hdfs.log.hdfs_task_handler import HdfsTaskHandler
from airflow.providers.elasticsearch.log.es_task_handler import ElasticsearchTaskHandler
from airflow.providers.fab.auth_manager.fab_auth_manager import FabAuthManager
from airflow.providers.google.cloud.log.gcs_task_handler import GCSTaskHandler
from airflow.providers.standard.operators import datetime, trigger_dagrun
from airflow.providers.standard.sensors import weekday
from airflow.sensors.weekday import DayOfWeekSensor
from airflow.timetables.simple import NullTimetable

DAG(dag_id="class_schedule", schedule="@hourly")

DAG(dag_id="class_schedule_interval", schedule_interval="@hourly")

DAG(dag_id="class_timetable", timetable=NullTimetable())

DAG(dag_id="class_concurrency", concurrency=12)

DAG(dag_id="class_fail_stop", fail_stop=True)

DAG(dag_id="class_default_view", default_view="dag_default_view")

DAG(dag_id="class_orientation", orientation="BT")

allow_future_exec_dates_dag = DAG(dag_id="class_allow_future_exec_dates")
allow_future_exec_dates_dag.allow_future_exec_dates


@dag(schedule="0 * * * *")
def decorator_schedule():
    pass


@dag(schedule_interval="0 * * * *")
def decorator_schedule_interval():
    pass


@dag(timetable=NullTimetable())
def decorator_timetable():
    pass


@dag()
def decorator_deprecated_operator_args():
    trigger_dagrun_op = trigger_dagrun.TriggerDagRunOperator(
        task_id="trigger_dagrun_op1", trigger_dag_id="test", execution_date="2024-12-04"
    )
    trigger_dagrun_op2 = TriggerDagRunOperator(
        task_id="trigger_dagrun_op2", trigger_dag_id="test", execution_date="2024-12-04"
    )

    branch_dt_op = datetime.BranchDateTimeOperator(
        task_id="branch_dt_op", use_task_execution_day=True, task_concurrency=5
    )
    branch_dt_op2 = BranchDateTimeOperator(
        task_id="branch_dt_op2",
        use_task_execution_day=True,
        sla=timedelta(seconds=10),
    )

    dof_task_sensor = weekday.DayOfWeekSensor(
        task_id="dof_task_sensor",
        week_day=1,
        use_task_execution_day=True,
    )
    dof_task_sensor2 = DayOfWeekSensor(
        task_id="dof_task_sensor2",
        week_day=1,
        use_task_execution_day=True,
    )

    bdow_op = weekday.BranchDayOfWeekOperator(
        task_id="bdow_op",
        follow_task_ids_if_false=None,
        follow_task_ids_if_true=None,
        week_day=1,
        use_task_execution_day=True,
    )
    bdow_op2 = BranchDayOfWeekOperator(
        task_id="bdow_op2",
        follow_task_ids_if_false=None,
        follow_task_ids_if_true=None,
        week_day=1,
        use_task_execution_day=True,
    )

    trigger_dagrun_op >> trigger_dagrun_op2
    branch_dt_op >> branch_dt_op2
    dof_task_sensor >> dof_task_sensor2
    bdow_op >> bdow_op2


# deprecated filename_template argument in FileTaskHandler
S3TaskHandler(filename_template="/tmp/test")
HdfsTaskHandler(filename_template="/tmp/test")
ElasticsearchTaskHandler(filename_template="/tmp/test")
GCSTaskHandler(filename_template="/tmp/test")

FabAuthManager(None)


# airflow.sdk.Variable
from airflow.sdk import Variable

Variable.get("key", default_var="deprecated")
Variable.get(key="key", default_var="deprecated")

Variable.get("key", default="default")
Variable.get(key="key", default="default")
Variable.get("key")
Variable.get("key", deserialize_json=True)


# airflow.providers.standard.operators.python (provide_context parameter removed)
def virtualenv_callable():
    pass

from airflow.providers.standard.operators.python import (
    PythonOperator,
    BranchPythonOperator,
    ShortCircuitOperator,
    PythonVirtualenvOperator,
    BranchPythonVirtualenvOperator,
    ExternalPythonOperator,
    BranchExternalPythonOperator,
)

PythonOperator(task_id="task", python_callable=lambda: None, provide_context=True)
BranchPythonOperator(task_id="task", python_callable=lambda: None, provide_context=True)
ShortCircuitOperator(task_id="task", python_callable=lambda: None, provide_context=True)
PythonVirtualenvOperator(
    task_id="virtualenv_python",
    python_callable=virtualenv_callable,
    requirements=["colorama==0.4.0"],
    system_site_packages=False,
    provide_context=True,
)
BranchPythonVirtualenvOperator(
    task_id="branching_venv",
    requirements=["numpy~=1.26.0"],
    venv_cache_path="some_path",
    python_callable=virtualenv_callable,
    op_args=["some_args"],
    provide_context=True,
)
ExternalPythonOperator(
    task_id="external_python",
    python_callable=virtualenv_callable,
    python="some_path",
    provide_context=True,
)
BranchExternalPythonOperator(
    task_id="branching_ext_python",
    python="some_path",
    python_callable=virtualenv_callable,
    op_args=["some_args"],
    provide_context=True,
)

PythonOperator(task_id="task", python_callable=lambda: None)
BranchPythonOperator(task_id="task", python_callable=lambda: None)
ShortCircuitOperator(task_id="task", python_callable=lambda: None)
PythonVirtualenvOperator(
    task_id="virtualenv_python",
    python_callable=virtualenv_callable,
    requirements=["colorama==0.4.0"],
    system_site_packages=False,
)
BranchPythonVirtualenvOperator(
    task_id="branching_venv",
    requirements=["numpy~=1.26.0"],
    venv_cache_path="some_path",
    python_callable=virtualenv_callable,
    op_args=["some_args"],
)
ExternalPythonOperator(
    task_id="external_python",
    python_callable=virtualenv_callable,
    python="some_path",
)
BranchExternalPythonOperator(
    task_id="branching_ext_python",
    python="some_path",
    python_callable=virtualenv_callable,
    op_args=["some_args"],
)

# airflow.operators.python (provide_context parameter removed)
from airflow.operators.python import (
    PythonOperator,
    BranchPythonOperator,
    ShortCircuitOperator,
    PythonVirtualenvOperator,
    BranchPythonVirtualenvOperator,
    ExternalPythonOperator,
    BranchExternalPythonOperator,
)

PythonOperator(task_id="task", python_callable=lambda: None, provide_context=True)
BranchPythonOperator(task_id="task", python_callable=lambda: None, provide_context=True)
ShortCircuitOperator(task_id="task", python_callable=lambda: None, provide_context=True)
PythonVirtualenvOperator(
    task_id="virtualenv_python",
    python_callable=virtualenv_callable,
    requirements=["colorama==0.4.0"],
    system_site_packages=False,
    provide_context=True,
)
BranchPythonVirtualenvOperator(
    task_id="branching_venv",
    requirements=["numpy~=1.26.0"],
    venv_cache_path="some_path",
    python_callable=virtualenv_callable,
    op_args=["some_args"],
    provide_context=True,
)
ExternalPythonOperator(
    task_id="external_python",
    python_callable=virtualenv_callable,
    python="some_path",
    provide_context=True,
)
BranchExternalPythonOperator(
    task_id="branching_ext_python",
    python="some_path",
    python_callable=virtualenv_callable,
    op_args=["some_args"],
    provide_context=True,
)

PythonOperator(task_id="task", python_callable=lambda: None)
BranchPythonOperator(task_id="task", python_callable=lambda: None)
ShortCircuitOperator(task_id="task", python_callable=lambda: None)
PythonVirtualenvOperator(
    task_id="virtualenv_python",
    python_callable=virtualenv_callable,
    requirements=["colorama==0.4.0"],
    system_site_packages=False,
)
BranchPythonVirtualenvOperator(
    task_id="branching_venv",
    requirements=["numpy~=1.26.0"],
    venv_cache_path="some_path",
    python_callable=virtualenv_callable,
    op_args=["some_args"],
)
ExternalPythonOperator(
    task_id="external_python",
    python_callable=virtualenv_callable,
    python="some_path",
)
BranchExternalPythonOperator(
    task_id="branching_ext_python",
    python="some_path",
    python_callable=virtualenv_callable,
    op_args=["some_args"],
)


# airflow.utils.trigger_rule.TriggerRule.NONE_FAILED_OR_SKIPPED is deprecated.
from airflow.decorators import task
from airflow.operators.python import PythonOperator
from airflow.utils.trigger_rule import TriggerRule

@task(trigger_rule="none_failed_or_skipped")
def invalid_trigger_rule_task():
    pass

@task(trigger_rule=TriggerRule.NONE_FAILED_OR_SKIPPED)
def invalid_trigger_rule_task():
    pass

@task(trigger_rule="all_success")
def valid_trigger_rule_task():
    pass

PythonOperator(task_id="invalid_trigger_rule_task", python_callable=lambda: None, trigger_rule="none_failed_or_skipped")

PythonOperator(task_id="invalid_trigger_rule_task", python_callable=lambda: None, trigger_rule=TriggerRule.NONE_FAILED_OR_SKIPPED)

PythonOperator(task_id="valid_trigger_rule_task", python_callable=lambda: None, trigger_rule="all_success")

from airflow.sdk import task
from airflow.providers.standard.operators.python import PythonOperator

@task(trigger_rule="none_failed_or_skipped")
def invalid_trigger_rule_task():
    pass

@task(trigger_rule=TriggerRule.NONE_FAILED_OR_SKIPPED)
def invalid_trigger_rule_task():
    pass

@task(trigger_rule="all_success")
def valid_trigger_rule_task():
    pass

PythonOperator(task_id="invalid_trigger_rule_task", python_callable=lambda: None, trigger_rule="none_failed_or_skipped")

PythonOperator(task_id="invalid_trigger_rule_task", python_callable=lambda: None, trigger_rule=TriggerRule.NONE_FAILED_OR_SKIPPED)

PythonOperator(task_id="valid_trigger_rule_task", python_callable=lambda: None, trigger_rule="all_success")

from airflow.providers.common.sql.operators.sql import SQLExecuteQueryOperator

execute_query = SQLExecuteQueryOperator(
    task_id="execute_query",
    sql="SELECT 1; SELECT * FROM AIRFLOW_DB_METADATA_TABLE LIMIT 1;",
    split_statements=True,
    return_last=False,
    trigger_rule="none_failed_or_skipped",
)

execute_query = SQLExecuteQueryOperator(
    task_id="execute_query",
    sql="SELECT 1; SELECT * FROM AIRFLOW_DB_METADATA_TABLE LIMIT 1;",
    split_statements=True,
    return_last=False,
    trigger_rule=TriggerRule.NONE_FAILED_OR_SKIPPED,
)

from airflow.providers.amazon.aws.operators.s3 import S3FileTransformOperator

file_transform = S3FileTransformOperator(
    task_id="file_transform",
    source_s3_key="s3://bucket/key",
    dest_s3_key="s3://bucket_2/key_2",
    transform_script="cp",
    replace=True,
    trigger_rule="none_failed_or_skipped",
)

file_transform = S3FileTransformOperator(
    task_id="file_transform",
    source_s3_key="s3://bucket/key",
    dest_s3_key="s3://bucket_2/key_2",
    transform_script="cp",
    replace=True,
    trigger_rule=TriggerRule.NONE_FAILED_OR_SKIPPED,
)
