from abc import abstractmethod
from typing import overload
from typing_extensions import override


###
# Unused arguments on functions.
###
def f(self, x):
    print("Hello, world!")


def f(cls, x):
    print("Hello, world!")


def f(self, x):
    ...


def f(cls, x):
    ...


###
# Unused arguments on lambdas.
###
lambda x: print("Hello, world!")


class C:
    ###
    # Unused arguments.
    ###
    def f(self, x):
        print("Hello, world!")

    def f(self, /, x):
        print("Hello, world!")

    def f(cls, x):
        print("Hello, world!")

    @classmethod
    def f(cls, x):
        print("Hello, world!")

    @staticmethod
    def f(cls, x):
        print("Hello, world!")

    @staticmethod
    def f(x):
        print("Hello, world!")

    ###
    # Unused arguments attached to empty functions (OK).
    ###
    def f(self, x):
        ...

    def f(self, /, x):
        ...

    def f(cls, x):
        ...

    @classmethod
    def f(cls, x):
        ...

    @staticmethod
    def f(cls, x):
        ...

    @staticmethod
    def f(x):
        ...

    def f(self, x):
        """Docstring."""

    def f(self, x):
        """Docstring."""
        ...

    def f(self, x):
        pass

    def f(self, x):
        raise NotImplementedError

    def f(self, x):
        raise NotImplementedError()

    def f(self, x):
        raise NotImplementedError("...")

    def f(self, x):
        raise NotImplemented

    def f(self, x):
        raise NotImplemented()

    def f(self, x):
        raise NotImplemented("...")

    ###
    # Unused functions attached to abstract methods (OK).
    ###
    @abstractmethod
    def f(self, x):
        print("Hello, world!")

    @abstractmethod
    def f(self, /, x):
        print("Hello, world!")

    @abstractmethod
    def f(cls, x):
        print("Hello, world!")

    @classmethod
    @abstractmethod
    def f(cls, x):
        print("Hello, world!")

    @staticmethod
    @abstractmethod
    def f(cls, x):
        print("Hello, world!")

    @staticmethod
    @abstractmethod
    def f(x):
        print("Hello, world!")

    ###
    # Unused functions attached to overrides (OK).
    ###
    @override
    def f(self, x):
        print("Hello, world!")

    @override
    def f(self, /, x):
        print("Hello, world!")

    @override
    def f(cls, x):
        print("Hello, world!")

    @classmethod
    @override
    def f(cls, x):
        print("Hello, world!")

    @staticmethod
    @override
    def f(cls, x):
        print("Hello, world!")

    @staticmethod
    @override
    def f(x):
        print("Hello, world!")


###
# Unused arguments attached to overloads (OK).
###
@overload
def f(a: str, b: str) -> str:
    ...


@overload
def f(a: int, b: int) -> str:
    ...


def f(a, b):
    return f"{a}{b}"


###
# Unused arguments on magic methods.
###
class C:
    def __init__(self, x) -> None:
        print("Hello, world!")

    def __str__(self) -> str:
        return "Hello, world!"

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        print("Hello, world!")
