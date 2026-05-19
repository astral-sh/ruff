from airflow.sdk import task

condition1 = True
condition2 = True


# Violations (should trigger AIR004):

@task.branch
def sdk_two_returns_one_non_empty():  # AIR004
    if condition1:
        return ["my_downstream_task"]
    return []


# No violations:

@task.branch
def sdk_two_non_empty_returns():
    if condition1:
        return ["task_a"]
    if condition2:
        return ["task_b"]
    return []
