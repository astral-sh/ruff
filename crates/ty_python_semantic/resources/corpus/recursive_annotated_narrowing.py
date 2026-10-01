# Regression test for https://github.com/astral-sh/ty/issues/3837.

from typing import Annotated, TypeIs, reveal_type


class Container[T]: ...


def is_container[T](value: object, other: T) -> TypeIs[Container[T]]:
    return True


value = int
while True:
    if is_container(value, value):
        value = Annotated[list[value], "metadata"]
    else:
        value = {value}
    reveal_type(value)
