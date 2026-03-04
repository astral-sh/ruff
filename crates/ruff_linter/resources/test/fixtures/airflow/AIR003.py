from airflow.decorators import task
from airflow.operators.python import BranchPythonOperator, ShortCircuitOperator
from airflow.providers.standard.operators.python import (
    BranchPythonOperator as ProviderBranchPythonOperator,
)

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


@task.branch()
def with_parens():  # AIR003
    if condition1:
        return ["downstream_task"]
    return []


@task.branch
def bare_return_and_list():  # AIR003
    if condition1:
        return ["downstream_task"]
    return


@task.branch
def none_return_and_list():  # AIR003
    if condition1:
        return ["downstream_task"]
    return None


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


# BranchPythonOperator violations (should trigger AIR003):


def operator_short_circuit_candidate():
    if condition1:
        return ["downstream_task"]
    return []


BranchPythonOperator(task_id="task", python_callable=operator_short_circuit_candidate)  # AIR003


def provider_short_circuit_candidate():
    if condition1:
        return ["downstream_task"]
    return []


ProviderBranchPythonOperator(  # AIR003
    task_id="task", python_callable=provider_short_circuit_candidate
)


# BranchPythonOperator non-violations:


def operator_two_non_empty():
    if condition1:
        return ["task_a"]
    if condition2:
        return ["task_b"]
    return []


BranchPythonOperator(task_id="task", python_callable=operator_two_non_empty)


def operator_single_return():
    return ["downstream_task"]


BranchPythonOperator(task_id="task", python_callable=operator_single_return)

ShortCircuitOperator(task_id="task", python_callable=operator_short_circuit_candidate)

BranchPythonOperator(task_id="task", python_callable=lambda: ["downstream_task"])

BranchPythonOperator(task_id="task")
