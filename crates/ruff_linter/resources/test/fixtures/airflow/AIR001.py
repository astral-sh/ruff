from airflow.operators import PythonOperator
from airflow.providers.airbyte.operators.airbyte import AirbyteTriggerSyncOperator
from airflow.providers.amazon.aws.operators.appflow import AppflowFlowRunOperator


def my_callable():
    pass


my_task = PythonOperator(task_id="my_task", callable=my_callable)
incorrect_name = PythonOperator(task_id="my_task")  # AIR001

my_task = AirbyteTriggerSyncOperator(task_id="my_task", callable=my_callable)
incorrect_name = AirbyteTriggerSyncOperator(task_id="my_task")  # AIR001

my_task = AppflowFlowRunOperator(task_id="my_task", callable=my_callable)
incorrect_name = AppflowFlowRunOperator(task_id="my_task")  # AIR001

# Consider only from the `airflow.operators` (or providers operators) module
from airflow import MyOperator

incorrect_name = MyOperator(task_id="my_task")
