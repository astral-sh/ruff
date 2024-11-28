from airflow.operators.subdag import SubDagOperator
from airflow.operators.python import PythonOperator
from airflow.utils.task_group import TaskGroup  # This is the recommended alternative

# This should trigger AIR304
task1 = SubDagOperator(
    task_id="subdag_task",
    subdag=some_subdag,
)

# This should be fine
task2 = PythonOperator(
    task_id="python_task",
    python_callable=lambda: None,
)

# This is the recommended way using TaskGroup
with TaskGroup(group_id="my_task_group") as task_group:
    task4 = PythonOperator(
        task_id="task_in_group",
        python_callable=lambda: None,
    )

# Direct import should also trigger AIR304
from airflow.operators.subdag_operator import SubDagOperator

task3 = SubDagOperator(
    task_id="another_subdag",
    subdag=another_subdag,
)
