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
