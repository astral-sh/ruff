from airflow.decorators import task

condition1 = True
condition2 = True
condition3 = True


# Violations (should trigger AIR003):

@task.branch
def two_returns_one_non_empty():  # AIR003
    if condition1:
        return ["my_downstream_task"]
    return []


@task.branch
def three_returns_one_non_empty():  # AIR003
    if condition1:
        return []
    if condition2:
        return ["another_downstream_task"]
    return []


@task.branch
def nested_returns_one_non_empty():  # AIR003
    if condition1:
        if condition2:
            return []
        return ["downstream_task"]
    return []


# No violations:

@task.branch
def two_non_empty_returns():
    if condition1:
        return ["task_a"]
    if condition2:
        return ["task_b"]
    return []


@task.branch
def all_empty_returns():
    if condition1:
        return []
    if condition2:
        return []
    return []


@task.branch
def single_return():
    return ["downstream_task"]


def not_decorated():
    if condition1:
        return ["downstream_task"]
    return []


@task.short_circuit
def already_short_circuit():
    if condition1:
        return True
    return False
