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


import abc
import enum
import types
from functools import cached_property


class BaseCache2:
    @cached_property
    def prop(self) -> None:
        print("Property not found")
        return None

    @abc.abstractproperty
    def prop2(self) -> None:
        return None

    @types.DynamicClassAttribute
    def prop3(self) -> None:
        return None

    @enum.property
    def prop4(self) -> None:
        return None
