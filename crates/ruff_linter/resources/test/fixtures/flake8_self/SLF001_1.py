from __future__ import annotations

from typing import Annotated

# https://github.com/astral-sh/ruff/issues/9022

class Lorem[T]:
    def f(self):
        lorem_1 = Lorem()
        lorem_1._value = 1  # fine

        lorem_2 = Lorem[bytes]()
        lorem_2._value = 1  # fine


class Ipsum:
    def __new__(cls):
        instance = super().__new__(cls)
        instance._value = 1  # fine


class Dolor[T]:
    def f(
        self,
        a: Dolor,
        b: Dolor[int],
        c: Annotated[Dolor, ...],
        d: Annotated[Dolor[str], ...]
    ):
        a._value = 1  # fine
        b._value = 1  # fine
        c._value = 1  # fine
        d._value = 1  # fine

    @classmethod
    def m(cls):
        instance = cls()
        instance._value = 1  # fine


class M(type):
    @classmethod
    def f(mcs):
        cls = mcs()
        cls._value = 1
