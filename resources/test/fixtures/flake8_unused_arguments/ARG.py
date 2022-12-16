from abc import abstractmethod
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
