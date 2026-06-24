# Regression corpus for recursive inference growth from issues #3827 and #3837.

from collections.abc import Callable
from functools import partial
from typing import Annotated, TypeIs


class Container[T]: ...
class Other[T]: ...


def identity[T](value: T) -> T:
    return value


def is_container[T](x: object, y: T) -> TypeIs[Container[T]]:
    return True


def is_container_or_other[T](x: object, y: T) -> TypeIs[Container[T] | Other[T]]:
    return True


def recursive_iterator_growth():
    while True:
        x = iter([[None] + [x]])


def recursive_narrowing_distribution():
    while True:
        if is_container(x, type(x)):
            x = [x]
        else:
            x = {x}


def recursive_narrowing_distribution_inside_a_nested_wrapper():
    while True:
        if is_container(x, type(x)):
            x = [[x]]
        else:
            x = {x}


def recursive_narrowing_distribution_inside_a_tuple():
    while True:
        if is_container(x, type(x)):
            x = (x,)
        else:
            x = {x}


def recursive_narrowing_distribution_inside_a_tuple_with_a_stable_element():
    while True:
        if is_container(x, type(x)):
            x = (0, x)
        else:
            x = {x}


def recursive_narrowing_distribution_in_multiple_tuple_elements():
    while True:
        if is_container(x, type(x)):
            x = (x, x)
        else:
            x = {x}


def recursive_narrowing_distribution_with_an_initialized_value():
    x = 1
    while True:
        if is_container(x, type(x)):
            x = [x]
        else:
            x = {x}


def recursive_narrowing_with_a_stable_previous_arm():
    x = 0
    while True:
        if is_container(x, x):
            x = [x]
        else:
            x = x


def recursive_narrowing_distribution_with_multiple_targets():
    while True:
        if is_container_or_other(y, type(y)):
            y = [y]
        else:
            y = {y}


def recursive_narrowing_with_multiple_growing_frontiers():
    def test(flag: bool):
        x = 0
        while True:
            if is_container(x, x):
                x = [x] if flag else (x,)
            else:
                x = x


def recursive_narrowing_distribution_through_a_bound_method():
    while True:
        if is_container(x, type(x)):
            x = x.__str__
        else:
            x = {x}


def recursive_narrowing_through_a_negative_bound_method():
    x = 0
    while True:
        if is_container(x, x):
            x = [x]
        else:
            x = x.__str__


def recursive_narrowing_through_a_precise_partial():
    x = 0
    while True:
        if is_container(x, x):
            x = [x]
        else:
            x = partial(identity, x)


def recursive_type_form_growth_with_a_stable_bound_method():
    x = int
    while True:
        if is_container(x, x):
            x = list[x]
        else:
            x = x.__str__


def recursive_narrowing_distribution_through_a_generator_expression():
    x = 0
    while True:
        if is_container(x, x):
            x = (element for element in (x,))
        else:
            x = [x]


def recursive_narrowing_distribution_through_a_generator_expression_and_tuple():
    x = 0
    while True:
        if is_container(x, x):
            x = (element for element in (x,))
        else:
            x = (x,)


def recursive_narrowing_distribution_through_type():
    x = 0
    while True:
        if is_container(x, type(x)):
            x = (element for element in (x,))
        else:
            x = [x]


def recursive_narrowing_through_a_runtime_generic_alias():
    x = int
    while True:
        if is_container(x, type(x)):
            x = list[x]
        else:
            x = {x}


def recursive_narrowing_through_a_tuple_generic_alias():
    x = int
    while True:
        if is_container(x, type(x)):
            x = tuple[x]
        else:
            x = {x}


def recursive_narrowing_through_a_runtime_callable():
    x = int
    while True:
        if is_container(x, type(x)):
            x = Callable[[], x]
        else:
            x = {x}


def recursive_narrowing_through_multiple_runtime_type_forms():
    x = int
    while True:
        if is_container(x, x):
            x = list[x]
        else:
            x = Callable[[], x]


def recursive_narrowing_through_same_origin_runtime_type_forms():
    x = int
    while True:
        if is_container(x, x):
            x = list[x]
        else:
            x = list[list[x]]


def recursive_narrowing_through_a_runtime_union():
    x = int
    while True:
        if is_container(x, x):
            x = list[x] | str
        else:
            x = {x}


def recursive_narrowing_through_a_runtime_annotated_value():
    x = int
    while True:
        if is_container(x, x):
            x = Annotated[list[x], "metadata"]
        else:
            x = {x}
