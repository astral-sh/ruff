import abc
import ast
from abc import abstractmethod, ABCMeta
from ast import Name


# marked as exempt base class
class A0:
    pass


# not exempt base class
class A1:
    pass


# Errors

class B0(A1, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class B1(ast.Param, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class B2(A1, ast.Param, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


# OK

class C0(A0, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class C1(ast.Name, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class C2(Name, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class C3(ast.Param, A0, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class C4(A0, ast.Param, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class C5(ast.Name, A1, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class C6(A1, ast.Name, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class C6(Name, A1, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class C7(A1, Name, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass


class C8(A0, Name, metaclass=abc.ABCMeta):
    @abstractmethod
    def foo(self): pass
