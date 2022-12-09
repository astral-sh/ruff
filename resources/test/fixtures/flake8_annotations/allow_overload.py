from typing import overload


@overload
def foo(i: int) -> "int":
    ...


@overload
def foo(i: "str") -> "str":
    ...


def foo(i):
    return i


@overload
def bar(i: int) -> "int":
    ...


@overload
def bar(i: "str") -> "str":
    ...


class X:
    def bar(i):
        return i


# TODO(charlie): This third case should raise an error (as in Mypy), because we have a
# statement between the interfaces and implementation.
@overload
def baz(i: int) -> "int":
    ...


@overload
def baz(i: "str") -> "str":
    ...


x = 1


def baz(i):
    return i
