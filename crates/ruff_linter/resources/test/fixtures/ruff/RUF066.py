import abc
import typing


class User:  # Test normal class properties
    @property
    def name(self):  # ERROR: No return
        f"{self.first_name} {self.last_name}"

    @property
    def age(self):  # OK: Returning something
        return 100

    def method(self):  # OK: Not a property
        x = 1

    @property
    def nested(self):  # ERROR: Property itself doesn't return
        def inner():
            return 0

    @property
    def stub(self): ...  # OK: A stub; doesn't return anything


class UserMeta(metaclass=abc.ABCMeta):  # Test properies inside of an ABC class
    @property
    @abc.abstractmethod
    def abstr_prop1(self): ...  # OK: Abstract methods doesn't need to return anything

    @property
    @abc.abstractmethod
    def abstr_prop2(self):  # OK: Abstract methods doesn't need to return anything
        """
        A cool docstring
        """

    @property
    def prop1(self):  # OK: Returning a value
        return 1

    @property
    def prop2(self):  # ERROR: Not returning something (even when we are inside an ABC)
        50

    def method(self):  # OK: Not a property
        x = 1


def func():  # OK: Not a property
    x = 1


class Proto(typing.Protocol):  # Tests for a Protocol class
    @property
    def prop1(self) -> int: ...  # OK: A stub property


class File:  # Extra tests for things like yield/yield from/raise
    @property
    def stream1(self):  # OK: Yields something
        yield

    @property
    def stream2(self):  # OK: Yields from something
        yield from self.stream1

    @property
    def children(self):  # OK: Raises
        raise ValueError("File does not have children")
