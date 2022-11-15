"""
Should emit:
B024 - on lines 17, 34, 52, 58, 69, 74, 79, 84, 89
"""

import abc
import abc as notabc
from abc import ABC, ABCMeta
from abc import abstractmethod
from abc import abstractmethod as abstract
from abc import abstractmethod as abstractaoeuaoeuaoeu
from abc import abstractmethod as notabstract

import foo


class Base_1(ABC):  # error
    def method(self):
        foo()


class Base_2(ABC):
    @abstractmethod
    def method(self):
        foo()


class Base_3(ABC):
    @abc.abstractmethod
    def method(self):
        foo()


class Base_4(ABC):
    @notabc.abstractmethod
    def method(self):
        foo()


class Base_5(ABC):
    @abstract
    def method(self):
        foo()


class Base_6(ABC):
    @abstractaoeuaoeuaoeu
    def method(self):
        foo()


class Base_7(ABC):  # error
    @notabstract
    def method(self):
        foo()


class MetaBase_1(metaclass=ABCMeta):  # error
    def method(self):
        foo()


class MetaBase_2(metaclass=ABCMeta):
    @abstractmethod
    def method(self):
        foo()


class abc_Base_1(abc.ABC):  # error
    def method(self):
        foo()


class abc_Base_2(metaclass=abc.ABCMeta):  # error
    def method(self):
        foo()


class notabc_Base_1(notabc.ABC):  # error
    def method(self):
        foo()


class multi_super_1(notabc.ABC, abc.ABCMeta):  # safe
    def method(self):
        foo()


class multi_super_2(notabc.ABC, metaclass=abc.ABCMeta):  # safe
    def method(self):
        foo()


class non_keyword_abcmeta_1(ABCMeta):  # safe
    def method(self):
        foo()


class non_keyword_abcmeta_2(abc.ABCMeta):  # safe
    def method(self):
        foo()


# very invalid code, but that's up to mypy et al to check
class keyword_abc_1(metaclass=ABC):  # safe
    def method(self):
        foo()


class keyword_abc_2(metaclass=abc.ABC):  # safe
    def method(self):
        foo()


class abc_set_class_variable_1(ABC):  # safe
    foo: int


class abc_set_class_variable_2(ABC):  # safe
    foo = 2


class abc_set_class_variable_3(ABC):  # safe
    foo: int = 2


# this doesn't actually declare a class variable, it's just an expression
class abc_set_class_variable_4(ABC):  # error
    foo
