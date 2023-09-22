"""
Should emit:
B027 - on lines 13, 16, 19, 23
"""
import abc
from abc import ABC
from abc import (
    abstractmethod,
    abstractproperty,
    abstractclassmethod,
    abstractstaticmethod,
)
from abc import abstractmethod as notabstract
from abc import abstractproperty as notabstract_property


class AbstractClass(ABC):
    def empty_1(self):  # error
        ...

    def empty_2(self):  # error
        pass

    def empty_3(self):  # error
        """docstring"""
        ...

    def empty_4(self):  # error
        """multiple ellipsis/pass"""
        ...
        pass
        ...
        pass

    @notabstract
    def abstract_0(self):
        ...

    @abstractmethod
    def abstract_1(self):
        ...

    @abstractmethod
    def abstract_2(self):
        pass

    @abc.abstractmethod
    def abstract_3(self):
        ...

    @abc.abstractproperty
    def abstract_4(self):
        ...

    @abstractproperty
    def abstract_5(self):
        ...

    @notabstract_property
    def abstract_6(self):
        ...

    @abstractclassmethod
    def abstract_7(self):
        pass

    @abc.abstractclassmethod
    def abstract_8(self):
        ...

    @abstractstaticmethod
    def abstract_9(self):
        pass

    @abc.abstractstaticmethod
    def abstract_10(self):
        ...

    def body_1(self):
        print("foo")
        ...

    def body_2(self):
        self.body_1()


class NonAbstractClass:
    def empty_1(self):  # safe
        ...

    def empty_2(self):  # safe
        pass


# ignore @overload, fixes issue #304
# ignore overload with other imports, fixes #308
import typing
import typing as t
import typing as anything
from typing import Union, overload


class AbstractClass(ABC):
    @overload
    def empty_1(self, foo: str):
        ...

    @typing.overload
    def empty_1(self, foo: int):
        ...

    @t.overload
    def empty_1(self, foo: list):
        ...

    @anything.overload
    def empty_1(self, foo: float):
        ...

    @abstractmethod
    def empty_1(self, foo: Union[str, int, list, float]):
        ...


from dataclasses import dataclass


@dataclass
class Foo(ABC):  # noqa: B024
    ...
