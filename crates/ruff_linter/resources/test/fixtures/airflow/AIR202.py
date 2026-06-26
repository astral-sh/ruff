from __future__ import annotations

import collections
import collections.abc
import typing
from typing import Dict, Mapping, MutableMapping, TypedDict

from airflow.decorators import task
from airflow.sdk import task as sdk_task


# --- Annotation path: should flag (fix → multiple_outputs=True) ---


@task
def annotated_dict() -> dict:  # AIR202
    return {"x": 1}


@task()
def annotated_dict_subscript() -> dict[str, int]:  # AIR202
    return {"x": 1}


@task
def annotated_typing_dict() -> Dict[str, int]:  # AIR202
    return {"x": 1}


@task
def annotated_typing_mapping() -> Mapping[str, int]:  # AIR202
    return {"x": 1}


@task
def annotated_collections_abc_mapping() -> collections.abc.Mapping:  # AIR202
    return {"x": 1}


@task
def annotated_mutable_mapping() -> MutableMapping[str, int]:  # AIR202
    return {"x": 1}


class MyTD(TypedDict):
    x: int


@task
def annotated_typed_dict() -> MyTD:  # AIR202
    return {"x": 1}


@sdk_task
def annotated_dict_via_sdk() -> dict:  # AIR202
    return {"x": 1}


# --- Body path: should flag (fix → multiple_outputs=False) ---


@task
def body_dict_literal():  # AIR202
    return {"x": 1}


@task
def body_dict_comp():  # AIR202
    return {k: v for k, v in enumerate(range(3))}


@task
def body_dict_call():  # AIR202
    return dict(a=1)


@task
def body_dict_literal_annotated_non_mapping() -> int:  # AIR202
    return {"x": 1}


# --- Decorator call-form coverage: should flag ---


@task(retries=3)
def call_form_with_kwarg() -> dict:  # AIR202
    return {"x": 1}


@task(
    retries=3,
)
def call_form_multiline() -> dict:  # AIR202
    return {"x": 1}


# --- Variant decorators: should flag ---


@task.branch
def variant_branch():  # AIR202
    return {"a": 1}


@task.short_circuit
def variant_short_circuit():  # AIR202
    return {"a": 1}


@task.virtualenv
def variant_virtualenv() -> dict:  # AIR202
    return {"a": 1}


@task.docker(image="python:3.11")
def variant_docker() -> Mapping[str, int]:  # AIR202
    return {"a": 1}


@task.kubernetes
def variant_kubernetes():  # AIR202
    return {"a": 1}


@task.short_circuit(trigger_rule="all_done")
def variant_short_circuit_call() -> dict:  # AIR202
    return {"a": 1}


# --- Negative cases: should NOT flag ---


@task(multiple_outputs=True)
def explicit_true() -> dict:
    return {"x": 1}


@task(multiple_outputs=False)
def explicit_false() -> dict:
    return {"x": 1}


@task
def no_return_no_annotation():
    print("hi")


@task
def returns_scalar():
    return 1


@task
def returns_list() -> list[int]:
    return [1, 2]


@task
def returns_tuple() -> tuple[int, int]:
    return (1, 2)


@task
def returns_none() -> None:
    return None


@task.sensor
def sensor_returning_dict():
    return {"a": 1}


def plain_function_returning_dict() -> dict:
    return {"x": 1}


# --- Nested-scope negatives: dict returns inside nested defs/classes should NOT flag ---


@task
def nested_function_returning_dict():  # should NOT flag: dict returned by inner fn, not outer
    def inner():
        return {"x": 1}

    return inner()


@task
def nested_class_method_returning_dict():  # should NOT flag: dict returned by inner class method
    class Inner:
        def make(self):
            return {"x": 1}

    return Inner().make()
