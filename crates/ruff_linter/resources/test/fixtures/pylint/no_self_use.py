import abc

from typing_extensions import override


class Person:
    def developer_greeting(self, name):  # [no-self-use]
        print(f"Greetings {name}!")

    def greeting_1(self):  # [no-self-use]
        print("Hello!")

    def greeting_2(self):  # [no-self-use]
        print("Hi!")


# OK
def developer_greeting():
    print("Greetings developer!")


# OK
class Person:
    name = "Paris"

    def __init__(self):
        pass

    def __cmp__(self, other):
        print(24)

    def __repr__(self):
        return "Person"

    def func(self):
        ...

    def greeting_1(self):
        print(f"Hello from {self.name} !")

    @staticmethod
    def greeting_2():
        print("Hi!")


class Base(abc.ABC):
    """abstract class"""

    @abstractmethod
    def abstract_method(self):
        """abstract method could not be a function"""
        raise NotImplementedError


class Sub(Base):
    @override
    def abstract_method(self):
        print("concrete method")


class Prop:
    @property
    def count(self):
        return 24


class A:
    def foo(self):
        ...


class B(A):
    @override
    def foo(self):
        ...

    def foobar(self):
        super()

    def bar(self):
        super().foo()

    def baz(self):
        if super().foo():
            ...


# See: https://github.com/astral-sh/ruff/issues/12568
from attrs import define, field


@define
class Foo:
    x: int = field()
    y: int

    @x.validator
    def validate_x(self, attribute, value):
        if value <= 0:
            raise ValueError("x must be a positive integer")

    @y.validator
    def validate_y(self, attribute, value):
        if value <= 0:
            raise ValueError("y must be a positive integer")


class Foo:

    # No errors

    def string(self):
        msg = ""
        raise NotImplementedError(msg)

    def fstring(self, x):
        msg = f"{x}"
        raise NotImplementedError(msg)

    def docstring(self):
        """Lorem ipsum dolor sit amet."""
        msg = ""
        raise NotImplementedError(msg)


    # Errors

    def non_simple_assignment(self):
        msg = foo = ""
        raise NotImplementedError(msg)

    def non_simple_assignment_2(self):
        msg[0] = ""
        raise NotImplementedError(msg)

    def unused_message(self):
        msg = ""
        raise NotImplementedError("")

    def unused_message_2(self, x):
        msg = ""
        raise NotImplementedError(x)

class TPerson:
    def developer_greeting(self, name):  # [no-self-use]
        print(t"Greetings {name}!")

    def greeting_1(self):
        print(t"Hello from {self.name} !")

    def tstring(self, x):
        msg = t"{x}"
        raise NotImplementedError(msg)

