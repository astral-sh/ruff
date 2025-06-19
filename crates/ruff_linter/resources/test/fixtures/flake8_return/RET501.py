def x(y):
    if not y:
        return
    return None  # error


class BaseCache:
    def get(self, key: str) -> str | None:
        print(f"{key} not found")
        return None

    def get(self, key: str) -> None:
        print(f"{key} not found")
        return None

    @property
    def prop(self) -> None:
        print("Property not found")
        return None


from functools import cached_property


class BaseCache2:
    @cached_property
    def prop(self) -> None:
        print("Property not found")
        return None


import abc
import enum
import types


class Baz:
    @abc.abstractproperty
    def prop2(self) -> None:
        print("Override me")
        return None

    @types.DynamicClassAttribute
    def prop3(self) -> None:
        print("Gotta make this a multiline function for it to be a meaningful test")
        return None

    @enum.property
    def prop4(self) -> None:
        print("I've run out of things to say")
        return None


# https://github.com/astral-sh/ruff/issues/18774
class _:
    def foo(bar):
        if not bar:
            return
        return (
            None # comment
        )
