# Regression test for https://github.com/astral-sh/ruff/issues/19632
# When @overload is imported from a custom module listed in typing-modules,
# Ruff should recognize it as typing.overload and not emit false F811
# diagnostics for each overloaded function definition.
#
# Requires: typing-modules = ["std"]
from std import overload


@overload
def func(a: str, b: int) -> int: ...


@overload
def func(a: int, b: str) -> int: ...


def func(a: int | str, b: int | str) -> int:
    return 0
