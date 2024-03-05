import abc
from abc import abstractmethod, ABCMeta


# Errors

class A0(metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class A1(metaclass=ABCMeta):
    @abstractmethod
    def foo(self): pass


class B0:
    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__()


class B1:
    pass


class A2(B0, B1, metaclass=ABCMeta):
    @abstractmethod
    def foo(self): pass


class A3(B0, before_metaclass=1, metaclass=abc.ABCMeta):
    pass


# OK

class Meta(type):
    def __new__(cls, *args, **kwargs):
        return super().__new__(cls, *args)


class A4(metaclass=Meta, no_metaclass=ABCMeta):
    @abstractmethod
    def foo(self): pass


class A5(metaclass=Meta):
    pass


class A6(abc.ABC):
    @abstractmethod
    def foo(self): pass


class A7(B0, abc.ABC, B1):
    @abstractmethod
    def foo(self): pass
