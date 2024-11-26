from airflow.operators.python import PythonOperator
from airflow import DAG

# Using deprecated task_concurrency parameter
task1 = PythonOperator(
    task_id="task1",
    task_concurrency=2,  # This should trigger AIR303
    python_callable=lambda: None,
)

# Using new max_active_tis_per_dag parameter
task2 = PythonOperator(
    task_id="task2",
    max_active_tis_per_dag=2,  # This should be fine
    python_callable=lambda: None,
)

# Another example with task_concurrency
task3 = PythonOperator(
    task_id="task3",
    task_concurrency=5,  # This should trigger AIR303
    python_callable=lambda: None,
)
