import abc
from abc import (
    abstractclassmethod,
    abstractproperty,
    abstractstaticmethod,
)


class Foo:
    @abc.abstractclassmethod
    def from_module_classmethod(cls): ...

    @abstractclassmethod
    def imported_classmethod(cls): ...

    @abc.abstractstaticmethod
    def from_module_staticmethod(): ...

    @abstractstaticmethod
    def imported_staticmethod(): ...

    @abc.abstractproperty
    def from_module_property(self): ...

    @abstractproperty
    def imported_property(self): ...


class Bar:
    @abc.abstractmethod
    @classmethod
    def already_modern(cls): ...

