from airflow.operators import PythonOperator


def my_callable():
    pass


my_task = PythonOperator(task_id="my_task", callable=my_callable)
my_task_2 = PythonOperator(callable=my_callable, task_id="my_task_2")

incorrect_name = PythonOperator(task_id="my_task")
incorrect_name_2 = PythonOperator(callable=my_callable, task_id="my_task_2")

from my_module import MyClass

incorrect_name = MyClass(task_id="my_task")
